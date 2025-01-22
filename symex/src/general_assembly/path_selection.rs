use super::{arch::Arch, state::GAState};
use crate::smt::DExpr;

#[derive(Debug, Clone)]
pub struct Path<A: Arch> {
    /// The state to use when resuming execution.
    ///
    /// The location in the state should be where to resume execution at.
    pub state: GAState<A>,

    /// Constraints to add before starting execution on this path.
    pub constraints: Vec<DExpr>,
}

impl<A: Arch> Path<A> {
    /// Creates a new path starting at a certain state, optionally asserting a
    /// condition on the created path.
    pub fn new(state: GAState<A>, constraint: Option<DExpr>) -> Self {
        let constraints = match constraint {
            Some(c) => vec![c],
            None => vec![],
        };

        Self { state, constraints }
    }
}

/// Depth-first search path exploration.
///
/// Each path is explored for as long as possible, when a path finishes the most
/// recently added path is the next to be run.
#[derive(Debug, Clone)]
pub struct DFSPathSelection<A: Arch> {
    paths: Vec<Path<A>>,
}

impl<A: Arch> DFSPathSelection<A> {
    /// Creates new without any stored paths.
    pub fn new() -> Self {
        Self { paths: Vec::new() }
    }

    /// Add a new path to be explored.
    pub fn save_path(&mut self, path: Path<A>) {
        path.state.constraints.push();
        self.paths.push(path);
    }

    /// Retrieve the next path to explore.
    pub fn get_path(&mut self) -> Option<Path<A>> {
        match self.paths.pop() {
            Some(path) => {
                path.state.constraints.pop();
                Some(path)
            }
            None => None,
        }
    }

    pub fn waiting_paths(&self) -> usize {
        self.paths.len()
    }
}
