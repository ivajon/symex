use crate::core::{memory::MemoryError, smt::SolverError};

mod cfg;
pub mod llvm;
mod stats;
mod vm;

pub use cfg::Config;
pub use stats::Stats;
pub use vm::VM;

// Should support different executors such as for LLVM and ASM
pub trait Executor {}

// Should be generic enough to support e.g. LLVM modules and ASM modules
pub trait Module {}

// Indiviual errors from the specific executors should be converted to this more general error

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExecutorError {
    #[error("Abort {0}")]
    Abort(i64),

    #[error("SolverError")]
    SolverError(#[from] SolverError),

    #[error("MemoryError")]
    MemoryError(#[from] MemoryError),

    #[error("Other {0}")]
    Other(String),
}