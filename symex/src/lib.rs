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

pub mod elf_util;
pub mod general_assembly;
pub mod memory;
//#[cfg(not(feature = "llvm"))]
pub mod run_elf;
#[cfg(feature = "llvm")]
pub mod run_llvm;
pub mod smt;
#[cfg(feature = "llvm")]
pub mod util;
#[cfg(feature = "llvm")]
pub mod vm;
