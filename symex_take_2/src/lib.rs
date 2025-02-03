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

use arch::ArchError;
use memory::MemoryError;
use project::ProjectError;
use smt::SolverError;

pub mod arch;
pub mod elf_util;
pub mod executor;
pub mod initiation;
pub mod manager;
pub mod memory;
pub mod path_selection;
pub mod project;
pub mod run_elf;
pub mod smt;
//pub mod util;

pub type Result<T> = std::result::Result<T, GAError>;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GAError {
    #[error("Project error: {0}")]
    ProjectError(#[from] ProjectError),

    #[error("memory error: {0}")]
    MemoryError(#[from] MemoryError),

    #[error("Entry function {0} not found.")]
    EntryFunctionNotFound(String),

    #[error("Writing to static memory not permitted.")]
    WritingToStaticMemoryProhibited,

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

#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum call stack depth. Default is `1000`.
    pub max_call_depth: usize,

    /// Maximum iteration count. Default is `1000`.
    pub max_iter_count: usize,

    /// Maximum amount of concretizations for function pointers. Default is `1`.
    pub max_fn_ptr_resolutions: usize,

    /// Maximum amount of concretizations for a memory address. This does not
    /// apply for e.g. ArrayMemory, but does apply for ObjectMemory. Default
    /// is `100`.
    pub max_memory_access_resolutions: usize,

    /// Maximum amount of concretizations for memmove, memcpy, memset and other
    /// intrinsic functions. Default is `100`.
    pub max_intrinsic_concretizations: usize,
}
