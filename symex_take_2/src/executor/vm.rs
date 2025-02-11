//! Describes the VM for general assembly

use super::{hooks::HookContainer, state::GAState, GAExecutor, PathResult};
use crate::{
    arch::SupportedArchitecture,
    path_selection::{DFSPathSelection, Path},
    project::dwarf_helper::SubProgram,
    smt::{SmtMap, SmtSolver},
    Composition,
    Result,
};

#[derive(Debug)]
pub struct VM<C: Composition> {
    pub project: <C::Memory as SmtMap>::ProgramMemory,
    pub paths: DFSPathSelection<C>,
}

impl<C: Composition> VM<C> {
    pub fn new(
        project: <C::Memory as SmtMap>::ProgramMemory,
        ctx: &C::SMT,
        function: &SubProgram,
        end_pc: u64,
        state_container: C::StateContainer,
        hooks: HookContainer<C>,
        architecture: SupportedArchitecture,
    ) -> Result<Self> {
        let mut vm = Self {
            project: project.clone(),
            paths: DFSPathSelection::new(),
        };

        let mut state = GAState::<C>::new(
            ctx.clone(),
            ctx.clone(),
            project,
            hooks,
            &function.name,
            end_pc,
            state_container,
            architecture,
        )?;
        let _ = state.memory.set_pc(function.bounds.0 as u32)?;

        vm.paths.save_path(Path::new(state, None));

        Ok(vm)
    }

    #[cfg(test)]
    pub(crate) fn new_test_vm(
        project: <C::Memory as SmtMap>::ProgramMemory,
        state: GAState<C>,
    ) -> Result<Self> {
        let mut vm = Self {
            project: project.clone(),
            paths: DFSPathSelection::new(),
        };

        vm.paths.save_path(Path::new(state, None));

        Ok(vm)
    }

    pub fn new_with_state(
        project: <C::Memory as SmtMap>::ProgramMemory,
        state: GAState<C>,
    ) -> Self {
        let mut vm = Self {
            project,
            paths: DFSPathSelection::new(),
        };

        vm.paths.save_path(Path::new(state, None));

        vm
    }

    pub fn run(
        &mut self,
        logger: &mut C::Logger,
    ) -> Result<Option<(PathResult<C>, GAState<C>, Vec<C::SmtExpression>)>> {
        if let Some(path) = self.paths.get_path() {
            // try stuff
            let mut executor = GAExecutor::from_state(path.state, self, self.project.clone());

            for constraint in path.constraints.clone() {
                executor.state.constraints.assert(&constraint);
            }

            let result = executor.resume_execution(logger)?;
            return Ok(Some((result, executor.state, path.constraints)));
        }
        Ok(None)
    }
}
