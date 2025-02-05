//! Describes a general assembly instruction.

use general_assembly::operation::Operation;

use super::state::GAState2;
use crate::{arch::Architecture, executor::state::GAState, Composition};

/// Representing a cycle count for an instruction.
#[derive(Debug, Clone)]
pub enum CycleCount<A: Architecture> {
    /// Cycle count is a pre-calculated value
    Value(usize),

    /// Cycle count depends on execution state
    Function(fn(state: &GAState<A>) -> usize),
}

/// Represents a general assembly instruction.
#[derive(Debug, Clone)]
pub struct Instruction<A: Architecture> {
    /// The size of the original machine instruction in number of bits.
    pub instruction_size: u32,

    /// A list of operations that will be executed in order when
    /// executing the instruction.
    pub operations: Vec<Operation>,

    /// The maximum number of cycles the instruction will take.
    /// This can depend on state and will be evaluated after the
    /// instruction has executed but before the next instruction.
    pub max_cycle: CycleCount<A>,

    /// Denotes whether or not the instruction required access to the underlying
    /// memory or not.
    pub memory_access: bool,
}

/// Representing a cycle count for an instruction.
#[derive(Debug, Clone)]
pub enum CycleCount2<C: Composition> {
    /// Cycle count is a pre-calculated value
    Value(usize),

    /// Cycle count depends on execution state
    Function(fn(state: &GAState2<C>) -> usize),
}

/// Represents a general assembly instruction.
#[derive(Debug, Clone)]
pub struct Instruction2<C: Composition> {
    /// The size of the original machine instruction in number of bits.
    pub instruction_size: u32,

    /// A list of operations that will be executed in order when
    /// executing the instruction.
    pub operations: Vec<Operation>,

    /// The maximum number of cycles the instruction will take.
    /// This can depend on state and will be evaluated after the
    /// instruction has executed but before the next instruction.
    pub max_cycle: CycleCount2<C>,

    /// Denotes whether or not the instruction required access to the underlying
    /// memory or not.
    pub memory_access: bool,
}
