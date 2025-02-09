#![allow(dead_code, missing_docs)]
use std::{fmt::Display, path::PathBuf};

use gimli::{DebugAbbrev, DebugInfo, DebugStr};
use hashbrown::HashMap;
use object::{Object, ObjectSection, ObjectSymbol};
use tracing::debug;

use crate::{
    arch::SupportedArchitecture,
    defaults::boolector::UserState,
    executor::hooks::{HookContainer, PCHook2, UserStateContainer},
    logging::NoLogger,
    manager::SymexArbiter,
    project::{dwarf_helper::SubProgramMap, Project, ProjectError},
    smt::{SmtMap, SmtSolver},
    Composition,
    Endianness,
};

//pub mod run_config;

mod sealed {

    pub trait ArchOverride {}
    pub trait SmtSolverConfigured {}
    pub trait BinaryLoadingDone {}
}
use sealed::*;

#[doc(hidden)]
pub struct SmtConfigured<Smt: SmtSolver> {
    smt: Smt,
}

#[doc(hidden)]
pub struct SmtNotConfigured;

#[doc(hidden)]
pub struct BinaryLoaded<'file> {
    object: object::File<'file>,
}

#[doc(hidden)]
pub struct BinaryNotLoaded;

#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct NoArchOverride;

#[doc(hidden)]
pub struct SymexConstructor<
    'str,
    Override: ArchOverride,
    Smt: SmtSolverConfigured,
    Binary: BinaryLoadingDone,
> {
    file: &'str str,
    override_arch: Override,
    smt: Smt,
    binary_file: Binary,
}

impl<'str> SymexConstructor<'str, NoArchOverride, SmtNotConfigured, BinaryNotLoaded> {
    pub const fn new(path: &'str str) -> Self {
        Self {
            file: path,
            override_arch: NoArchOverride,
            smt: SmtNotConfigured,
            binary_file: BinaryNotLoaded,
        }
    }
}

impl<'str, S: SmtSolverConfigured, B: BinaryLoadingDone>
    SymexConstructor<'str, NoArchOverride, S, B>
{
    pub fn override_architecture<A: Into<SupportedArchitecture>>(
        self,
        a: A,
    ) -> SymexConstructor<'str, SupportedArchitecture, S, B> {
        SymexConstructor::<'str, SupportedArchitecture, S, B> {
            file: self.file,
            override_arch: a.into(),
            smt: self.smt,
            binary_file: self.binary_file,
        }
    }
}

impl<'str, A: ArchOverride, B: BinaryLoadingDone> SymexConstructor<'str, A, SmtNotConfigured, B> {
    pub fn configure_smt<S: SmtSolver>(self) -> SymexConstructor<'str, A, SmtConfigured<S>, B> {
        SymexConstructor {
            file: self.file,
            override_arch: self.override_arch,
            smt: SmtConfigured::<S> { smt: S::new() },
            binary_file: self.binary_file,
        }
    }
}

impl<'str, 'file, A: ArchOverride, S: SmtSolverConfigured>
    SymexConstructor<'str, A, S, BinaryNotLoaded>
{
    pub fn load_binary(self) -> crate::Result<SymexConstructor<'str, A, S, BinaryLoaded<'file>>> {
        let file = std::fs::read(self.file)
            .map_err(|e| crate::GAError::CouldNotOpenFile(e.to_string()))?;
        let data = &(*file.leak());
        let obj_file = match object::File::parse(data) {
            Ok(x) => x,
            Err(e) => {
                debug!("Error: {}", e);
                let mut ret = PathBuf::new();
                ret.push(self.file);

                return Err(crate::GAError::ProjectError(
                    ProjectError::UnableToParseElf(ret.display().to_string()),
                ))?;
            }
        };
        Ok(SymexConstructor {
            file: self.file,
            override_arch: self.override_arch,
            smt: self.smt,
            binary_file: BinaryLoaded { object: obj_file },
        })
    }
}

impl<'str, 'file, S: SmtSolverConfigured>
    SymexConstructor<'str, NoArchOverride, S, BinaryLoaded<'file>>
{
    pub fn discover(
        self,
    ) -> crate::Result<SymexConstructor<'str, SupportedArchitecture, S, BinaryLoaded<'file>>> {
        let arch = SupportedArchitecture::discover(&self.binary_file.object)?;

        Ok(SymexConstructor {
            file: self.file,
            override_arch: arch,
            smt: self.smt,
            binary_file: self.binary_file,
        })
    }
}

impl<'str, 'file, S: SmtSolver>
    SymexConstructor<'str, SupportedArchitecture, SmtConfigured<S>, BinaryLoaded<'file>>
{
    pub fn compose<
        C: Composition,
        StateCreator: FnOnce() -> C::StateContainer,
        LoggingCreator: FnOnce(&SubProgramMap) -> C::Logger,
    >(
        self,
        user_state_composer: StateCreator,
        logger: LoggingCreator,
    ) -> crate::Result<SymexArbiter<C>>
    where
        C::Memory: SmtMap<ProgramMemory = &'static Project>,
        C: Composition<SMT = S>,
        //C: Composition<StateContainer = Box<A>>,
    {
        let binary = self.binary_file.object;
        let smt = self.smt.smt;

        let endianness = if binary.is_little_endian() {
            Endianness::Little
        } else {
            Endianness::Big
        };

        let mut symtab = HashMap::new();
        for symbol in binary.symbols() {
            symtab.insert(
                match symbol.name() {
                    Ok(name) => name.to_owned(),
                    Err(_) => continue, // Ignore entry if name can not be read
                },
                symbol.address(),
            );
        }

        let gimli_endian = match endianness {
            Endianness::Little => gimli::RunTimeEndian::Little,
            Endianness::Big => gimli::RunTimeEndian::Big,
        };

        let debug_info = binary.section_by_name(".debug_info").unwrap();
        let debug_info = DebugInfo::new(debug_info.data().unwrap(), gimli_endian);

        let debug_abbrev = binary.section_by_name(".debug_abbrev").unwrap();
        let debug_abbrev = DebugAbbrev::new(debug_abbrev.data().unwrap(), gimli_endian);

        let debug_str = binary.section_by_name(".debug_str").unwrap();
        let debug_str = DebugStr::new(debug_str.data().unwrap(), gimli_endian);

        let mut map = SubProgramMap::new(&debug_info, &debug_abbrev, &debug_str);
        let mut hooks = HookContainer::default(&map)?;
        self.override_arch.add_hooks(&mut hooks, &mut map);

        let project = Box::new(Project::from_binary(binary, symtab)?);
        let project = Box::leak(project);

        Ok(SymexArbiter::<C>::new(
            logger(&map),
            project,
            smt,
            user_state_composer(),
            hooks,
            map,
            self.override_arch,
        ))
    }
}

impl Display for NoArchOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[derive(Debug, Clone)]
        struct SomeUserState {
            sp_writes: Vec<u64>,
        }

        impl UserStateContainer for SomeUserState {}

        impl SomeUserState {
            fn new() -> Self {
                Self {
                    sp_writes: Vec::new(),
                }
            }
        }

        let mut symex: SymexArbiter<UserState<SomeUserState>> = SymexConstructor::new("asd")
            .load_binary()
            .unwrap()
            .discover()
            .unwrap()
            //.override_architecture::<crate::arch::arm::v7::ArmV7EM>()
            .configure_smt()
            .compose(|| SomeUserState::new(), |_| NoLogger)
            .unwrap();

        let _res = symex.add_hooks(|hook_container, lookup| {
            hook_container.add_pc_hook(0x1234, PCHook2::Continue);
            hook_container
                .add_pc_hook_regex(
                    lookup,
                    r"some_function",
                    PCHook2::Intrinsic(|_state| Ok(())),
                )
                .expect("Initiation failed!");

            // Get the stack pointer writes.
            hook_container.add_register_write_hook("SP".to_string(), |state, value| {
                let value_const = value.get_constant().unwrap();
                state.state.sp_writes.push(value_const);
                state.memory.set_register("SP", value)?;
                Ok(())
            });
        });
        let _path_result = symex.run("Some_function");

        write!(f, "Not overriding architecture")
    }
}

impl ArchOverride for SupportedArchitecture {}
impl ArchOverride for NoArchOverride {}

impl SmtSolverConfigured for SmtNotConfigured {}

impl<S: SmtSolver> SmtSolverConfigured for SmtConfigured<S> {}

impl BinaryLoadingDone for BinaryNotLoaded {}
impl<'file> BinaryLoadingDone for BinaryLoaded<'file> {}

//let context = Box::new(DContext::new());
//    let context = Box::leak(context);
//
//    let end_pc = 0xFFFFFFFE;
//
//    debug!("Parsing elf file: {}", path);
//    let file = fs::read(path).expect("Unable to open file.");
//    let data = file.as_ref();
//    let obj_file = match object::File::parse(data) {
//        Ok(x) => x,
//        Err(e) => {
//            debug!("Error: {}", e);
//            return Err(ProjectError::UnableToParseElf(path.to_owned()))?;
//        }
//    };
//
//    add_architecture_independent_hooks(&mut cfg);
//    let project = Box::new(Project::from_path(&mut cfg, obj_file,
// &architecture)?);    let project = Box::leak(project);
//    project.add_pc_hook(end_pc, PCHook::EndSuccess);
//    debug!("Created project: {:?}", project);
//
//    let mut vm = VM::new(project, context, function, end_pc, architecture)?;
//    run_elf_paths(&mut vm, &cfg)
