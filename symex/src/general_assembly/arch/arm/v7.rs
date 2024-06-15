use std::fmt::Display;

use decoder::Convert;
use disarmv7::prelude::{Operation as V7Operation, *};
use general_assembly::operation::Operation;
use object::{File, Object};
use regex::Regex;

use crate::{
    elf_util::{ExpressionType, Variable},
    general_assembly::{
        arch::{Arch, ArchError, ParseError},
        instruction::Instruction,
        project::{MemoryHookAddress, MemoryReadHook, PCHook, RegisterReadHook, RegisterWriteHook},
        run_config::RunConfig,
        state::GAState,
    },
};

use super::{arm_isa, ArmIsa};

#[rustfmt::skip]
pub mod decoder;
pub mod compare;
#[cfg(test)]
pub mod test;
pub mod timing;

/// Type level denotation for the Armv7-EM ISA.
#[derive(Debug, Default, Clone)]
pub struct ArmV7EM {}

impl Arch for ArmV7EM {
    fn add_hooks(&self, cfg: &mut RunConfig) {
        let symbolic_sized = |state: &mut GAState<Self>| {
            let value_ptr = state.get_register("R0".to_owned())?;
            let size = state.get_register("R1".to_owned())?.get_constant().unwrap() * 8;
            let name = "any".to_owned() + &state.marked_symbolic.len().to_string();
            let symb_value = state.ctx.unconstrained(size as u32, &name);
            state.marked_symbolic.push(Variable {
                name: Some(name),
                value: symb_value.clone(),
                ty: ExpressionType::Integer(size as usize),
            });
            state.memory.write(&value_ptr, symb_value)?;

            let lr = state.get_register("LR".to_owned())?;
            state.set_register("PC".to_owned(), lr)?;
            Ok(())
        };

        cfg.pc_hooks.push((
            Regex::new(r"^symbolic_size<.+>$").unwrap(),
            PCHook::Intrinsic(symbolic_sized),
        ));
        // §B1.4 Specifies that R[15] => Addr(Current instruction) + 4
        //
        // This can be translated in to
        //
        // PC - Size(prev instruction) / 8 + 4
        // as PC points to the next instruction, we
        //
        //
        // Or we can simply take the previous PC + 4.
        let read_pc: RegisterReadHook<Self> = |state| {
            let new_pc = state
                .ctx
                .from_u64(state.last_pc + 4, state.project.get_word_size())
                .simplify();
            Ok(new_pc)
        };

        let read_sp: RegisterReadHook<Self> = |state| {
            let two = state.ctx.from_u64((!(0b11u32)) as u64, 32);
            let sp = state.get_register("SP".to_owned()).unwrap();
            let sp = sp.simplify();
            Ok(sp.and(&two))
        };

        let write_pc: RegisterWriteHook<Self> =
            |state, value| state.set_register("PC".to_owned(), value);
        let write_sp: RegisterWriteHook<Self> = |state, value| {
            state.set_register(
                "SP".to_owned(),
                value.and(&state.ctx.from_u64((!(0b11u32)) as u64, 32)),
            )?;
            let sp = state.get_register("SP".to_owned()).unwrap();
            let sp = sp.simplify();
            state.set_register("SP".to_owned(), sp)
        };

        cfg.register_read_hooks.push(("PC+".to_owned(), read_pc));
        cfg.register_write_hooks.push(("PC+".to_owned(), write_pc));
        cfg.register_read_hooks.push(("SP&".to_owned(), read_sp));
        cfg.register_write_hooks.push(("SP&".to_owned(), write_sp));

        // reset allways done
        let read_reset_done: MemoryReadHook<Self> = |state, _addr| {
            let value = state.ctx.from_u64(0xffff_ffff, 32);
            Ok(value)
        };
        cfg.memory_read_hooks
            .push((MemoryHookAddress::Single(0x4000c008), read_reset_done));
    }

    fn translate(&self, buff: &[u8], state: &GAState<Self>) -> Result<Instruction, ArchError> {
        let mut buff: disarmv7::buffer::PeekableBuffer<u8, _> = buff.iter().cloned().into();

        let instr = V7Operation::parse(&mut buff).map_err(|e| ArchError::ParsingError(e.into()))?;
        let timing = Self::cycle_count_m4_core(&instr.1);
        let ops: Vec<Operation> = instr.clone().convert(state.get_in_conditional_block());

        Ok(Instruction {
            instruction_size: instr.0 as u32,
            operations: ops,
            max_cycle: timing,
            memory_access: Self::memory_access(&instr.1),
        })
    }

    fn discover(file: &File) -> Result<Option<Self>, ArchError> {
        let f = match file {
            File::Elf32(f) => Ok(f),
            _ => Err(ArchError::IncorrectFileType),
        }?;
        let section = match f.section_by_name(".ARM.attributes") {
            Some(section) => Ok(section),
            None => Err(ArchError::MissingSection(".ARM.attributes")),
        }?;
        let isa = arm_isa(&section)?;
        match isa {
            ArmIsa::ArmV6M => Ok(None),
            ArmIsa::ArmV7EM => Ok(Some(ArmV7EM {})),
        }
    }
}

impl Display for ArmV7EM {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ARMv7-M")
    }
}

impl From<disarmv7::ParseError> for ParseError {
    fn from(value: disarmv7::ParseError) -> Self {
        match value {
            disarmv7::ParseError::Undefined => ParseError::InvalidInstruction,
            disarmv7::ParseError::ArchError(aerr) => match aerr {
                disarmv7::prelude::arch::ArchError::InvalidCondition => {
                    ParseError::InvalidCondition
                }
                disarmv7::prelude::arch::ArchError::InvalidRegister(_) => {
                    ParseError::InvalidRegister
                }
                disarmv7::prelude::arch::ArchError::InvalidField(_) => {
                    ParseError::MalfromedInstruction
                }
            },
            disarmv7::ParseError::Unpredictable => ParseError::Unpredictable,
            disarmv7::ParseError::Invalid16Bit(_) | disarmv7::ParseError::Invalid32Bit(_) => {
                ParseError::InvalidInstruction
            }
            disarmv7::ParseError::InvalidField(_) => ParseError::MalfromedInstruction,
            disarmv7::ParseError::Incomplete32Bit => ParseError::InsufficientInput,
            disarmv7::ParseError::InternalError(info) => ParseError::Generic(info),
            disarmv7::ParseError::IncompleteParser => {
                ParseError::Generic("Encountered instruction that is not yet supported.")
            }
            disarmv7::ParseError::InvalidCondition => ParseError::InvalidCondition,
            disarmv7::ParseError::IncompleteProgram => ParseError::InsufficientInput,
            disarmv7::ParseError::InvalidRegister(_) => ParseError::InvalidRegister,
            disarmv7::ParseError::PartiallyParsed(error, _) => (*error).into(),
        }
    }
}
