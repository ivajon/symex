use crate::{
    executor::{hooks::HookContainer, vm::VM, PathResult},
    logging::{Logger, Region},
    smt::SmtMap,
    Composition,
};

pub struct SymexArbiter<C: Composition> {
    logger: C::Logger,
    project: <C::Memory as SmtMap>::ProgramMemory,
    ctx: C::SMT,
    state_container: C::StateContainer,
    hooks: HookContainer<C>,
}

impl<C: Composition> SymexArbiter<C> {
    pub(crate) fn new(
        logger: C::Logger,
        project: <C::Memory as SmtMap>::ProgramMemory,
        ctx: C::SMT,
        state_container: C::StateContainer,
        hooks: HookContainer<C>,
    ) -> Self {
        Self {
            logger,
            project,
            ctx,
            state_container,
            hooks,
        }
    }
}

impl<C: Composition> SymexArbiter<C> {
    pub fn run(&mut self, function: &str) -> crate::Result<&C::Logger> {
        let mut vm = VM::new(
            self.project.clone(),
            &self.ctx,
            function,
            0xFFFFFFFE,
            self.state_container.clone(),
            self.hooks.clone(),
        )?;

        let mut path_idx = 0;
        self.logger.change_path(path_idx);
        while let Some((result, state, conditions)) = vm.run()? {
            self.logger.add_constraints(
                conditions
                    .iter()
                    .map(|el| format!("{el:?}"))
                    .collect::<Vec<_>>(),
            );

            if let PathResult::Suppress = result {
                self.logger.warn(
                    <C::Logger as Logger>::RegionIdentifier::global(),
                    "Suppressing path",
                );
                path_idx += 1;
                self.logger.change_path(path_idx);
                continue;
            }

            self.logger.record_path_result(result);
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
