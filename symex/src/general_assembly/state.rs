//! Holds the state in general assembly execution.

use std::collections::{HashMap, VecDeque};

use general_assembly::{condition::Condition, operand::DataWord};
use tracing::{debug, trace};

use super::{arch::Arch, instruction::Instruction, project::Project};
use crate::{
    elf_util::{ExpressionType, Variable},
    general_assembly::{
        project::{PCHook, ProjectError},
        GAError,
        Result,
    },
    memory::ArrayMemory,
    smt::{DContext, DExpr, DSolver},
};

pub enum HookOrInstruction<'a, A: Arch> {
    PcHook(&'a PCHook<A>),
    Instruction(Instruction<A>),
}

#[derive(Clone, Debug)]
pub struct ContinueInsideInstruction<A: Arch> {
    pub instruction: Instruction<A>,
    pub index: usize,
    pub local: HashMap<String, DExpr>,
}

#[derive(Clone, Debug)]
pub struct GAState<A: Arch> {
    pub project: &'static Project<A>,
    pub ctx: &'static DContext,
    pub constraints: DSolver,
    pub marked_symbolic: Vec<Variable>,
    pub memory: ArrayMemory,
    pub count_cycles: bool,
    pub cycle_count: usize,
    pub cycle_laps: Vec<(usize, String)>,
    pub last_instruction: Option<Instruction<A>>,
    pub last_pc: u64,
    pub registers: HashMap<String, DExpr>,
    pub continue_in_instruction: Option<ContinueInsideInstruction<A>>,
    pub current_instruction: Option<Instruction<A>>,
    pub architecture: A,
    pc_register: u64, // this register is special
    flags: HashMap<String, DExpr>,
    instruction_counter: usize,
    has_jumped: bool,
    instruction_conditions: VecDeque<Condition>,
}

impl<A: Arch> GAState<A> {
    /// Create a new state.
    pub fn new(
        ctx: &'static DContext,
        project: &'static Project<A>,
        constraints: DSolver,
        function: &str,
        end_address: u64,
        architecture: A,
    ) -> Result<Self> {
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

        let memory = ArrayMemory::new(ctx, ptr_size, project.get_endianness());
        let mut registers = HashMap::new();
        let pc_expr = ctx.from_u64(pc_reg, ptr_size);
        registers.insert("PC".to_owned(), pc_expr);

        let sp_expr = ctx.from_u64(sp_reg, ptr_size);
        registers.insert("SP".to_owned(), sp_expr);

        // set the link register to max value to detect when returning from a function
        let end_pc_expr = ctx.from_u64(end_address, ptr_size);
        registers.insert("LR".to_owned(), end_pc_expr);

        let mut flags = HashMap::new();
        flags.insert("N".to_owned(), ctx.unconstrained(1, "flags.N"));
        flags.insert("Z".to_owned(), ctx.unconstrained(1, "flags.Z"));
        flags.insert("C".to_owned(), ctx.unconstrained(1, "flags.C"));
        flags.insert("V".to_owned(), ctx.unconstrained(1, "flags.V"));

        Ok(GAState {
            project,
            ctx,
            constraints,
            marked_symbolic: Vec::new(),
            memory,
            cycle_count: 0,
            cycle_laps: vec![],
            registers,
            pc_register: pc_reg,
            flags,
            instruction_counter: 0,
            has_jumped: false,
            last_instruction: None,
            last_pc: pc_reg,
            count_cycles: true,
            continue_in_instruction: None,
            current_instruction: None,
            instruction_conditions: VecDeque::new(),
            architecture,
        })
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
    pub fn get_last_instruction(&self) -> Option<Instruction<A>> {
        self.last_instruction.clone()
    }

    /// Checks if the execution is currently inside of a conditional block.
    pub fn get_in_conditional_block(&self) -> bool {
        !self.instruction_conditions.is_empty()
    }

    /// Increment the cycle counter with the cycle count of the last
    /// instruction.
    pub fn increment_cycle_count(&mut self) {
        // do nothing if cycles should not be counted
        if !self.count_cycles {
            return;
        }

        let cycles = match &self.last_instruction {
            Some(i) => match i.max_cycle {
                super::instruction::CycleCount::Value(v) => v,
                super::instruction::CycleCount::Function(f) => f(self),
            },
            None => 0,
        };
        trace!(
            "Incrementing cycles: {}, for {:?}",
            cycles,
            self.last_instruction
        );
        self.cycle_count += cycles;
    }

    /// Update the last instruction that was executed.
    pub fn set_last_instruction(&mut self, instruction: Instruction<A>) {
        self.last_instruction = Some(instruction);
    }

    pub fn add_instruction_conditions(&mut self, conditions: &Vec<Condition>) {
        for condition in conditions {
            self.instruction_conditions.push_back(condition.to_owned());
        }
    }

    pub fn get_next_instruction_condition_expression(&mut self) -> Option<DExpr> {
        // TODO add error handling
        self.instruction_conditions
            .pop_front()
            .map(|condition| self.get_expr(&condition).unwrap())
    }

    /// Create a state used for testing.
    pub fn create_test_state(
        project: &'static Project<A>,
        ctx: &'static DContext,
        constraints: DSolver,
        start_pc: u64,
        start_stack: u64,
        architecture: A,
    ) -> Self {
        let pc_reg = start_pc;
        let ptr_size = project.get_ptr_size();

        let sp_reg = start_stack;
        debug!("Found stack start at addr: {:#X}.", sp_reg);

        let memory = ArrayMemory::new(ctx, ptr_size, project.get_endianness());
        let mut registers = HashMap::new();
        let pc_expr = ctx.from_u64(pc_reg, ptr_size);
        registers.insert("PC".to_owned(), pc_expr);

        let sp_expr = ctx.from_u64(sp_reg, ptr_size);
        registers.insert("SP".to_owned(), sp_expr);

        let mut flags = HashMap::new();
        flags.insert("N".to_owned(), ctx.unconstrained(1, "flags.N"));
        flags.insert("Z".to_owned(), ctx.unconstrained(1, "flags.Z"));
        flags.insert("C".to_owned(), ctx.unconstrained(1, "flags.C"));
        flags.insert("V".to_owned(), ctx.unconstrained(1, "flags.V"));

        GAState {
            project,
            ctx,
            constraints,
            marked_symbolic: Vec::new(),
            memory,
            cycle_count: 0,
            cycle_laps: vec![],
            registers,
            pc_register: pc_reg,
            flags,
            instruction_counter: 0,
            has_jumped: false,
            last_instruction: None,
            last_pc: pc_reg,
            count_cycles: true,
            continue_in_instruction: None,
            current_instruction: None,
            instruction_conditions: VecDeque::new(),
            architecture,
        }
    }

    /// Set a value to a register.
    pub fn set_register(&mut self, register: String, expr: DExpr) -> Result<()> {
        // crude solution should prbobly change
        if register == "PC" {
            let value = match expr.get_constant() {
                Some(v) => {
                    // assert!(v % 4 == 0);
                    v
                }
                None => {
                    trace!("not a concrete pc try to generate possible values");
                    let values: Vec<u64> = match self.constraints.get_values(&expr, 500).unwrap() {
                        crate::smt::Solutions::Exactly(v) => v
                            .iter()
                            .map(|n| match n.get_constant() {
                                Some(v) => v,
                                None => todo!("e"),
                            })
                            .collect(),
                        crate::smt::Solutions::AtLeast(_v) => todo!(),
                    };
                    trace!("{} possible PC values", values.len());
                    for v in values {
                        trace!("Possible PC: {:#X}", v);
                    }
                    todo!("handel symbolic branch")
                }
            };
            self.pc_register = value;
        }

        match self.project.get_register_write_hook(&register) {
            Some(hook) => hook(self, expr),
            None => {
                self.registers.insert(register, expr);
                Ok(())
            }
        }
    }

    /// Get the value stored at a register.
    pub fn get_register(&mut self, register: String) -> Result<DExpr> {
        // check register hooks
        match self.project.get_register_read_hook(&register) {
            // run hook if found
            Some(hook) => Ok(hook(self)?),
            // if no hook found read like normal
            None => match self.registers.get(&register) {
                Some(v) => Ok(v.to_owned()),
                None => {
                    // If register do not exist yet create it with unconstrained value.
                    let value = self
                        .ctx
                        .unconstrained(self.project.get_word_size(), &register);
                    self.marked_symbolic.push(Variable {
                        name: Some(register.to_owned()),
                        value: value.clone(),
                        ty: ExpressionType::Integer(self.project.get_word_size() as usize),
                    });
                    self.registers.insert(register.to_owned(), value.to_owned());
                    Ok(value)
                }
            },
        }
    }

    /// Set the value of a flag.
    pub fn set_flag(&mut self, flag: String, expr: DExpr) {
        let expr = expr.simplify().simplify();
        trace!("flag {} set to {:?}", flag, expr);
        self.flags.insert(flag, expr);
    }

    /// Get the value of a flag.
    pub fn get_flag(&mut self, flag: String) -> Option<DExpr> {
        match self.flags.get(&flag) {
            Some(v) => Some(v.to_owned()),
            None => todo!(),
        }
    }

    /// Get the expression for a condition based on the current flag values.
    pub fn get_expr(&mut self, condition: &Condition) -> Result<DExpr> {
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
                n._ne(&v)
            }
            Condition::GT => {
                let z = self.get_flag("Z".to_owned()).unwrap();
                let n = self.get_flag("N".to_owned()).unwrap();
                let v = self.get_flag("V".to_owned()).unwrap();
                z.not().and(&n._eq(&v))
            }
            Condition::LE => {
                let z = self.get_flag("Z".to_owned()).unwrap();
                let n = self.get_flag("N".to_owned()).unwrap();
                let v = self.get_flag("V".to_owned()).unwrap();
                z.and(&n._ne(&v))
            }
            Condition::None => self.ctx.from_bool(true),
        })
    }

    /// Get the next instruction based on the address in the PC register.
    pub fn get_next_instruction(&self) -> Result<HookOrInstruction<A>> {
        let pc = self.pc_register & !(0b1); // Not applicable for all architectures TODO: Fix this.;
        match self.project.get_pc_hook(pc) {
            Some(hook) => Ok(HookOrInstruction::PcHook(hook)),
            None => Ok(HookOrInstruction::Instruction(
                self.project.get_instruction(pc, self)?,
            )),
        }
    }

    fn read_word_from_memory_no_static(&self, address: &DExpr) -> Result<DExpr> {
        Ok(self.memory.read(address, self.project.get_word_size())?)
    }

    fn write_word_from_memory_no_static(&mut self, address: &DExpr, value: DExpr) -> Result<()> {
        Ok(self.memory.write(address, value)?)
    }

    /// Read a word form memory. Will respect the endianness of the project.
    pub fn read_word_from_memory(&self, address: &DExpr) -> Result<DExpr> {
        match address.get_constant() {
            Some(address_const) => {
                if self.project.address_in_range(address_const) {
                    // read from static memory in project
                    let value = match self.project.get_word(address_const)? {
                        DataWord::Word64(data) => self.ctx.from_u64(data, 64),
                        DataWord::Word32(data) => self.ctx.from_u64(data as u64, 32),
                        DataWord::Word16(data) => self.ctx.from_u64(data as u64, 16),
                        DataWord::Word8(data) => self.ctx.from_u64(data as u64, 8),
                    };
                    Ok(value)
                } else {
                    self.read_word_from_memory_no_static(address)
                }
            }

            // For non constant addresses always read non_static memory
            None => self.read_word_from_memory_no_static(address),
        }
    }

    /// Write a word to memory. Will respect the endianess of the project.
    pub fn write_word_to_memory(&mut self, address: &DExpr, value: DExpr) -> Result<()> {
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

    pub fn instruction_from_array_ptr(
        &self,
        data: &[u8],
    ) -> crate::general_assembly::project::Result<Instruction<A>> {
        self.architecture
            .translate(data, self)
            .map_err(|el| el.into())
    }
}
