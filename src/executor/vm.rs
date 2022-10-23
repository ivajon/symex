use crate::{
    core::executor::VMError,
    core::smt::Solver,
    executor::llvm::{project::Project, LLVMState},
    llvm::{LLVMExecutorError, ReturnValue},
    path_exploration::Path,
    smt::{DContext, DSolver},
    Config, DFSPathExploration, LLVMExecutor, PathExploration, Stats, Variable,
};
use tracing::error;

#[derive(Debug)]
pub struct VM {
    project: &'static Project,

    pub(crate) paths: DFSPathExploration,

    pub cfg: Config,

    pub stats: Stats,

    pub inputs: Vec<Variable>,
}

impl VM {
    pub fn new(
        project: &'static Project,
        ctx: &'static DContext,
        fn_name: &str,
    ) -> Result<Self, VMError> {
        let (module, function) = project.find_entry_function(fn_name)?;
        if !function.parameters.is_empty() {
            error!(
                "Function {} has parameters: {:?}",
                function.name, function.parameters
            );
            return Err(VMError::UnexpectedParameter);
        }

        let solver = DSolver::new(ctx);
        let state = LLVMState::new(ctx, project, solver, module, function);

        let mut vm = Self {
            project,
            paths: DFSPathExploration::new(),
            cfg: Config::new(),
            stats: Stats::new(),
            inputs: Vec::new(),
        };
        let path = Path::new(state, None);
        vm.paths.save_path(path);

        Ok(vm)
    }

    pub fn run(&mut self) -> Option<(Result<ReturnValue, VMError>, LLVMState)> {
        loop {
            if let Some(path) = self.paths.get_path() {
                let mut executor = LLVMExecutor::from_state(path.state, self, self.project);
                for constraint in path.constraints {
                    executor.state.constraints.assert(&constraint);
                }

                let res = match executor.resume_execution() {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        if let LLVMExecutorError::SuppressPath = e {
                            continue;
                        } else {
                            Err(e.into())
                        }
                    }
                };
                break Some((res, executor.state));
            } else {
                break None;
            }
        }
    }
}
