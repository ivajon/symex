#![allow(dead_code, missing_docs)]
use std::{
    fmt::Display,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use gimli::{DebugAbbrev, DebugInfo, DebugStr};
use hashbrown::HashMap;
use object::{File, Object, ObjectSection, ObjectSymbol};
use tracing::{debug, trace};

use crate::{
    arch::Architecture,
    executor::hooks::HookContainer,
    project::{dwarf_helper::SubProgramMap, segments::Segments, ProjectError},
    smt::SmtSolver,
    Composition,
    Endianness,
    WordSize,
};

pub mod run_config;

mod sealed {
    use crate::{arch::Architecture, smt::SmtSolver};

    pub trait ArchOverride: Architecture {
        fn override_architecture() -> Self;
    }
    pub trait SmtSolverConfigured {}
    pub trait BinaryLoadingDone {}
}
use sealed::*;

pub struct SmtConfigured<Smt: SmtSolver> {
    smt: Smt,
}

pub struct SmtNotConfigured;

pub struct BinaryLoaded<'file> {
    object: object::File<'file>,
}

pub struct BinaryNotLoaded;

#[derive(Debug, Clone)]
struct NoArchOverride;

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
    fn new(path: &'str str) -> Self {
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
    pub fn override_architecture<A: ArchOverride>(self) -> SymexConstructor<'str, A, S, B> {
        SymexConstructor::<'str, A, S, B> {
            file: self.file,
            override_arch: A::override_architecture(),
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

impl<'str, 'file, A: Architecture, S: SmtSolver>
    SymexConstructor<'str, A, SmtConfigured<S>, BinaryLoaded<'file>>
{
    fn compose<C: Composition>(
        self,
        user_state: C::StateContainer,
        logger: C::Logger,
    ) -> crate::Result<todo!()> {
        let binary = self.binary_file.object;
        let smt = self.smt.smt;
        let a = self.override_arch;

        let segments = Segments::from_file(&binary);
        let endianness = if binary.is_little_endian() {
            Endianness::Little
        } else {
            Endianness::Big
        };

        // Do not catch 16 or 8 bit architectures but will do for now.
        let word_size = if binary.is_64() {
            WordSize::Bit64
        } else {
            WordSize::Bit32
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

        let map = SubProgramMap::new(&debug_info, &debug_abbrev, &debug_str);
        let hooks = HookContainer::default(&map)?;

        todo!()
    }
}

pub struct SymexArbiter<C: Composition> {
    logger: C::Logger,
}

impl Display for NoArchOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Not overriding architecture")
    }
}

impl<A: Architecture> ArchOverride for A {
    fn override_architecture() -> Self {
        Self::new()
    }
}
impl Architecture for NoArchOverride {
    fn translate<C: Composition<Architecture = Self>>(
        &self,
        _buff: &[u8],
        _state: &crate::executor::state::GAState2<C>,
    ) -> Result<crate::executor::instruction::Instruction2<C>, crate::arch::ArchError> {
        unimplemented!("NoArchOverride is not an architecture");
    }

    fn add_hooks<C: Composition<Architecture = Self>>(
        &self,
        _cfg: &mut crate::executor::hooks::HookContainer<C>,
        _sub_program_lookup: &mut crate::project::dwarf_helper::SubProgramMap,
    ) {
        unimplemented!("NoArchOverride is not an architecture");
    }

    fn discover(_file: &File<'_>) -> Result<Option<Self>, crate::arch::ArchError> {
        unimplemented!("NoArchOverride is not an architecture");
    }

    fn new() -> Self
    where
        Self: Sized,
    {
        Self
    }
}

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
