#![allow(dead_code, missing_docs)]
use std::path::PathBuf;

use object::{File, ReadRef};

use crate::arch::Arch;

pub mod run_config;

pub struct SymexConfigurator<const PATH_SPECIFIED: bool> {
    path: Option<PathBuf>,
}

pub struct SymexConfiguration<A: Arch> {
    path: PathBuf,
    arch: A,
}

pub struct SymexInitiator<'a, R: ReadRef<'a>> {
    file: File<'a, R>,
}
