use std::fmt::Debug;

use hashbrown::HashMap;
use tracing::trace;

use super::state::GAState2;
use crate::{
    arch::Architecture,
    project::dwarf_helper::SubProgramMap,
    smt::{MemoryError, SmtExpr, SmtMap, SmtSolver},
    Composition,
    Result,
};

/// Represents a generic state container.
pub trait StateContainer: Debug {
    type Architecture: Architecture + ?Sized;

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

#[derive(Debug, Clone)]
pub struct HookContainer<C: Composition> {
    register_read_hook:
        HashMap<String, fn(state: &mut GAState2<C>) -> super::Result<C::SmtExpression>>,

    register_write_hook:
        HashMap<String, fn(state: &mut GAState2<C>, value: C::SmtExpression) -> super::Result<()>>,

    pc_hook: HashMap<u64, PCHook2<C>>,

    single_memory_read_hook:
        HashMap<u64, fn(state: &mut GAState2<C>, address: u64) -> super::Result<C::SmtExpression>>,

    single_memory_write_hook: HashMap<
        u64,
        fn(state: &mut GAState2<C>, value: C::SmtExpression, address: u64) -> super::Result<()>,
    >,

    // TODO: Replace with a proper range tree implementation.
    range_memory_read_hook: Vec<(
        (u64, u64),
        fn(state: &mut GAState2<C>, address: u64) -> super::Result<C::SmtExpression>,
    )>,

    range_memory_write_hook: Vec<(
        (u64, u64),
        fn(state: &mut GAState2<C>, value: C::SmtExpression, address: u64) -> super::Result<()>,
    )>,
}

type RegisterReadHook<C: Composition> =
    fn(state: &mut GAState2<C>) -> super::Result<C::SmtExpression>;
type RegisterWriteHook<C: Composition> =
    fn(state: &mut GAState2<C>, value: C::SmtExpression) -> super::Result<()>;

type MemoryReadHook<C: Composition> =
    fn(state: &mut GAState2<C>, address: u64) -> super::Result<C::SmtExpression>;
type MemoryWriteHook<C: Composition> =
    fn(state: &mut GAState2<C>, value: C::SmtExpression, address: u64) -> super::Result<()>;

type MemoryRangeReadHook<C: Composition> =
    fn(state: &mut GAState2<C>, address: u64) -> super::Result<C::SmtExpression>;
type MemoryRangeWriteHook<C: Composition> =
    fn(state: &mut GAState2<C>, value: C::SmtExpression, address: u64) -> super::Result<()>;

impl<C: Composition> HookContainer<C> {
    /// Adds a PC hook to the executor.
    ///
    /// ## NOTE
    ///
    /// If a hook already exists for this address it will be overwritten.
    pub fn add_pc_hook(&mut self, pc: u64, value: PCHook2<C>) -> &mut Self {
        let _ = self.pc_hook.insert(pc, value);
        self
    }

    /// Adds a register read hook to the executor.
    ///
    /// ## NOTE
    ///
    /// If a hook already exists for this register it will be overwritten.
    pub fn add_register_read_hook(
        &mut self,
        register: String,
        hook: RegisterReadHook<C>,
    ) -> &mut Self {
        let _ = self.register_read_hook.insert(register, hook);
        self
    }

    /// Adds a register write hook to the executor.
    ///
    /// ## NOTE
    ///
    /// If a hook already exists for this register it will be overwritten.
    pub fn add_register_write_hook(
        &mut self,
        register: String,
        hook: RegisterWriteHook<C>,
    ) -> &mut Self {
        let _ = self.register_write_hook.insert(register, hook);
        self
    }

    /// Adds a memory read hook to the executor.
    ///
    /// ## NOTE
    ///
    /// If a hook already exists for this address it will be overwritten.
    pub fn add_memory_read_hook(&mut self, address: u64, hook: MemoryReadHook<C>) -> &mut Self {
        let _ = self.single_memory_read_hook.insert(address, hook);
        self
    }

    /// Adds a memory write hook to the executor.
    ///
    /// ## NOTE
    ///
    /// If a hook already exists for this address it will be overwritten.
    pub fn add_memory_write_hook(&mut self, address: u64, hook: MemoryWriteHook<C>) -> &mut Self {
        let _ = self.single_memory_write_hook.insert(address, hook);
        self
    }

    /// Adds a range memory read hook to the executor.
    ///
    /// If any address in this range is read it will trigger this hook.
    pub fn add_range_memory_read_hook(
        &mut self,
        (lower, upper): (u64, u64),
        hook: MemoryRangeReadHook<C>,
    ) -> &mut Self {
        let _ = self.range_memory_read_hook.push(((lower, upper), hook));
        self
    }

    /// Adds a range memory write hook to the executor.
    ///
    /// If any address in this range is written it will trigger this hook.
    pub fn add_range_memory_write_hook(
        &mut self,
        (lower, upper): (u64, u64),
        hook: MemoryRangeWriteHook<C>,
    ) -> &mut Self {
        let _ = self.range_memory_write_hook.push(((lower, upper), hook));
        self
    }

    /// Adds a pc hook via regex matching in the dwarf data.
    pub fn add_pc_hook_regex(
        &mut self,
        map: &SubProgramMap,
        pattern: &'static str,
        hook: PCHook2<C>,
    ) -> Result<()> {
        let program = match map.get_by_regex(pattern) {
            Some(pattern) => pattern,
            None => return Err(crate::GAError::EntryFunctionNotFound(pattern.to_string())),
        };

        self.add_pc_hook(program.bounds.0, hook);
        Ok(())
    }
}

pub struct Reader<'a, C: Composition> {
    memory: &'a mut <C::SMT as SmtSolver>::Memory,
    container: &'a mut HookContainer<C>,
}

pub struct Writer<'a, C: Composition> {
    memory: &'a mut <C::SMT as SmtSolver>::Memory,
    container: &'a mut HookContainer<C>,
}

impl<C: Composition> HookContainer<C> {
    pub fn new() -> Self {
        Self {
            register_read_hook: HashMap::new(),
            register_write_hook: HashMap::new(),
            pc_hook: HashMap::new(),
            single_memory_read_hook: HashMap::new(),
            single_memory_write_hook: HashMap::new(),
            range_memory_read_hook: Vec::new(),
            range_memory_write_hook: Vec::new(),
        }
    }

    pub fn reader<'a>(
        &'a mut self,
        memory: &'a mut <C::SMT as SmtSolver>::Memory,
    ) -> Reader<'a, C> {
        Reader {
            memory,
            container: self,
        }
    }

    pub fn writer<'a>(
        &'a mut self,
        memory: &'a mut <C::SMT as SmtSolver>::Memory,
    ) -> Writer<'a, C> {
        Writer {
            memory,
            container: self,
        }
    }

    pub fn get_pc_hooks(&self, value: u32) -> ResultOrHook<u32, &PCHook2<C>> {
        if let Some(pchook) = self.pc_hook.get(&(value as u64)) {
            return ResultOrHook::Hook(pchook);
        }
        ResultOrHook::Result(value)
    }
}

pub enum ResultOrHook<A: Sized, B: Sized> {
    Result(A),
    Hook(B),
    Hooks(Vec<B>),
}

impl<'a, C: Composition> Reader<'a, C> {
    pub fn read_memory(
        &mut self,
        addr: C::SmtExpression,
        size: usize,
    ) -> ResultOrHook<
        std::result::Result<C::SmtExpression, MemoryError>,
        fn(state: &mut GAState2<C>, address: u64) -> Result<C::SmtExpression>,
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

    pub fn read_register(
        &mut self,
        id: &String,
    ) -> ResultOrHook<
        std::result::Result<C::SmtExpression, MemoryError>,
        fn(state: &mut GAState2<C>) -> Result<C::SmtExpression>,
    > {
        if let Some(hook) = self.container.register_read_hook.get(id) {
            return ResultOrHook::Hook(hook.clone());
        }

        ResultOrHook::Result(self.memory.get_register(id))
    }

    pub fn read_pc(&mut self) -> std::result::Result<C::SmtExpression, MemoryError> {
        self.memory.get_pc()
    }
}

impl<'a, C: Composition> Writer<'a, C> {
    pub fn write_memory(
        &mut self,
        addr: C::SmtExpression,
        value: C::SmtExpression,
    ) -> ResultOrHook<
        std::result::Result<(), MemoryError>,
        fn(
            state: &mut GAState2<C>,
            value: <<C as Composition>::SMT as SmtSolver>::Expression,
            address: u64,
        ) -> Result<()>,
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

    pub fn write_register(
        &mut self,
        id: &String,
        value: &C::SmtExpression,
    ) -> ResultOrHook<
        std::result::Result<(), MemoryError>,
        fn(&mut GAState2<C>, <<C as Composition>::SMT as SmtSolver>::Expression) -> Result<()>,
    > {
        if let Some(hook) = self.container.register_write_hook.get(id) {
            return ResultOrHook::Hook(hook.clone());
        }

        ResultOrHook::Result(self.memory.set_register(id, value.clone()))
    }

    pub fn write_pc(&mut self, value: u32) -> std::result::Result<(), MemoryError> {
        self.memory.set_pc(value)
    }
}

impl<C: Composition> HookContainer<C> {
    pub fn default(map: &SubProgramMap) -> Result<Self> {
        let mut ret = Self::new();
        // intrinsic functions
        let start_cyclecount = |state: &mut GAState2<C>| {
            state.cycle_count = 0;
            trace!("Reset the cycle count (cycle count: {})", state.cycle_count);

            // jump back to where the function was called from
            let lr = state.get_register("LR".to_owned()).unwrap();
            state.set_register("PC".to_owned(), lr)?;
            Ok(())
        };
        let end_cyclecount = |state: &mut GAState2<C>| {
            // stop counting
            state.count_cycles = false;
            trace!(
                "Stopped counting cycles (cycle count: {})",
                state.cycle_count
            );

            // jump back to where the function was called from
            let lr = state.get_register("LR".to_owned()).unwrap();
            state.set_register("PC".to_owned(), lr)?;
            Ok(())
        };

        ret.add_pc_hook_regex(map, r"^panic_$", PCHook2::EndFailure("panic"))?;
        ret.add_pc_hook_regex(
            map,
            r"^panic_cold_explicit$",
            PCHook2::EndFailure("explicit panic"),
        )?;
        ret.add_pc_hook_regex(
            map,
            r"^unwrap_failed$",
            PCHook2::EndFailure("unwrap failed"),
        )?;
        ret.add_pc_hook_regex(
            map,
            r"^panic_bounds_check$",
            PCHook2::EndFailure("bounds check failed"),
        )?;
        ret.add_pc_hook_regex(
            map,
            r"^unreachable_unchecked$",
            PCHook2::EndFailure("reached a unreachable unchecked call undefined behavior"),
        )?;
        ret.add_pc_hook_regex(map, r"^suppress_path$", PCHook2::Suppress)?;
        ret.add_pc_hook_regex(
            map,
            r"^start_cyclecount$",
            PCHook2::Intrinsic(start_cyclecount),
        )?;
        ret.add_pc_hook_regex(map, r"^end_cyclecount$", PCHook2::Intrinsic(end_cyclecount))?;
        Ok(ret)
    }
}
