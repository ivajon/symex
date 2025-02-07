#![deny(warnings)]
#![deny(
    clippy::all,
    clippy::perf,
    rustdoc::all,
    rust_2024_compatibility,
    rust_2018_idioms
)]
// Add exceptions for things that are not error prone.
#![allow(
    clippy::new_without_default,
    clippy::uninlined_format_args,
    clippy::module_name_repetitions,
    clippy::too_many_arguments,
    // TODO: Add comments for these.
    clippy::missing_errors_doc,
    clippy::cast_lossless,
    // TODO: Remove this and add crate level docs.
    rustdoc::missing_crate_level_docs,
    tail_expr_drop_order
)]
#![feature(non_null_from_ref)]

use std::fmt::Debug;

use arch::{ArchError, Architecture};
use executor::hooks::StateContainer;
use logging::Logger;
use memory::MemoryError;
use project::ProjectError;
use smt::{SmtExpr, SmtMap, SmtSolver, SolverError};

pub mod arch;
pub mod defaults;
pub mod elf_util;
pub mod executor;
pub mod initiation;
pub mod logging;
pub mod manager;
pub mod memory;
pub mod path_selection;
pub mod project;
pub mod run_elf;
pub mod smt;

pub type Result<T> = std::result::Result<T, GAError>;

/// Denotes a tool composition used for analysis.
pub trait Composition: Clone + Debug {
    /// The state container, this can be either only architecture specific data
    /// or it may include user provided data.
    type StateContainer: StateContainer<Architecture = Self::Architecture> + Clone;
    type SMT: SmtSolver<Memory = Self::Memory, Expression = Self::SmtExpression>;
    type Architecture: Architecture + ?Sized;
    type Logger: Logger;

    type SmtExpression: SmtExpr;
    type Memory: SmtMap<SMT = Self::SMT, Expression = <Self::SMT as SmtSolver>::Expression>;

    fn logger(&mut self) -> &mut Self::Logger;
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GAError {
    #[error("Project error: {0}")]
    ProjectError(#[from] ProjectError),

    #[error("memory error: {0}")]
    MemoryError(#[from] MemoryError),

    #[error("memory error: {0}")]
    SmtMemoryError(#[from] smt::MemoryError),

    #[error("Entry function {0} not found.")]
    EntryFunctionNotFound(String),

    #[error("Writing to static memory not permitted.")]
    WritingToStaticMemoryProhibited,

    #[error("Program counter is not deterministic.")]
    NonDeterministicPC,

    #[error("Could not open the specified file.")]
    CouldNotOpenFile(String),

    #[error("Solver error.")]
    SolverError(#[from] SolverError),

    #[error("Architecture error.")]
    ArchError(#[from] ArchError),
}

#[derive(Debug, Clone, Copy)]
pub enum WordSize {
    Bit64,
    Bit32,
    Bit16,
    Bit8,
}

#[derive(Debug, Clone)]
pub enum Endianness {
    Little,
    Big,
}
