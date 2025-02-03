//! Describes the VM for general assembly

use general_assembly::prelude::Operand;
use hashbrown::HashMap;
use kd_tree::KdMap;

use super::{
    state::{GAState, GAState2},
    GAExecutor,
    PathResult,
};
use crate::{
    arch::Arch,
    path_selection::{DFSPathSelection, Path},
    project::Project,
    smt::{DContext, DSolver, MemoryError, SmtExpr, SmtMap, SmtSolver},
    GAError,
    Result,
    WordSize,
};

#[derive(Debug)]
pub struct VM<A: Arch> {
    pub project: &'static Project<A>,
    pub paths: DFSPathSelection<A>,
}

impl<A: Arch> VM<A> {
    pub fn new(
        project: &'static Project<A>,
        ctx: &'static DContext,
        fn_name: &str,
        end_pc: u64,
        architecture: A,
    ) -> Result<Self> {
        let mut vm = Self {
            project,
            paths: DFSPathSelection::new(),
        };

        let solver = DSolver::new(ctx);
        let state = GAState::<A>::new(ctx, project, solver, fn_name, end_pc, architecture)?;

        vm.paths.save_path(Path::new(state, None));

        Ok(vm)
    }

    pub fn new_with_state(project: &'static Project<A>, state: GAState<A>) -> Self {
        let mut vm = Self {
            project,
            paths: DFSPathSelection::new(),
        };

        vm.paths.save_path(Path::new(state, None));

        vm
    }

    pub fn run(&mut self) -> Result<Option<(PathResult, GAState<A>)>> {
        if let Some(path) = self.paths.get_path() {
            // try stuff
            let mut executor = GAExecutor::from_state(path.state, self, self.project);

            for constraint in path.constraints {
                executor.state.constraints.assert(&constraint);
            }

            let result = executor.resume_execution()?;
            return Ok(Some((result, executor.state)));
        }
        Ok(None)
    }
}

/// Represents a generic state container.
pub trait StateContainer {
    type Architecture: Arch;

    #[must_use]
    /// Returns the underlying architechture.
    fn as_arch(&mut self) -> &mut Self::Architecture;
}

pub trait Composition {
    type StateContainer: StateContainer;
    type SMT: SmtSolver;
}
//pub type PCHooks2<A> = HashMap<u64, PCHook<A>>;
//
///// Hook for a register read.
//pub type RegisterReadHook2<A> = fn(state: &mut GAState<A>) ->
// super::Result<DExpr>; pub type RegisterReadHooks2<A> = HashMap<String,
// RegisterReadHook2<A>>;
//
///// Hook for a register write.
//pub type RegisterWriteHook2<A> = fn(state: &mut GAState<A>, value: DExpr) ->
// SuperResult<()>; pub type RegisterWriteHooks2<A> = HashMap<String,
// RegisterWriteHook2<A>>;
//
//#[derive(Debug, Clone)]
//pub enum MemoryHookAddress2 {
//    Single(u64),
//    Range(u64, u64),
//}
//
///// Hook for a memory write.
//pub type MemoryWriteHook2<A> =
//    fn(state: &mut GAState<A>, address: u64, value: DExpr, bits: u32) ->
// SuperResult<()>; pub type SingleMemoryWriteHooks2<A> = HashMap<u64,
// MemoryWriteHook<A>>; pub type RangeMemoryWriteHooks2<A> = Vec<((u64, u64),
// MemoryWriteHook<A>)>;
//
///// Hook for a memory read.
//pub type MemoryReadHook2<A> = fn(state: &mut GAState<A>, address: u64) ->
// SuperResult<DExpr>; pub type SingleMemoryReadHooks2<A> = HashMap<u64,
// MemoryReadHook<A>>; pub type RangeMemoryReadHooks2<A> = Vec<((u64, u64),
// MemoryReadHook<A>)>;

// TODO: replace ga state <A> with state container.

#[derive(Debug, Clone, Copy)]
pub enum PCHook2<S: StateContainer> {
    Continue,
    EndSuccess,
    EndFailure(&'static str),
    Intrinsic(fn(state: &mut GAState<S::Architecture>) -> super::Result<()>),
    Suppress,
}

#[derive(Debug)]
pub struct VM2<SMT: SmtSolver, C: Composition> {
    pub solver: SMT,
    pub project: Project<<C::StateContainer as StateContainer>::Architecture>,
    pub paths: DFSPathSelection<<C::StateContainer as StateContainer>::Architecture>,
}

struct HookContainer<C: Composition> {
    register_read_hook: HashMap<
        String,
        fn(state: &mut GAState2<C>) -> super::Result<<C::SMT as SmtSolver>::Expression>,
    >,
    register_write_hook: HashMap<
        String,
        fn(state: &mut GAState2<C>, value: <C::SMT as SmtSolver>::Expression) -> super::Result<()>,
    >,
    pc_hook: HashMap<u64, PCHook2<<C as Composition>::StateContainer>>,
    single_memory_read_hook: HashMap<
        u64,
        fn(state: &mut GAState2<C>) -> super::Result<<C::SMT as SmtSolver>::Expression>,
    >,
    single_memory_write_hook: HashMap<
        u64,
        fn(state: &mut GAState2<C>, value: <C::SMT as SmtSolver>::Expression) -> super::Result<()>,
    >,
    range_memory_read_hook: Vec<(
        (u64, u64),
        fn(state: &mut GAState2<C>) -> super::Result<<C::SMT as SmtSolver>::Expression>,
    )>,
    range_memory_write_hook: Vec<(
        (u64, u64),
        fn(state: &mut GAState2<C>, value: <C::SMT as SmtSolver>::Expression) -> super::Result<()>,
    )>,
}

struct Reader<'a, C: Composition> {
    memory: &'a <C::SMT as SmtSolver>::Memory,
    container: &'a mut HookContainer<C>,
}
struct Writer<'a, C: Composition> {
    memory: &'a mut <C::SMT as SmtSolver>::Memory,
    container: &'a mut HookContainer<C>,
}

impl<C: Composition> HookContainer<C> {
    fn reader<'a>(&'a mut self, memory: &'a <C::SMT as SmtSolver>::Memory) -> Reader<'a, C> {
        Reader {
            memory,
            container: self,
        }
    }

    fn writer<'a>(&'a mut self, memory: &'a mut <C::SMT as SmtSolver>::Memory) -> Writer<'a, C> {
        Writer {
            memory,
            container: self,
        }
    }
}

pub enum ResultOrHook<A: Sized, B: Sized> {
    Result(A),
    Hook(B),
    Hooks(Vec<B>),
}

impl<'a, C: Composition> Reader<'a, C> {
    fn read_memory(
        &mut self,
        addr: <C::SMT as SmtSolver>::Expression,
        size: WordSize,
    ) -> ResultOrHook<
        std::result::Result<<C::SMT as SmtSolver>::Expression, MemoryError>,
        fn(state: &mut GAState2<C>) -> Result<<C::SMT as SmtSolver>::Expression>,
    > {
        let caddr = addr.get_constant();
        if caddr.is_none() {
            return ResultOrHook::Result(self.memory.get(&addr, size));
        }

        let caddr = caddr.unwrap();

        if let Some(hook) = self.container.single_memory_read_hook.get(&caddr) {
            let mut ret = self
                .container
                .range_memory_read_hook
                .iter()
                .filter(|el| ((el.0 .0)..=(el.0 .1)).contains(&caddr))
                .map(|el| el.1)
                .collect::<Vec<_>>();
            ret.push(hook.clone());
            return ResultOrHook::Hooks(ret.clone());
        }

        let ret = self
            .container
            .range_memory_read_hook
            .iter()
            .filter(|el| ((el.0 .0)..=(el.0 .1)).contains(&caddr))
            .map(|el| el.1)
            .collect::<Vec<_>>();
        if !ret.is_empty() {
            return ResultOrHook::Hooks(ret);
        }
        ResultOrHook::Result(self.memory.get(&addr, size))
    }

    fn read_register(
        &mut self,
        id: &String,
        size: WordSize,
    ) -> ResultOrHook<
        std::result::Result<<C::SMT as SmtSolver>::Expression, MemoryError>,
        fn(state: &mut GAState2<C>) -> Result<<C::SMT as SmtSolver>::Expression>,
    > {
        if let Some(hook) = self.container.register_read_hook.get(id) {
            return ResultOrHook::Hook(hook.clone());
        }

        ResultOrHook::Result(self.memory.get_register(id, size))
    }

    fn read_pc(
        &mut self,
    ) -> ResultOrHook<
        std::result::Result<<C::SMT as SmtSolver>::Expression, MemoryError>,
        fn(state: &mut GAState2<C>) -> Result<<C::SMT as SmtSolver>::Expression>,
    > {
        ResultOrHook::Result(self.memory.get_pc())
    }
}

impl<'a, C: Composition> Writer<'a, C> {
    fn write_memory(
        &mut self,
        addr: <C::SMT as SmtSolver>::Expression,
        value: <C::SMT as SmtSolver>::Expression,
    ) -> ResultOrHook<
        std::result::Result<(), MemoryError>,
        fn(&mut GAState2<C>, <<C as Composition>::SMT as SmtSolver>::Expression) -> Result<()>,
    > {
        let caddr = addr.get_constant();
        if caddr.is_none() {
            return ResultOrHook::Result(self.memory.set(&addr, &value));
        }

        let caddr = caddr.unwrap();

        if let Some(hook) = self.container.single_memory_write_hook.get(&caddr) {
            let mut ret = self
                .container
                .range_memory_write_hook
                .iter()
                .filter(|el| ((el.0 .0)..=(el.0 .1)).contains(&caddr))
                .map(|el| el.1)
                .collect::<Vec<_>>();
            ret.push(hook.clone());
            return ResultOrHook::Hooks(ret.clone());
        }

        let ret = self
            .container
            .range_memory_write_hook
            .iter()
            .filter(|el| ((el.0 .0)..=(el.0 .1)).contains(&caddr))
            .map(|el| el.1)
            .collect::<Vec<_>>();
        if !ret.is_empty() {
            return ResultOrHook::Hooks(ret);
        }
        ResultOrHook::Result(self.memory.set(&addr, &value))
    }

    fn write_register(
        &mut self,
        id: &String,
        value: &<C::SMT as SmtSolver>::Expression,
    ) -> ResultOrHook<
        std::result::Result<(), MemoryError>,
        fn(&mut GAState2<C>, <<C as Composition>::SMT as SmtSolver>::Expression) -> Result<()>,
    > {
        if let Some(hook) = self.container.register_write_hook.get(id) {
            return ResultOrHook::Hook(hook.clone());
        }

        ResultOrHook::Result(self.memory.set_register(id, value))
    }

    fn write_pc(
        &mut self,
        value: u32,
    ) -> ResultOrHook<
        std::result::Result<(), MemoryError>,
        fn(&mut GAState2<C>, <<C as Composition>::SMT as SmtSolver>::Expression) -> Result<()>,
    > {
        ResultOrHook::Result(self.memory.set_pc(&value))
    }
}

impl<SMT: SmtSolver, C: Composition<SMT = SMT>> VM2<SMT, C> {
    pub fn new(
        project: Project<<C::StateContainer as StateContainer>::Architecture>,
        fn_name: &str,
        end_pc: u64,
        solver: SMT,
        composition: C,
    ) -> Result<Self> {
        let mut vm = Self {
            solver,
            project,
            paths: DFSPathSelection::new(),
        };

        let state = GAState::<A>::new(ctx, project, solver, fn_name, end_pc, architecture)?;

        vm.paths.save_path(Path::new(state, None));

        Ok(vm)
    }

    pub fn new_with_state(
        project: &'static Project<<C::StateContainer as StateContainer>::Architecture>,
        state: GAState2<C>,
    ) -> Self {
        let mut vm = Self {
            project,
            paths: DFSPathSelection::new(),
        };

        vm.paths.save_path(Path::new(state, None));

        vm
    }

    pub fn run(&mut self) -> Result<Option<(PathResult, GAState<A>)>> {
        if let Some(path) = self.paths.get_path() {
            // try stuff
            let mut executor = GAExecutor::from_state(path.state, self, self.project);

            for constraint in path.constraints {
                executor.state.constraints.assert(&constraint);
            }

            let result = executor.resume_execution()?;
            return Ok(Some((result, executor.state)));
        }
        Ok(None)
    }
}
