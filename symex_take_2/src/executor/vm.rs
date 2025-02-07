//! Describes the VM for general assembly

use super::{hooks::HookContainer, state::GAState2, GAExecutor, PathResult};
use crate::{
    path_selection::{DFSPathSelection, Path},
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
        fn_name: &str,
        end_pc: u64,
        state_container: C::StateContainer,
        hooks: HookContainer<C>,
    ) -> Result<Self> {
        let mut vm = Self {
            project: project.clone(),
            paths: DFSPathSelection::new(),
        };

        let state = GAState2::<C>::new(
            ctx.clone(),
            ctx.clone(),
            project,
            hooks,
            fn_name,
            end_pc,
            state_container,
        )?;

        vm.paths.save_path(Path::new(state, None));

        Ok(vm)
    }

    pub fn new_with_state(
        project: <C::Memory as SmtMap>::ProgramMemory,
        state: GAState2<C>,
    ) -> Self {
        let mut vm = Self {
            project,
            paths: DFSPathSelection::new(),
        };

        vm.paths.save_path(Path::new(state, None));

        vm
    }

    pub fn run(&mut self) -> Result<Option<(PathResult<C>, GAState2<C>, Vec<C::SmtExpression>)>> {
        if let Some(path) = self.paths.get_path() {
            // try stuff
            let mut executor = GAExecutor::from_state(path.state, self, self.project.clone());

            for constraint in path.constraints.clone() {
                executor.state.constraints.assert(&constraint);
            }

            let result = executor.resume_execution()?;
            return Ok(Some((result, executor.state, path.constraints)));
        }
        Ok(None)
    }
}
