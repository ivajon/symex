//! General assembly executor

use std::collections::HashMap;

use general_assembly::{
    operand::{DataWord, Operand},
    operation::Operation,
    shift::Shift,
};
use tracing::{debug, trace};

use super::{
    arch::Arch,
    instruction::Instruction,
    project::Project,
    state::{ContinueInsideInstruction, GAState},
    vm::VM,
    Result,
};
use crate::{
    general_assembly::{path_selection::Path, state::HookOrInstruction},
    smt::{smt_boolector::BoolectorSolverContext, DExpr, SolverError},
};

pub struct GAExecutor<'vm, A: Arch + Clone + 'static> {
    pub vm: &'vm mut VM<A>,
    pub state: GAState<A>,
    pub project: &'static Project<A>,
    //current_instruction: Option<Instruction>,
    current_operation_index: usize,
}

pub enum PathResult {
    Success(Option<DExpr>),
    Failure(&'static str),
    AssumptionUnsat,
    Suppress,
}

struct AddWithCarryResult {
    carry_out: DExpr,
    overflow: DExpr,
    result: DExpr,
}

impl<'vm, A: Arch> GAExecutor<'vm, A> {
    /// Construct a executor from a state.
    pub fn from_state(state: GAState<A>, vm: &'vm mut VM<A>, project: &'static Project<A>) -> Self {
        Self {
            vm,
            state,
            project,
            //current_instruction: None,
            current_operation_index: 0,
        }
    }

    pub fn resume_execution(&mut self) -> Result<PathResult> {
        let possible_continue = self.state.continue_in_instruction.to_owned();

        if let Some(i) = possible_continue {
            self.continue_executing_instruction(&i)?;
            self.state.continue_in_instruction = None;
            self.state.set_last_instruction(i.instruction);
        }

        loop {
            let instruction = match self.state.get_next_instruction()? {
                HookOrInstruction::Instruction(v) => v,
                HookOrInstruction::PcHook(hook) => match hook {
                    crate::general_assembly::project::PCHook::Continue => {
                        debug!("Continuing");
                        let lr = self.state.get_register("LR".to_owned()).unwrap();
                        self.state.set_register("PC".to_owned(), lr)?;
                        continue;
                    }
                    crate::general_assembly::project::PCHook::EndSuccess => {
                        debug!("Symbolic execution ended successfully");
                        self.state.increment_cycle_count();
                        return Ok(PathResult::Success(None));
                    }
                    crate::general_assembly::project::PCHook::EndFailure(reason) => {
                        debug!("Symbolic execution ended unsuccessfully");
                        let data = *reason;
                        self.state.increment_cycle_count();
                        return Ok(PathResult::Failure(data));
                    }
                    crate::general_assembly::project::PCHook::Suppress => {
                        self.state.increment_cycle_count();
                        return Ok(PathResult::Suppress);
                    }
                    crate::general_assembly::project::PCHook::Intrinsic(f) => {
                        f(&mut self.state)?;

                        // set last instruction to empty to no count instruction twice
                        self.state.last_instruction = None;
                        continue;
                    }
                },
            };

            // Add cycles to cycle count
            self.state.increment_cycle_count();

            trace!("executing instruction: {:?}", instruction);
            self.execute_instruction(&instruction)?;

            self.state.set_last_instruction(instruction);
        }
    }

    // Fork execution. Will create a new path with `constraint`.
    fn fork(&mut self, constraint: DExpr) -> Result<()> {
        trace!("Save backtracking path: constraint={:?}", constraint);
        let forked_state = self.state.clone();
        let path = Path::new(forked_state, Some(constraint));

        self.vm.paths.save_path(path);
        Ok(())
    }

    /// Creates smt expression from a dataword.
    fn get_dexpr_from_dataword(&mut self, data: DataWord) -> DExpr {
        match data {
            DataWord::Word64(v) => self.state.ctx.from_u64(v, 64),
            DataWord::Word32(v) => self.state.ctx.from_u64(v as u64, 32),
            DataWord::Word16(v) => self.state.ctx.from_u64(v as u64, 16),
            DataWord::Word8(v) => self.state.ctx.from_u64(v as u64, 8),
        }
    }

    /// Retrieves a smt expression representing value stored at `address` in
    /// memory.
    fn get_memory(&mut self, address: u64, bits: u32) -> Result<DExpr> {
        trace!("Getting memory addr: {:?}", address);
        // check for hook and return early
        if let Some(hook) = self.project.get_memory_read_hook(address) {
            return hook(&mut self.state, address);
        }

        if self.project.address_in_range(address) {
            if bits == self.project.get_word_size() {
                // full word
                Ok(self.get_dexpr_from_dataword(self.project.get_word(address)?))
            } else if bits == self.project.get_word_size() / 2 {
                // half word
                Ok(self.get_dexpr_from_dataword(self.project.get_half_word(address)?.into()))
            } else if bits == 8 {
                // byte
                Ok(self
                    .state
                    .ctx
                    .from_u64(self.project.get_byte(address)? as u64, 8))
            } else {
                todo!()
            }
        } else {
            let symbolic_address = self
                .state
                .ctx
                .from_u64(address, self.project.get_ptr_size());
            let data = self.state.memory.read(&symbolic_address, bits)?;
            Ok(data)
        }
    }

    /// Sets the memory at `address` to `data`.
    fn set_memory(&mut self, data: DExpr, address: u64, bits: u32) -> Result<()> {
        trace!("Setting memory addr: {:?}", address);
        // check for hook and return early
        if let Some(hook) = self.project.get_memory_write_hook(address) {
            return hook(&mut self.state, address, data, bits);
        }

        if self.project.address_in_range(address) {
            Err(super::GAError::WritingToStaticMemoryProhibited)
        } else {
            let symbolic_address = self
                .state
                .ctx
                .from_u64(address, self.project.get_ptr_size());
            self.state
                .memory
                .write(&symbolic_address, data.resize_unsigned(bits).simplify())?;
            Ok(())
        }
    }

    /// Get the smt expression for a operand.
    pub(crate) fn get_operand_value(
        &mut self,
        operand: &Operand,
        local: &HashMap<String, DExpr>,
    ) -> Result<DExpr> {
        let ret = match operand {
            Operand::Register(name) => Ok(self.state.get_register(name.to_owned())?),
            Operand::Immediate(v) => Ok(self.get_dexpr_from_dataword(v.to_owned())),
            Operand::Address(address, width) => {
                let address = self.get_dexpr_from_dataword(*address);
                let address = self.resolve_address(address, local)?;
                self.get_memory(address, *width)
            }
            Operand::AddressWithOffset {
                address: _,
                offset_reg: _,
                width: _,
            } => todo!(),
            Operand::Local(k) => Ok((local.get(k).unwrap()).to_owned()),
            Operand::AddressInLocal(local_name, width) => {
                let address =
                    self.get_operand_value(&Operand::Local(local_name.to_owned()), local)?;
                let address = self.resolve_address(address, local)?;
                self.get_memory(address, *width)
            }
            Operand::Flag(f) => {
                let value = self.state.get_flag(f.clone());
                match value {
                    Some(value) => Ok(value.resize_unsigned(self.project.get_word_size())),
                    None => todo!(),
                }
            }
        };

        ret
    }

    /// Sets what the operand represents to `value`.
    pub(crate) fn set_operand_value(
        &mut self,
        operand: &Operand,
        value: DExpr,
        local: &mut HashMap<String, DExpr>,
    ) -> Result<()> {
        match operand {
            Operand::Register(v) => {
                trace!("Setting register {} to {:?}", v, value);
                self.state.set_register(v.to_owned(), value)?
            }
            Operand::Immediate(_) => panic!(), // not prohibited change to error later
            Operand::AddressInLocal(local_name, width) => {
                let address =
                    self.get_operand_value(&Operand::Local(local_name.to_owned()), local)?;
                let address = self.resolve_address(address, local)?;
                self.set_memory(value.simplify(), address, *width)?;
            }
            Operand::Address(address, width) => {
                let address = self.get_dexpr_from_dataword(*address);
                let address = self.resolve_address(address, local)?;
                self.set_memory(value.simplify(), address, *width)?;
            }
            Operand::AddressWithOffset {
                address: _,
                offset_reg: _,
                width: _,
            } => todo!(),
            Operand::Local(k) => {
                local.insert(k.to_owned(), value);
            }
            Operand::Flag(f) => {
                // TODO!
                //
                // Might be a good thing to throw an error here if the value is not 0 or 1.
                self.state
                    .set_flag(f.clone(), value.resize_unsigned(1).simplify());
            }
        }
        Ok(())
    }

    fn resolve_address(&mut self, address: DExpr, local: &HashMap<String, DExpr>) -> Result<u64> {
        match &address.get_constant() {
            Some(addr) => Ok(*addr),
            None => {
                // find all possible addresses
                let addresses = self.state.constraints.get_values(&address, 255)?;

                let addresses = match addresses {
                    crate::smt::Solutions::Exactly(a) => Ok(a),
                    crate::smt::Solutions::AtLeast(_) => Err(SolverError::TooManySolutions),
                }?;

                if addresses.len() == 1 {
                    return Ok(addresses[0].get_constant().unwrap());
                }

                if addresses.is_empty() {
                    return Err(SolverError::Unsat.into());
                }

                // create paths for all but the first address
                for addr in &addresses[1..] {
                    if self.current_operation_index
                        < self
                            .state
                            .current_instruction
                            .as_ref()
                            .unwrap()
                            .operations
                            .len()
                            - 1
                    {
                        self.state.continue_in_instruction = Some(ContinueInsideInstruction {
                            instruction: self
                                .state
                                .current_instruction
                                .as_ref()
                                .unwrap()
                                .to_owned(),
                            index: self.current_operation_index,
                            local: local.clone(),
                        })
                    }

                    let constraint = address._eq(addr);
                    self.fork(constraint)?;
                }

                // assert first address and return concrete
                let concrete_address = &addresses[0];
                self.state
                    .constraints
                    .assert(&address._eq(concrete_address));
                Ok(concrete_address.get_constant().unwrap())
            }
        }
    }

    fn continue_executing_instruction(
        &mut self,
        inst_to_continue: &ContinueInsideInstruction<A>,
    ) -> Result<()> {
        let mut local = inst_to_continue.local.to_owned();
        self.state.current_instruction = Some(inst_to_continue.instruction.to_owned());
        for i in inst_to_continue.index..inst_to_continue.instruction.operations.len() {
            let operation = &inst_to_continue.instruction.operations[i];
            self.current_operation_index = i;
            self.execute_operation(operation, &mut local)?;
        }
        Ok(())
    }

    /// Execute a single instruction.
    pub(crate) fn execute_instruction(&mut self, i: &Instruction<A>) -> Result<()> {
        // update last pc
        let new_pc = self.state.get_register("PC".to_owned())?;
        self.state.last_pc = new_pc.get_constant().unwrap();

        // Always increment pc before executing the operations
        self.state.set_register(
            "PC".to_owned(),
            new_pc.add(
                &self
                    .state
                    .ctx
                    .from_u64((i.instruction_size / 8) as u64, self.project.get_ptr_size()),
            ),
        )?;

        // reset has branched before execution of instruction.
        self.state.reset_has_jumped();

        // increment instruction count before execution
        // so that forked path count this instruction
        self.state.increment_instruction_count();

        self.state.current_instruction = Some(i.to_owned());

        // check if we should actually execute the instruction
        let should_run = match self.state.get_next_instruction_condition_expression() {
            Some(c) => match c.get_constant_bool() {
                Some(constant_c) => constant_c,
                None => {
                    let true_possible = self.state.constraints.is_sat_with_constraint(&c)?;
                    let false_possible = self.state.constraints.is_sat_with_constraint(&c.not())?;

                    if true_possible && false_possible {
                        self.fork(c.not())?;
                        self.state.constraints.assert(&c);
                    }

                    true_possible
                }
            },
            None => true,
        };

        if should_run {
            // initiate local variable storage
            let mut local: HashMap<String, DExpr> = HashMap::new();
            for (n, operation) in i.operations.iter().enumerate() {
                self.current_operation_index = n;
                self.execute_operation(operation, &mut local)?;
            }
        }

        Ok(())
    }

    /// Execute a single operation or all operations contained inside a
    /// operation.
    pub(crate) fn execute_operation(
        &mut self,
        operation: &Operation,
        local: &mut HashMap<String, DExpr>,
    ) -> Result<()> {
        trace!("Executing operation: {:?}", operation);
        match operation {
            Operation::Nop => (), // nop so do nothing
            Operation::Move {
                destination,
                source,
            } => {
                let value = self.get_operand_value(source, local)?.simplify();
                self.set_operand_value(destination, value.clone(), local)?;
            }
            Operation::Add {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.add(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Sub {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.sub(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Mul {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.mul(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::UDiv {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.udiv(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::SDiv {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.sdiv(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::And {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.and(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Or {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.or(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Xor {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let result = op1.xor(&op2);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Not {
                destination,
                operand,
            } => {
                let op = self.get_operand_value(operand, local)?;

                let result = op.not();
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Shift {
                destination,
                operand,
                shift_n,
                shift_t,
            } => {
                let value = self.get_operand_value(operand, local)?;
                let shift_amount = self.get_operand_value(shift_n, local)?;
                let result = match shift_t {
                    Shift::Lsl => value.sll(&shift_amount),
                    Shift::Lsr => value.srl(&shift_amount),
                    Shift::Asr => value.sra(&shift_amount),
                    Shift::Rrx => {
                        let ret = value
                            .and(&shift_amount.sub(&self.state.ctx.from_u64(1, 32)))
                            .srl(&self.state.ctx.from_u64(1, 32))
                            .simplify();
                        ret.or(&self
                            .state
                            // Set the carry bit right above the last bit
                            .get_flag("C".to_owned())
                            .unwrap()
                            .sll(&shift_amount.add(&self.state.ctx.from_u64(1, 32))))
                    }
                    Shift::Ror => {
                        let word_size = self.state.ctx.from_u64(
                            self.project.get_word_size() as u64,
                            self.project.get_word_size(),
                        );
                        value
                            .srl(&shift_amount)
                            .or(&value.srl(&word_size.sub(&shift_amount)))
                    }
                };
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Sl {
                destination,
                operand,
                shift,
            } => {
                let value = self.get_operand_value(operand, local)?;
                let shift_amount = self.get_operand_value(shift, local)?;
                let result = value.sll(&shift_amount);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Srl {
                destination,
                operand,
                shift,
            } => {
                let value = self.get_operand_value(operand, local)?;
                let shift_amount = self.get_operand_value(shift, local)?;
                let result = value.srl(&shift_amount);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Sra {
                destination,
                operand,
                shift,
            } => {
                let value = self.get_operand_value(operand, local)?;
                let shift_amount = self.get_operand_value(shift, local)?;
                let result = value.sra(&shift_amount);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Sror {
                destination,
                operand,
                shift,
            } => {
                let word_size = self.state.ctx.from_u64(
                    self.project.get_word_size() as u64,
                    self.project.get_word_size(),
                );
                let value = self.get_operand_value(operand, local)?;
                let shift = self.get_operand_value(shift, local)?.srem(&word_size);
                let result = value.srl(&shift).or(&value.sll(&word_size.sub(&shift)));
                self.set_operand_value(destination, result, local)?;
            }
            Operation::ConditionalJump {
                destination,
                condition,
            } => {
                let dest_value = self.get_operand_value(destination, local)?;
                let c = self.state.get_expr(condition)?.simplify();
                trace!("conditional expr: {:?}", c);

                // if constant just jump
                if let Some(constant_c) = c.get_constant_bool() {
                    if constant_c {
                        self.state.set_has_jumped();
                        let destination = dest_value;
                        self.state.set_register("PC".to_owned(), destination)?;
                    }
                    return Ok(());
                }

                let true_possible = self.state.constraints.is_sat_with_constraint(&c)?;
                let false_possible = self.state.constraints.is_sat_with_constraint(&c.not())?;
                trace!(
                    "true possible: {} false possible: {}",
                    true_possible,
                    false_possible
                );

                let destination: DExpr = match (true_possible, false_possible) {
                    (true, true) => {
                        if self.current_operation_index
                            < (self
                                .state
                                .current_instruction
                                .as_ref()
                                .unwrap()
                                .operations
                                .len()
                                - 1)
                        {
                            self.state.continue_in_instruction = Some(ContinueInsideInstruction {
                                instruction: self
                                    .state
                                    .current_instruction
                                    .as_ref()
                                    .unwrap()
                                    .to_owned(),
                                index: self.current_operation_index + 1,
                                local: local.to_owned(),
                            });
                        }
                        self.fork(c.not())?;
                        self.state.constraints.assert(&c);
                        self.state.set_has_jumped();
                        Ok(dest_value)
                    }
                    (true, false) => {
                        self.state.set_has_jumped();
                        Ok(dest_value)
                    }
                    (false, true) => Ok(self.state.get_register("PC".to_owned())?), /* safe to asume PC exist */
                    (false, false) => Err(SolverError::Unsat),
                }?;

                self.state.set_register("PC".to_owned(), destination)?;
            }
            Operation::ConditionalExecution { conditions } => {
                self.state.add_instruction_conditions(conditions);
            }
            Operation::SetNFlag(operand) => {
                let value = self.get_operand_value(operand, local)?;
                let shift = self
                    .state
                    .ctx
                    .from_u64((self.project.get_word_size() - 1) as u64, 32);
                let result = value.srl(&shift).resize_unsigned(1);
                self.state.set_flag("N".to_owned(), result);
            }
            Operation::SetZFlag(operand) => {
                let value = self.get_operand_value(operand, local)?;
                let result = value._eq(&self.state.ctx.zero(self.project.get_word_size()));
                self.state.set_flag("Z".to_owned(), result);
            }
            Operation::SetCFlag {
                operand1,
                operand2,
                sub,
                carry,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let one = self.state.ctx.from_u64(1, self.project.get_word_size());

                let result = match (sub, carry) {
                    (true, true) => {
                        // I do not now if this part is used in any ISA but it is here for
                        // completeness.
                        let carry_in = self.state.get_flag("C".to_owned()).unwrap();
                        let op2 = op2.not();

                        // Check for carry on twos complement of op2
                        // Fixes edgecase op2 = 0.
                        let c2 = op2.uaddo(&one);

                        add_with_carry(
                            &op1,
                            &op2.add(&one),
                            &carry_in,
                            self.project.get_word_size(),
                        )
                        .carry_out
                        .or(&c2)
                    }
                    (true, false) => {
                        let lhs = op1;
                        let rhs = op2.not();
                        add_with_carry(&lhs, &rhs, &one, self.project.get_word_size()).carry_out
                    }
                    (false, true) => {
                        let carry_in = self.state.get_flag("C".to_owned()).unwrap();
                        add_with_carry(&op1, &op2, &carry_in, self.project.get_word_size())
                            .carry_out
                    }
                    (false, false) => op1.uaddo(&op2),
                };

                self.state.set_flag("C".to_owned(), result);
            }
            Operation::SetVFlag {
                operand1,
                operand2,
                sub,
                carry,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let one = self.state.ctx.from_u64(1, self.project.get_word_size());

                let result = match (sub, carry) {
                    (true, true) => {
                        // slightly wrong at op2 = 0
                        let carry_in = self.state.get_flag("C".to_owned()).unwrap();
                        let op2 = op2.not().add(&one);
                        add_with_carry(&op1, &op2, &carry_in, self.project.get_word_size()).overflow
                    }
                    (true, false) => {
                        add_with_carry(&op1, &op2.not(), &one, self.project.get_word_size())
                            .overflow
                    }
                    (false, true) => {
                        let carry_in = self.state.get_flag("C".to_owned()).unwrap();
                        add_with_carry(&op1, &op2, &carry_in, self.project.get_word_size()).overflow
                    }
                    (false, false) => op1.saddo(&op2),
                };

                self.state.set_flag("V".to_owned(), result);
            }
            Operation::ForEach {
                operands: _,
                operations: _,
            } => {
                todo!()
            }
            Operation::ZeroExtend {
                destination,
                operand,
                bits,
            } => {
                let op = self.get_operand_value(operand, local)?;
                let valid_bits = op.resize_unsigned(*bits);
                let result = valid_bits.zero_ext(self.project.get_word_size());
                self.set_operand_value(destination, result, local)?;
            }
            Operation::SignExtend {
                destination,
                operand,
                bits,
            } => {
                let op = self.get_operand_value(operand, local)?;
                let valid_bits = op.resize_unsigned(*bits);
                let result = valid_bits.sign_ext(self.project.get_word_size());
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Resize {
                destination,
                operand,
                bits,
            } => {
                let op = self.get_operand_value(operand, local)?;
                let result = op.resize_unsigned(*bits);
                self.set_operand_value(destination, result, local)?;
            }
            Operation::Adc {
                destination,
                operand1,
                operand2,
            } => {
                let op1 = self.get_operand_value(operand1, local)?;
                let op2 = self.get_operand_value(operand2, local)?;
                let carry = self
                    .state
                    .get_flag("C".to_owned())
                    .unwrap()
                    .zero_ext(self.project.get_word_size());
                let result =
                    add_with_carry(&op1, &op2, &carry, self.project.get_word_size()).result;
                self.set_operand_value(destination, result, local)?;
            }
            // These need to be tested are way to complex to be trusted
            Operation::SetCFlagShiftLeft { operand, shift } => {
                let op = self
                    .get_operand_value(operand, local)?
                    .zero_ext(1 + self.project.get_word_size());
                let shift = self
                    .get_operand_value(shift, local)?
                    .zero_ext(1 + self.project.get_word_size());
                let result = op.sll(&shift);
                let carry = result
                    .srl(&self.state.ctx.from_u64(
                        self.project.get_word_size() as u64,
                        self.project.get_word_size() + 1,
                    ))
                    .resize_unsigned(1);
                self.state.set_flag("C".to_owned(), carry);
            }
            Operation::SetCFlagSrl { operand, shift } => {
                let op = self
                    .get_operand_value(operand, local)?
                    .zero_ext(1 + self.project.get_word_size())
                    .sll(&self.state.ctx.from_u64(1, 1 + self.project.get_word_size()));
                let shift = self
                    .get_operand_value(shift, local)?
                    .zero_ext(1 + self.project.get_word_size());
                let result = op.srl(&shift);
                let carry = result.resize_unsigned(1);
                self.state.set_flag("C".to_owned(), carry);
            }
            Operation::SetCFlagSra { operand, shift } => {
                let op = self
                    .get_operand_value(operand, local)?
                    .zero_ext(1 + self.project.get_word_size())
                    .sll(&self.state.ctx.from_u64(1, 1 + self.project.get_word_size()));
                let shift = self
                    .get_operand_value(shift, local)?
                    .zero_ext(1 + self.project.get_word_size());
                let result = op.sra(&shift);
                let carry = result.resize_unsigned(1);
                self.state.set_flag("C".to_owned(), carry);
            }
            Operation::SetCFlagRor(operand) => {
                // this is right for armv6-m but may be wrong for other architectures
                let result = self.get_operand_value(operand, local)?;
                let word_size_minus_one = self.state.ctx.from_u64(
                    self.project.get_word_size() as u64 - 1,
                    self.project.get_word_size(),
                );
                // result = srl(op, shift) OR sll(op, word_size - shift)
                let c = result.srl(&word_size_minus_one).resize_unsigned(1);
                self.state.set_flag("C".to_owned(), c);
            }
            Operation::CountOnes {
                destination,
                operand,
            } => {
                let operand = self.get_operand_value(operand, local)?;
                let result = count_ones(&operand, self.state.ctx, self.project.get_word_size());
                self.set_operand_value(destination, result, local)?;
            }
            Operation::CountZeroes {
                destination,
                operand,
            } => {
                let operand = self.get_operand_value(operand, local)?;
                let result = count_zeroes(&operand, self.state.ctx, self.project.get_word_size());
                self.set_operand_value(destination, result, local)?;
            }
            Operation::CountLeadingOnes {
                destination,
                operand,
            } => {
                let operand = self.get_operand_value(operand, local)?;
                let result =
                    count_leading_ones(&operand, self.state.ctx, self.project.get_word_size());
                self.set_operand_value(destination, result, local)?;
            }
            Operation::CountLeadingZeroes {
                destination,
                operand,
            } => {
                let operand = self.get_operand_value(operand, local)?;
                let result =
                    count_leading_zeroes(&operand, self.state.ctx, self.project.get_word_size());
                self.set_operand_value(destination, result, local)?;
            }
        }
        Ok(())
    }
}

fn count_ones(input: &DExpr, ctx: &BoolectorSolverContext, word_size: u32) -> DExpr {
    let mut count = ctx.from_u64(0, word_size);
    let mask = ctx.from_u64(1, word_size);
    for n in 0..word_size {
        let symbolic_n = ctx.from_u64(n as u64, word_size);
        let to_add = input.srl(&symbolic_n).and(&mask);
        count = count.add(&to_add);
    }
    count
}

fn count_zeroes(input: &DExpr, ctx: &BoolectorSolverContext, word_size: u32) -> DExpr {
    let input = input.not();
    let mut count = ctx.from_u64(0, word_size);
    let mask = ctx.from_u64(1, word_size);
    for n in 0..word_size {
        let symbolic_n = ctx.from_u64(n as u64, word_size);
        let to_add = input.srl(&symbolic_n).and(&mask);
        count = count.add(&to_add);
    }
    count
}

fn count_leading_ones(input: &DExpr, ctx: &BoolectorSolverContext, word_size: u32) -> DExpr {
    let mut count = ctx.from_u64(0, word_size);
    let mut stop_count_mask = ctx.from_u64(1, word_size);
    let mask = ctx.from_u64(1, word_size);
    for n in (0..word_size).rev() {
        let symbolic_n = ctx.from_u64(n as u64, word_size);
        let to_add = input.srl(&symbolic_n).and(&mask).and(&stop_count_mask);
        stop_count_mask = to_add.clone();
        count = count.add(&to_add);
    }
    count
}

fn count_leading_zeroes(input: &DExpr, ctx: &BoolectorSolverContext, word_size: u32) -> DExpr {
    let input = input.not();
    let mut count = ctx.from_u64(0, word_size);
    let mut stop_count_mask = ctx.from_u64(1, word_size);
    let mask = ctx.from_u64(1, word_size);
    for n in (0..word_size).rev() {
        let symbolic_n = ctx.from_u64(n as u64, word_size);
        let to_add = input.srl(&symbolic_n).and(&mask).and(&stop_count_mask);
        stop_count_mask = to_add.clone();
        count = count.add(&to_add);
    }
    count
}

/// Does a add with carry and returns result, carry out and overflow like a hw
/// adder.
fn add_with_carry(
    op1: &DExpr,
    op2: &DExpr,
    carry_in: &DExpr,
    word_size: u32,
) -> AddWithCarryResult {
    let carry_in = carry_in.resize_unsigned(1);
    let c1 = op2.uaddo(&carry_in.zero_ext(word_size));
    let op2 = op2.add(&carry_in.zero_ext(word_size));
    let result = op1.add(&op2);
    let carry = op1.uaddo(&op2).or(&c1);
    let overflow = op1.saddo(&op2);
    AddWithCarryResult {
        carry_out: carry,
        overflow,
        result,
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use general_assembly::{
        condition::Condition,
        operand::{DataWord, Operand},
        operation::Operation,
    };

    use super::{count_leading_ones, count_ones, count_zeroes};
    use crate::{
        general_assembly::{
            arch::arm::v6::ArmV6M,
            executor::{add_with_carry, count_leading_zeroes, GAExecutor},
            instruction::{CycleCount, Instruction},
            project::Project,
            state::GAState,
            vm::VM,
            Endianness,
            WordSize,
        },
        smt::{DContext, DSolver},
    };

    #[test]
    fn test_count_ones_concrete() {
        let ctx = DContext::new();
        let num1 = ctx.from_u64(1, 32);
        let num32 = ctx.from_u64(32, 32);
        let numff = ctx.from_u64(0xff, 32);
        let result = count_ones(&num1, &ctx, 32);
        assert_eq!(result.get_constant().unwrap(), 1);
        let result = count_ones(&num32, &ctx, 32);
        assert_eq!(result.get_constant().unwrap(), 1);
        let result = count_ones(&numff, &ctx, 32);
        assert_eq!(result.get_constant().unwrap(), 8);
    }

    #[test]
    fn test_count_ones_symbolic() {
        let ctx = DContext::new();
        let solver = DSolver::new(&ctx);
        let any_u32 = ctx.unconstrained(32, "any1");
        let num_0x100 = ctx.from_u64(0x100, 32);
        let num_8 = ctx.from_u64(8, 32);
        solver.assert(&any_u32.ult(&num_0x100));
        let result = count_ones(&any_u32, &ctx, 32);
        let result_below_or_equal_8 = result.ulte(&num_8);
        let result_above_8 = result.ugt(&num_8);
        let can_be_below_or_equal_8 = solver
            .is_sat_with_constraint(&result_below_or_equal_8)
            .unwrap();
        let can_be_above_8 = solver.is_sat_with_constraint(&result_above_8).unwrap();
        assert!(can_be_below_or_equal_8);
        assert!(!can_be_above_8);
    }

    #[test]
    fn test_count_zeroes_concrete() {
        let ctx = DContext::new();
        let num1 = ctx.from_u64(!1, 32);
        let num32 = ctx.from_u64(!32, 32);
        let numff = ctx.from_u64(!0xff, 32);
        let result = count_zeroes(&num1, &ctx, 32);
        assert_eq!(result.get_constant().unwrap(), 1);
        let result = count_zeroes(&num32, &ctx, 32);
        assert_eq!(result.get_constant().unwrap(), 1);
        let result = count_zeroes(&numff, &ctx, 32);
        assert_eq!(result.get_constant().unwrap(), 8);
    }

    #[test]
    fn test_count_leading_ones_concrete() {
        let ctx = DContext::new();
        let input = ctx.from_u64(0b1000_0000, 8);
        let result = count_leading_ones(&input, &ctx, 8);
        assert_eq!(result.get_constant().unwrap(), 1);
        let input = ctx.from_u64(0b1100_0000, 8);
        let result = count_leading_ones(&input, &ctx, 8);
        assert_eq!(result.get_constant().unwrap(), 2);
        let input = ctx.from_u64(0b1110_0011, 8);
        let result = count_leading_ones(&input, &ctx, 8);
        assert_eq!(result.get_constant().unwrap(), 3);
    }

    #[test]
    fn test_count_leading_zeroes_concrete() {
        let ctx = DContext::new();
        let input = ctx.from_u64(!0b1000_0000, 8);
        let result = count_leading_zeroes(&input, &ctx, 8);
        assert_eq!(result.get_constant().unwrap(), 1);
        let input = ctx.from_u64(!0b1100_0000, 8);
        let result = count_leading_zeroes(&input, &ctx, 8);
        assert_eq!(result.get_constant().unwrap(), 2);
        let input = ctx.from_u64(!0b1110_0011, 8);
        let result = count_leading_zeroes(&input, &ctx, 8);
        assert_eq!(result.get_constant().unwrap(), 3);
    }

    #[test]
    fn test_add_with_carry() {
        let ctx = DContext::new();
        let one_bool = ctx.from_bool(true);
        let zero_bool = ctx.from_bool(false);
        let zero = ctx.from_u64(0, 32);
        let num42 = ctx.from_u64(42, 32);
        let num16 = ctx.from_u64(16, 32);
        let umax = ctx.from_u64(u32::MAX as u64, 32);
        let smin = ctx.from_u64(i32::MIN as u64, 32);
        let smax = ctx.from_u64(i32::MAX as u64, 32);

        // simple add
        let result = add_with_carry(&num42, &num16, &zero_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 58);
        assert!(!result.carry_out.get_constant_bool().unwrap());
        assert!(!result.overflow.get_constant_bool().unwrap());

        // simple sub
        let result = add_with_carry(&num42, &num16.not(), &one_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 26);
        assert!(result.carry_out.get_constant_bool().unwrap());
        assert!(!result.overflow.get_constant_bool().unwrap());

        // signed sub negative result
        let result = add_with_carry(&num16, &num42.not(), &one_bool, 32);
        assert_eq!(
            result.result.get_constant().unwrap(),
            (-26i32 as u32) as u64
        );
        assert!(!result.carry_out.get_constant_bool().unwrap());
        assert!(!result.overflow.get_constant_bool().unwrap());

        // unsigned overflow
        let result = add_with_carry(&umax, &num16, &zero_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 15 as u64);
        assert!(result.carry_out.get_constant_bool().unwrap());
        assert!(!result.overflow.get_constant_bool().unwrap());

        // signed overflow
        let result = add_with_carry(&smax, &num16, &zero_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 2147483663);
        assert!(!result.carry_out.get_constant_bool().unwrap());
        assert!(result.overflow.get_constant_bool().unwrap());

        // signed underflow
        let result = add_with_carry(&smin, &num16.not(), &one_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 2147483632);
        assert!(result.carry_out.get_constant_bool().unwrap());
        assert!(result.overflow.get_constant_bool().unwrap());

        // zero add
        let result = add_with_carry(&num16, &zero, &zero_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 16);
        assert!(!result.carry_out.get_constant_bool().unwrap());
        assert!(!result.overflow.get_constant_bool().unwrap());

        // zero sub
        let result = add_with_carry(&num16, &zero.not(), &one_bool, 32);
        assert_eq!(result.result.get_constant().unwrap(), 16);
        assert!(result.carry_out.get_constant_bool().unwrap());
        assert!(!result.overflow.get_constant_bool().unwrap());
    }

    fn setup_test_vm() -> VM<ArmV6M> {
        // create an empty project
        let project = Box::new(Project::manual_project(
            vec![],
            0,
            0,
            WordSize::Bit32,
            Endianness::Little,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            vec![],
            HashMap::new(),
            vec![],
        ));
        let project = Box::leak(project);
        let context = Box::new(DContext::new());
        let context = Box::leak(context);
        let solver = DSolver::new(context);
        let state =
            GAState::create_test_state(project, context, solver, 0, u32::MAX as u64, ArmV6M {});
        let vm = VM::new_with_state(project, state);
        vm
    }

    #[test]
    fn test_move() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let mut local = HashMap::new();
        let operand_r0 = Operand::Register("R0".to_owned());

        // move imm into reg
        let operation = Operation::Move {
            destination: operand_r0.clone(),
            source: Operand::Immediate(DataWord::Word32(42)),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0 = executor
            .get_operand_value(&operand_r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0, 42);

        // move reg to local
        let local_r0 = Operand::Local("R0".to_owned());
        let operation = Operation::Move {
            destination: local_r0.clone(),
            source: operand_r0.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0 = executor
            .get_operand_value(&local_r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0, 42);

        // move immediate to local memory addr
        let imm = Operand::Immediate(DataWord::Word32(23));
        let memory_op = Operand::AddressInLocal("R0".to_owned(), 32);
        let operation = Operation::Move {
            destination: memory_op.clone(),
            source: imm.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let dexpr_addr = executor.get_dexpr_from_dataword(DataWord::Word32(42));
        let in_memory_value = executor
            .state
            .read_word_from_memory(&dexpr_addr)
            .unwrap()
            .get_constant()
            .unwrap();

        assert_eq!(in_memory_value, 23);

        // move from memory to a local
        let operation = Operation::Move {
            destination: local_r0.clone(),
            source: memory_op.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let local_value = executor
            .get_operand_value(&local_r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();

        assert_eq!(local_value, 23);
    }

    #[test]
    fn test_add() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let mut local = HashMap::new();

        let r0 = Operand::Register("R0".to_owned());
        let imm_42 = Operand::Immediate(DataWord::Word32(42));
        let imm_umax = Operand::Immediate(DataWord::Word32(u32::MAX));
        let imm_16 = Operand::Immediate(DataWord::Word32(16));
        let imm_minus70 = Operand::Immediate(DataWord::Word32(-70i32 as u32));

        // test simple add
        let operation = Operation::Add {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 58);

        // test add with same operand and destination
        let operation = Operation::Add {
            destination: r0.clone(),
            operand1: r0.clone(),
            operand2: imm_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 74);

        // test add with negative number
        let operation = Operation::Add {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_minus70.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, (-28i32 as u32) as u64);

        // test add overflow
        let operation = Operation::Add {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_umax.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 41);
    }

    #[test]
    fn test_adc() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let mut local = HashMap::new();

        let imm_42 = Operand::Immediate(DataWord::Word32(42));
        let imm_12 = Operand::Immediate(DataWord::Word32(12));
        let imm_umax = Operand::Immediate(DataWord::Word32(u32::MAX));
        let r0 = Operand::Register("R0".to_owned());

        let true_dexpr = executor.state.ctx.from_bool(true);
        let false_dexpr = executor.state.ctx.from_bool(false);

        // test normal add
        executor.state.set_flag("C".to_owned(), false_dexpr.clone());
        let operation = Operation::Adc {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_12.clone(),
        };

        executor.execute_operation(&operation, &mut local).ok();
        let result = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();

        assert_eq!(result, 54);

        // test add with overflow
        executor.state.set_flag("C".to_owned(), false_dexpr.clone());
        let operation = Operation::Adc {
            destination: r0.clone(),
            operand1: imm_umax.clone(),
            operand2: imm_12.clone(),
        };

        executor.execute_operation(&operation, &mut local).ok();
        let result = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();

        assert_eq!(result, 11);

        // test add with carry in
        executor.state.set_flag("C".to_owned(), true_dexpr.clone());
        let operation = Operation::Adc {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_12.clone(),
        };

        executor.execute_operation(&operation, &mut local).ok();
        let result = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();

        assert_eq!(result, 55);
    }

    #[test]
    fn test_sub() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let mut local = HashMap::new();

        let r0 = Operand::Register("R0".to_owned());
        let imm_42 = Operand::Immediate(DataWord::Word32(42));
        let imm_imin = Operand::Immediate(DataWord::Word32(i32::MIN as u32));
        let imm_16 = Operand::Immediate(DataWord::Word32(16));
        let imm_minus70 = Operand::Immediate(DataWord::Word32(-70i32 as u32));

        // test simple sub
        let operation = Operation::Sub {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 26);

        // test sub with same operand and destination
        let operation = Operation::Sub {
            destination: r0.clone(),
            operand1: r0.clone(),
            operand2: imm_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 10);

        // test sub with negative number
        let operation = Operation::Sub {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_minus70.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 112);

        // test sub underflow
        let operation = Operation::Sub {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_imin.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, ((i32::MIN) as u32 + 42) as u64);
    }

    #[test]
    fn test_mul() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let mut local = HashMap::new();

        let r0 = Operand::Register("R0".to_owned());
        let imm_42 = Operand::Immediate(DataWord::Word32(42));
        let imm_minus_42 = Operand::Immediate(DataWord::Word32(-42i32 as u32));
        let imm_16 = Operand::Immediate(DataWord::Word32(16));
        let imm_minus_16 = Operand::Immediate(DataWord::Word32(-16i32 as u32));

        // simple multiplication
        let operation = Operation::Mul {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 672);

        // multiplication right minus
        let operation = Operation::Mul {
            destination: r0.clone(),
            operand1: imm_42.clone(),
            operand2: imm_minus_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value as u32, -672i32 as u32);

        // multiplication left minus
        let operation = Operation::Mul {
            destination: r0.clone(),
            operand1: imm_minus_42.clone(),
            operand2: imm_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value as u32, -672i32 as u32);

        // multiplication both minus
        let operation = Operation::Mul {
            destination: r0.clone(),
            operand1: imm_minus_42.clone(),
            operand2: imm_minus_16.clone(),
        };
        executor.execute_operation(&operation, &mut local).ok();

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 672);
    }

    #[test]
    fn test_set_v_flag() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let mut local = HashMap::new();

        let imm_42 = Operand::Immediate(DataWord::Word32(42));
        let imm_12 = Operand::Immediate(DataWord::Word32(12));
        let imm_imin = Operand::Immediate(DataWord::Word32(i32::MIN as u32));
        let imm_imax = Operand::Immediate(DataWord::Word32(i32::MAX as u32));

        // no overflow
        let operation = Operation::SetVFlag {
            operand1: imm_42.clone(),
            operand2: imm_12.clone(),
            sub: true,
            carry: false,
        };
        executor.execute_operation(&operation, &mut local).ok();

        let v_flag = executor
            .state
            .get_flag("V".to_owned())
            .unwrap()
            .get_constant_bool()
            .unwrap();
        assert!(!v_flag);

        // overflow
        let operation = Operation::SetVFlag {
            operand1: imm_imax.clone(),
            operand2: imm_12.clone(),
            sub: false,
            carry: false,
        };
        executor.execute_operation(&operation, &mut local).ok();

        let v_flag = executor
            .state
            .get_flag("V".to_owned())
            .unwrap()
            .get_constant_bool()
            .unwrap();
        assert!(v_flag);

        // underflow
        let operation = Operation::SetVFlag {
            operand1: imm_imin.clone(),
            operand2: imm_12.clone(),
            sub: true,
            carry: false,
        };
        executor.execute_operation(&operation, &mut local).ok();

        let v_flag = executor
            .state
            .get_flag("V".to_owned())
            .unwrap()
            .get_constant_bool()
            .unwrap();
        assert!(v_flag);
    }

    #[test]
    fn test_conditional_execution() {
        let mut vm = setup_test_vm();
        let project = vm.project;
        let mut executor =
            GAExecutor::from_state(vm.paths.get_path().unwrap().state, &mut vm, project);
        let imm_0 = Operand::Immediate(DataWord::Word32(0));
        let imm_1 = Operand::Immediate(DataWord::Word32(1));
        let local = HashMap::new();
        let r0 = Operand::Register("R0".to_owned());

        let program1 = vec![
            Instruction {
                instruction_size: 32,
                operations: vec![Operation::SetZFlag(imm_0.clone())],
                max_cycle: CycleCount::Value(0),
                memory_access: false,
            },
            Instruction {
                instruction_size: 32,
                operations: vec![Operation::ConditionalExecution {
                    conditions: vec![Condition::EQ, Condition::NE],
                }],
                max_cycle: CycleCount::Value(0),
                memory_access: false,
            },
            Instruction {
                instruction_size: 32,
                operations: vec![Operation::Move {
                    destination: r0.clone(),
                    source: imm_1,
                }],
                max_cycle: CycleCount::Value(0),
                memory_access: false,
            },
            Instruction {
                instruction_size: 32,
                operations: vec![Operation::Move {
                    destination: r0.clone(),
                    source: imm_0,
                }],
                max_cycle: CycleCount::Value(0),
                memory_access: false,
            },
        ];

        for p in program1 {
            executor.execute_instruction(&p).ok();
        }

        let r0_value = executor
            .get_operand_value(&r0, &local)
            .ok()
            .unwrap()
            .get_constant()
            .unwrap();
        assert_eq!(r0_value, 1);
    }
}
