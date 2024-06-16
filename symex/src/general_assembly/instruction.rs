//! Describes a general assembly instruction.

use general_assembly::operation::Operation;

use super::{arch::Arch, state::GAState};

/// Representing a cycle count for a instruction.
#[derive(Debug, Clone)]
pub enum CycleCount<A: Arch> {
    /// Cycle count is a precalculated value
    Value(usize),

    /// Cycle count depends on execution state
    Function(fn(state: &GAState<A>) -> usize),
}

/// Represents a general assembly instruction.
#[derive(Debug, Clone)]
pub struct Instruction<A: Arch> {
    /// The size of the original machine instruction in number of bits.
    pub instruction_size: u32,

    /// A list of operations that will be executed in order when
    /// executing the instruction.
    pub operations: Vec<Operation>,

    /// The maximum number of cycles the instruction will take.
    /// This can depend on state and will be evaluated after the
    /// instruction has executed but before the next instruction.
    pub max_cycle: CycleCount<A>,

    /// Denotes wether or not the instruction required access to the underlying
    /// memory or not.
    pub memory_access: bool,
}
