//! Defines a generic architecture
//!
//! An architecture is in the scope of this crate
//! something that defines a instruction set that
//! can be translated in to general_assembly [`Instruction`]s.
//! Moreover the architecture may define a few
//! architecture specific hooks.

pub mod arm;
use std::fmt::{Debug, Display};

use arm::{v6::ArmV6M, v7::ArmV7EM};
use object::File;
use thiserror::Error;

use crate::general_assembly::{instruction::Instruction, state::GAState, RunConfig};

#[derive(Debug, Eq, PartialEq, PartialOrd, Clone, Error)]
/// General architecture related errors.
pub enum ArchError {
    /// Thrown when an unsupported architecture is requested.
    #[error("Tried to execute code for an unsupported architecture")]
    UnsuportedArchitechture,

    /// Thrown when an unsupported file type is used.
    #[error("Tried to execute code from a non elf file.")]
    IncorrectFileType,

    /// Thrown when the binary files fields are malformed.
    #[error("Tried to read a malformed section.")]
    MalformedSection,

    /// Thrown when a specific required section does not exist in the binary
    #[error("Elf file missing critical section {0}.")]
    MissingSection(&'static str),

    /// Thrown when a different module errors and that error is not convertible
    /// in to an [`ArchError`]
    #[error("Generic archerror : {0}.")]
    ImplementorStringError(&'static str),

    /// Thrown when something goes wrong during instruction parsing.
    #[error("Error occured while parsing.")]
    ParsingError(#[from] ParseError),
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Clone, Error)]
pub enum ParseError {
    /// Input not long enough for an instruction.
    #[error("Insufficient input")]
    InsufficientInput,

    /// 32 bit instruction not long enough.
    #[error("Tried to parse a malformed instruction.")]
    MalfromedInstruction,

    /// Opcode not matching valid 32 bit instruction.
    #[error("Instruction not supported in the parser.")]
    InvalidInstruction,

    /// This instruction causes unpredictable behaviour.
    #[error("Instruction defined as unpredictable.")]
    Unpredictable,

    /// Trying to access an invalid register.
    #[error("Parser encounterd an invalid register.")]
    InvalidRegister,

    /// Invalid condition code used.
    #[error("Parser encounterd an invalid conditon.")]
    InvalidCondition,

    /// A generic parsing error.
    #[error("Parser encounterd some unspecified error.")]
    Generic(&'static str),
}

pub enum Architecture {
    Armv7EM(ArmV7EM),
    Armv6EM(ArmV6M),
}

/// A generic architecture
///
/// Denotes that the implementer can be treated as an architecture in this
/// crate.
pub trait Arch: Debug + Display + Clone + Copy + Sized + 'static {
    /// Converts a slice of bytes to an [`Instruction`]
    fn translate(&self, buff: &[u8], state: &GAState<Self>)
        -> Result<Instruction<Self>, ArchError>;

    /// Adds the architecture specific hooks to the [`RunConfig`]
    fn add_hooks(&self, cfg: &mut RunConfig<Self>);

    /// Returns an instance of self if the file is defined for this
    /// specific architecture.
    fn discover(file: &File) -> Result<Option<Self>, ArchError>;
}
