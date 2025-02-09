use crate::{
    arch::SupportedArchitecture,
    executor::{hooks::HookContainer, vm::VM, PathResult},
    logging::Logger,
    project::dwarf_helper::SubProgramMap,
    smt::SmtMap,
    Composition,
    GAError,
};

pub struct SymexArbiter<C: Composition> {
    logger: C::Logger,
    project: <C::Memory as SmtMap>::ProgramMemory,
    ctx: C::SMT,
    state_container: C::StateContainer,
    hooks: HookContainer<C>,
    symbol_lookup: SubProgramMap,
    archtecture: SupportedArchitecture,
}

impl<C: Composition> SymexArbiter<C> {
    pub(crate) fn new(
        logger: C::Logger,
        project: <C::Memory as SmtMap>::ProgramMemory,
        ctx: C::SMT,
        state_container: C::StateContainer,
        hooks: HookContainer<C>,
        symbol_lookup: SubProgramMap,
        archtecture: SupportedArchitecture,
    ) -> Self {
        Self {
            logger,
            project,
            ctx,
            state_container,
            hooks,
            symbol_lookup,
            archtecture,
        }
    }
}

impl<C: Composition> SymexArbiter<C> {
    pub fn add_hooks<F: FnMut(&mut HookContainer<C>, &SubProgramMap)>(
        &mut self,
        mut f: F,
    ) -> &mut Self {
        f(&mut self.hooks, &self.symbol_lookup);
        self
    }

    pub fn get_symbol_map(&self) -> &SubProgramMap {
        &self.symbol_lookup
    }

    pub fn run(&mut self, function: &str) -> crate::Result<&C::Logger> {
        let function = match self.symbol_lookup.get_by_name(function) {
            Some(value) => value,
            None => return Err(GAError::EntryFunctionNotFound(function.to_string())),
        };
        let mut vm = VM::new(
            self.project.clone(),
            &self.ctx,
            function,
            0xFFFFFFFE,
            self.state_container.clone(),
            self.hooks.clone(),
            self.archtecture.clone(),
        )?;

        let mut path_idx = 0;
        self.logger.change_path(path_idx);
        while let Some((result, state, conditions)) = vm.run(&mut self.logger)? {
            self.logger.add_constraints(
                conditions
                    .iter()
                    .map(|el| format!("{el:?}"))
                    .collect::<Vec<_>>(),
            );

            if let PathResult::Suppress = result {
                self.logger.warn("Suppressing path");
                path_idx += 1;
                self.logger.change_path(path_idx);
                continue;
            }

            self.logger.record_path_result(result);
            self.logger.record_execution_time(state.cycle_count);
            self.logger.record_final_state(state);
            path_idx += 1;
            self.logger.change_path(path_idx);
        }

        Ok(&self.logger)
    }

    pub fn consume(self) -> C::Logger {
        self.logger
    }
}

//pub struct Runner<'strings, 'ret, C: Composition, I: Iterator<Item =
// &'strings str>> {    arbiter: SymexArbiter<C>,
//    functions: I,
//    ret: PhantomData<&'ret ()>,
//}
//
//impl<'strings, 'ret, C: Composition, I: Iterator<Item = &'strings str>>
// Iterator    for Runner<'strings, 'ret, C, I>
//where
//    <C as Composition>::Logger: 'ret + 'strings,
//{
//    type Item = crate::Result<&'ret C::Logger>;
//
//    fn next(&'strings mut self) -> Option<Self::Item> {
//        let func = self.functions.next()?;
//        Some(self.arbiter.run(func))
//    }
//}
