use hashbrown::HashMap;

use super::state::GAState2;
use crate::{
    arch::Architecture,
    smt::{MemoryError, SmtExpr, SmtMap, SmtSolver},
    Composition,
    Result,
    WordSize,
};

/// Represents a generic state container.
pub trait StateContainer: Clone {
    type Architecture: Architecture;

    #[must_use]
    /// Returns the underlying architecture.
    fn as_arch(&mut self) -> &mut Self::Architecture;
}

#[derive(Debug, Clone, Copy)]
pub enum PCHook2<C: Composition> {
    Continue,
    EndSuccess,
    EndFailure(&'static str),
    Intrinsic(fn(state: &mut GAState2<C>) -> super::Result<()>),
    Suppress,
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

    pc_hook: HashMap<u64, PCHook2<C>>,

    single_memory_read_hook: HashMap<
        u64,
        fn(state: &mut GAState2<C>) -> super::Result<<C::SMT as SmtSolver>::Expression>,
    >,

    single_memory_write_hook: HashMap<
        u64,
        fn(state: &mut GAState2<C>, value: <C::SMT as SmtSolver>::Expression) -> super::Result<()>,
    >,

    // TODO: Replace with a proper range tree implementation.
    range_memory_read_hook: Vec<(
        (u64, u64),
        fn(state: &mut GAState2<C>) -> super::Result<<C::SMT as SmtSolver>::Expression>,
    )>,

    range_memory_write_hook: Vec<(
        (u64, u64),
        fn(state: &mut GAState2<C>, value: <C::SMT as SmtSolver>::Expression) -> super::Result<()>,
    )>,
}

pub struct Reader<'a, C: Composition> {
    memory: &'a <C::SMT as SmtSolver>::Memory,
    container: &'a mut HookContainer<C>,
}

pub struct Writer<'a, C: Composition> {
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
        size: usize,
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
        size: usize,
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
            return ResultOrHook::Result(self.memory.set(&addr, value));
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
        ResultOrHook::Result(self.memory.set(&addr, value))
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
