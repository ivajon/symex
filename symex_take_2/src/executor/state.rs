//! Holds the state in general assembly execution.

use std::collections::{HashMap, VecDeque};

use general_assembly::prelude::{Condition, DataWord};
use tracing::{debug, trace};

use super::{
    hooks::{HookContainer, PCHook2, Reader, ResultOrHook, StateContainer, Writer},
    instruction::Instruction2,
};
use crate::{
    arch::Architecture,
    project::{self, ProjectError},
    smt::{ProgramMemory, SmtExpr, SmtMap, SmtSolver},
    Composition,
    GAError,
    Result,
};

pub enum HookOrInstruction2<'a, C: Composition> {
    PcHook(&'a PCHook2<C>),
    Instruction(Instruction2<C>),
}

#[derive(Clone, Debug)]
pub struct ContinueInsideInstruction2<C: Composition> {
    pub instruction: Instruction2<C>,
    pub index: usize,
    pub local: HashMap<String, C::SmtExpression>,
}

#[derive(Clone, Debug)]
pub struct GAState2<C: Composition> {
    pub project: <C::Memory as SmtMap>::ProgramMemory,
    pub memory: <C::SMT as SmtSolver>::Memory,
    pub state: C::StateContainer,
    pub constraints: C::SMT,
    pub hooks: HookContainer<C>,
    pub count_cycles: bool,
    pub cycle_count: u64,
    pub last_instruction: Option<Instruction2<C>>,
    pub last_pc: u64,
    pub continue_in_instruction: Option<ContinueInsideInstruction2<C>>,
    pub current_instruction: Option<Instruction2<C>>,
    pub any_counter: u64,
    instruction_counter: usize,
    has_jumped: bool,
    instruction_conditions: VecDeque<Condition>,
}

impl<C: Composition> GAState2<C> {
    /// Create a new state.
    pub fn new(
        ctx: C::SMT,
        constraints: C::SMT,
        project: <C::Memory as SmtMap>::ProgramMemory,
        hooks: HookContainer<C>,
        function: &str,
        end_address: u64,
        state: C::StateContainer,
    ) -> std::result::Result<Self, GAError> {
        let pc_reg = match project.get_symbol_address(function) {
            Some(a) => a,
            None => return Err(GAError::EntryFunctionNotFound(function.to_owned())),
        };
        debug!("Found function at addr: {:#X}.", pc_reg);
        let ptr_size = project.get_ptr_size();

        let sp_reg = match project.get_symbol_address("_stack_start") {
            Some(a) => Ok(a),
            None => Err(ProjectError::UnableToParseElf(
                "start of stack not found".to_owned(),
            )),
        }?;
        debug!("Found stack start at addr: {:#X}.", sp_reg);

        let endianness = project.get_endianness();
        let memory = C::Memory::new(ctx.clone(), project.clone(), ptr_size as usize, endianness)?;
        let mut registers = HashMap::new();
        let pc_expr = ctx.from_u64(pc_reg, ptr_size as u32);
        registers.insert("PC".to_owned(), pc_expr);

        let sp_expr = ctx.from_u64(sp_reg, ptr_size as u32);
        registers.insert("SP".to_owned(), sp_expr);

        // Set the link register to max value to detect when returning from a function.
        let end_pc_expr = ctx.from_u64(end_address, ptr_size as u32);
        registers.insert("LR".to_owned(), end_pc_expr);

        let mut flags = HashMap::new();
        flags.insert("N".to_owned(), ctx.unconstrained(1, "flags.N"));
        flags.insert("Z".to_owned(), ctx.unconstrained(1, "flags.Z"));
        flags.insert("C".to_owned(), ctx.unconstrained(1, "flags.C"));
        flags.insert("V".to_owned(), ctx.unconstrained(1, "flags.V"));

        Ok(Self {
            project,
            constraints,
            memory,
            hooks,
            state,
            count_cycles: true,
            cycle_count: 0,
            last_instruction: None,
            last_pc: 0,
            continue_in_instruction: None,
            current_instruction: None,
            instruction_counter: 0,
            has_jumped: false,
            instruction_conditions: VecDeque::new(),
            any_counter: 0,
        })
    }

    pub fn label_new_symbolic(&mut self, start: &str) -> String {
        let ret = format!("{start}_{}", self.any_counter);
        self.any_counter += 1;
        ret
    }

    pub fn reset_has_jumped(&mut self) {
        self.has_jumped = false;
    }

    pub fn set_has_jumped(&mut self) {
        self.has_jumped = true;
    }

    /// Indicates if the last executed instruction was a conditional branch that
    /// branched.
    pub fn get_has_jumped(&self) -> bool {
        self.has_jumped
    }

    /// Increments the instruction counter by one.
    pub fn increment_instruction_count(&mut self) {
        self.instruction_counter += 1;
    }

    /// Gets the current instruction count
    pub fn get_instruction_count(&self) -> usize {
        self.instruction_counter
    }

    /// Gets the last instruction that was executed.
    pub fn get_last_instruction(&self) -> Option<Instruction2<C>> {
        self.last_instruction.clone()
    }

    /// Checks if the execution is currently inside of a conditional block.
    pub fn get_in_conditional_block(&self) -> bool {
        !self.instruction_conditions.is_empty()
    }

    /// Increment the cycle counter with the cycle count of the last
    /// instruction.
    pub fn increment_cycle_count(&mut self) {
        // Do nothing if cycles should not be counted
        if !self.count_cycles {
            return;
        }

        let cycles = match &self.last_instruction {
            Some(i) => match i.max_cycle {
                super::instruction::CycleCount2::Value(v) => v,
                super::instruction::CycleCount2::Function(f) => f(self),
            },
            None => 0,
        };
        trace!(
            "Incrementing cycles: {}, for {:?}",
            cycles,
            self.last_instruction
        );
        self.cycle_count += cycles as u64;
    }

    /// Update the last instruction that was executed.
    pub fn set_last_instruction(&mut self, instruction: Instruction2<C>) {
        self.last_instruction = Some(instruction);
    }

    pub fn add_instruction_conditions(&mut self, conditions: &Vec<Condition>) {
        for condition in conditions {
            self.instruction_conditions.push_back(condition.to_owned());
        }
    }

    pub fn get_next_instruction_condition_expression(&mut self) -> Option<C::SmtExpression> {
        // TODO add error handling
        self.instruction_conditions
            .pop_front()
            .map(|condition| self.get_expr(&condition).unwrap())
    }

    ///// Create a state used for testing.
    //pub fn create_test_state(
    //    project: &'static Project<C::Architecture>,
    //    ctx: C::SMT,
    //    constraints: DSolver,
    //    start_pc: u64,
    //    start_stack: u64,
    //) -> Self {
    //    let pc_reg = start_pc;
    //    let ptr_size = project.get_ptr_size();
    //
    //    let sp_reg = start_stack;
    //    debug!("Found stack start at addr: {:#X}.", sp_reg);
    //
    //    let memory = C::Memory::new(ctx, ptr_size, project.get_endianness());
    //    let mut registers = HashMap::new();
    //    let pc_expr = ctx.from_u64(pc_reg, ptr_size);
    //    registers.insert("PC".to_owned(), pc_expr);
    //
    //    let sp_expr = ctx.from_u64(sp_reg, ptr_size);
    //    registers.insert("SP".to_owned(), sp_expr);
    //
    //    let mut flags = HashMap::new();
    //    flags.insert("N".to_owned(), ctx.unconstrained(1, "flags.N"));
    //    flags.insert("Z".to_owned(), ctx.unconstrained(1, "flags.Z"));
    //    flags.insert("C".to_owned(), ctx.unconstrained(1, "flags.C"));
    //    flags.insert("V".to_owned(), ctx.unconstrained(1, "flags.V"));
    //
    //    GAState2 {
    //        project,
    //        memory,
    //        state,
    //        count_cycles: true,
    //        cycle_count: 0,
    //        last_instruction: None,
    //        last_pc: start_pc,
    //        continue_in_instruction: None,
    //        current_instruction: None,
    //        instruction_counter: 0,
    //        has_jumped: false,
    //        instruction_conditions: VecDeque::new(),
    //    }
    //}

    /// Set a value to a register.
    pub fn set_register(&mut self, register: String, expr: C::SmtExpression) -> Result<()> {
        // crude solution should probably change
        if register == "PC" {
            return self
                .hooks
                .writer(&mut self.memory)
                .write_pc(expr.get_constant().ok_or(GAError::NonDeterministicPC)? as u32)
                .map_err(|e| GAError::SmtMemoryError(e));
        }
        match self
            .hooks
            .writer(&mut self.memory)
            .write_register(&register, &expr)
        {
            ResultOrHook::Hook(hook) => hook(self, expr)?,
            ResultOrHook::Hooks(hooks) => {
                for hook in hooks {
                    hook(self, expr.clone())?;
                }
            }
            ResultOrHook::Result(Err(e)) => return Err(GAError::SmtMemoryError(e)),
            _ => {}
        }
        Ok(())
    }

    /// Get the value stored at a register.
    pub fn get_register(&mut self, register: String) -> Result<C::SmtExpression> {
        // crude solution should probably change
        if register == "PC" {
            return self
                .hooks
                .reader(&mut self.memory)
                .read_pc()
                .map_err(|e| GAError::SmtMemoryError(e));
        }
        match self.hooks.reader(&mut self.memory).read_register(&register) {
            ResultOrHook::Hook(hook) => hook(self),
            ResultOrHook::Hooks(_hooks) => todo!("Handle multiple hooks on read"),
            ResultOrHook::Result(Err(e)) => return Err(GAError::SmtMemoryError(e)),
            ResultOrHook::Result(o) => Ok(o?),
        }
    }

    /// Set the value of a flag.
    pub fn set_flag(&mut self, flag: String, expr: C::SmtExpression) -> Result<()> {
        let expr = expr.simplify().simplify();
        trace!("flag {} set to {:?}", flag, expr);
        Ok(self.memory.set_flag(&flag, expr)?)
    }

    /// Get the value of a flag.
    pub fn get_flag(&mut self, flag: String) -> Result<C::SmtExpression> {
        Ok(self.memory.get_flag(&flag)?)
    }

    /// Get the expression for a condition based on the current flag values.
    pub fn get_expr(&mut self, condition: &Condition) -> Result<C::SmtExpression> {
        Ok(match condition {
            Condition::EQ => self.get_flag("Z".to_owned()).unwrap(),
            Condition::NE => self.get_flag("Z".to_owned()).unwrap().not(),
            Condition::CS => self.get_flag("C".to_owned()).unwrap(),
            Condition::CC => self.get_flag("C".to_owned()).unwrap().not(),
            Condition::MI => self.get_flag("N".to_owned()).unwrap(),
            Condition::PL => self.get_flag("N".to_owned()).unwrap().not(),
            Condition::VS => self.get_flag("V".to_owned()).unwrap(),
            Condition::VC => self.get_flag("V".to_owned()).unwrap().not(),
            Condition::HI => {
                let c = self.get_flag("C".to_owned()).unwrap();
                let z = self.get_flag("Z".to_owned()).unwrap().not();
                c.and(&z)
            }
            Condition::LS => {
                let c = self.get_flag("C".to_owned()).unwrap().not();
                let z = self.get_flag("Z".to_owned()).unwrap();
                c.or(&z)
            }
            Condition::GE => {
                let n = self.get_flag("N".to_owned()).unwrap();
                let v = self.get_flag("V".to_owned()).unwrap();
                n.xor(&v).not()
            }
            Condition::LT => {
                let n = self.get_flag("N".to_owned()).unwrap();
                let v = self.get_flag("V".to_owned()).unwrap();
                n.ne(&v)
            }
            Condition::GT => {
                let z = self.get_flag("Z".to_owned()).unwrap();
                let n = self.get_flag("N".to_owned()).unwrap();
                let v = self.get_flag("V".to_owned()).unwrap();
                z.not().and(&n.eq(&v))
            }
            Condition::LE => {
                let z = self.get_flag("Z".to_owned()).unwrap();
                let n = self.get_flag("N".to_owned()).unwrap();
                let v = self.get_flag("V".to_owned()).unwrap();
                z.and(&n.ne(&v))
            }
            Condition::None => self.memory.from_bool(true),
        })
    }

    /// Get the next instruction based on the address in the PC register.
    pub fn get_next_instruction(&self) -> Result<HookOrInstruction2<'_, C>> {
        let pc = self
            .memory
            .get_pc()
            .map(|val| val.get_constant().ok_or(GAError::NonDeterministicPC))??
            & !(0b1); // Not applicable for all architectures TODO: Fix this.;
        match self.hooks.get_pc_hooks(pc as u32) {
            ResultOrHook::Hook(hook) => Ok(HookOrInstruction2::PcHook(hook)),
            ResultOrHook::Hooks(_) => todo!("Handle multiple hooks on a single address"),
            ResultOrHook::Result(pc) => Ok(HookOrInstruction2::Instruction(
                self.project.get_instruction2(pc as u64, self)?,
            )),
        }
    }

    fn read_word_from_memory_no_static(
        &self,
        address: &C::SmtExpression,
    ) -> Result<C::SmtExpression> {
        Ok(self
            .memory
            .get(address, self.project.get_word_size() as usize)?)
    }

    fn write_word_from_memory_no_static(
        &mut self,
        address: &C::SmtExpression,
        value: C::SmtExpression,
    ) -> Result<()> {
        Ok(self.memory.set(address, value)?)
    }

    /// Read a word form memory. Will respect the endianness of the project.
    pub fn read_word_from_memory(&self, address: &C::SmtExpression) -> Result<C::SmtExpression> {
        Ok(self.memory.get(address, self.memory.get_word_size())?)
    }

    /// Write a word to memory. Will respect the endianness of the project.
    pub fn write_word_to_memory(
        &mut self,
        address: &C::SmtExpression,
        value: C::SmtExpression,
    ) -> Result<()> {
        match address.get_constant() {
            Some(address_const) => {
                if self.project.address_in_range(address_const) {
                    Err(GAError::WritingToStaticMemoryProhibited)
                } else {
                    self.write_word_from_memory_no_static(address, value)
                }
            }

            // For non constant addresses always read non_static memory
            None => self.write_word_from_memory_no_static(address, value),
        }
    }

    pub fn instruction_from_array_ptr(&self, data: &[u8]) -> project::Result<Instruction2<C>> {
        self.state
            .as_arch()
            .translate(data, self)
            .map_err(|el| el.into())
    }

    pub fn reader<'a>(&'a mut self) -> Reader<'a, C> {
        self.hooks.reader(&mut self.memory)
    }

    pub fn writer<'a>(&'a mut self) -> Writer<'a, C> {
        self.hooks.writer(&mut self.memory)
    }
}
