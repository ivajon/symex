//! Defines armv6 hooks, instruction tranlsation and timings.

pub mod decoder;
pub mod timing;

use std::fmt::Display;

use armv6_m_instruction_parser::Error;
use object::{File, Object};
use regex::Regex;
use tracing::trace;

use super::{arm_isa, ArmIsa};
use crate::{
    elf_util::{ExpressionType, Variable},
    general_assembly::{
        arch::{Arch, ArchError, ParseError},
        instruction::Instruction,
        project::{MemoryHookAddress, MemoryReadHook, PCHook, RegisterReadHook, RegisterWriteHook},
        state::GAState,
        RunConfig,
    },
};

/// Type level denotation for the
/// [Armv6-M](https://developer.arm.com/documentation/ddi0419/latest/) ISA.
#[derive(Clone, Copy, Debug)]
pub struct ArmV6M {}

impl Arch for ArmV6M {
    fn add_hooks(&self, cfg: &mut RunConfig<Self>) {
        let symbolic_sized = |state: &mut GAState<Self>| {
            let value_ptr = state.get_register("R0".to_owned())?;
            let size = state.get_register("R1".to_owned())?.get_constant().unwrap() * 8;
            trace!(
                "trying to create symbolic: addr: {:?}, size: {}",
                value_ptr,
                size
            );
            let name = "any".to_owned() + &state.marked_symbolic.len().to_string();
            let symb_value = state.ctx.unconstrained(size as u32, &name);
            state.marked_symbolic.push(Variable {
                name: Some(name),
                value: symb_value.clone(),
                ty: ExpressionType::Integer(size as usize),
                alias: None,
            });
            state.memory.write(&value_ptr, symb_value)?;

            let lr = state.get_register("LR".to_owned())?;
            state.set_register("PC".to_owned(), lr, None)?;
            Ok(())
        };

        cfg.pc_hooks.push((
            Regex::new(r"^symbolic_size<.+>$").unwrap(),
            PCHook::Intrinsic(symbolic_sized),
        ));

        let read_pc: RegisterReadHook<Self> = |state| {
            let two = state.ctx.from_u64(1, 32);
            let pc = state.get_register("PC".to_owned()).unwrap();
            Ok(pc.add(&two))
        };

        let write_pc: RegisterWriteHook<Self> =
            |state, value| state.set_register("PC".to_owned(), value, None);

        cfg.register_read_hooks.push(("PC+".to_owned(), read_pc));
        cfg.register_write_hooks.push(("PC+".to_owned(), write_pc));

        // reset allways done
        let read_reset_done: MemoryReadHook<Self> = |state, _addr| {
            let value = state.ctx.from_u64(0xffff_ffff, 32);
            Ok(value)
        };
        cfg.memory_read_hooks
            .push((MemoryHookAddress::Single(0x4000c008), read_reset_done));
    }

    fn translate(
        &self,
        buff: &[u8],
        _state: &GAState<Self>,
    ) -> Result<Instruction<Self>, ArchError> {
        let ret = armv6_m_instruction_parser::parse(buff).map_err(map_err)?;
        let to_exec = Self::expand(ret);
        Ok(to_exec)
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
            ArmIsa::ArmV6M => Ok(Some(ArmV6M {})),
            ArmIsa::ArmV7EM => Ok(None),
        }
    }

    fn register_to_number(reg: &String) -> Option<u8> {
        Some(match reg.as_str() {
            "R0" => 0,
            "R1" => 1,
            "R2" => 2,
            "R3" => 3,
            "R4" => 4,
            "R5" => 5,
            "R6" => 6,
            "R7" => 7,
            "R8" => 8,
            "R9" => 9,
            "R10" => 10,
            "R11" => 11,
            "R12" => 12,
            "SP" => 13,
            "SP&" => 13,
            "LR" => 14,
            "PC" => 15,
            "PC+" => 15,
            _ => return None,
        })
    }

    fn number_to_register(reg: u8) -> Option<String> {
        Some(match reg {
            0 => "R0".to_owned(),
            1 => "R1".to_owned(),
            2 => "R2".to_owned(),
            3 => "R3".to_owned(),
            4 => "R4".to_owned(),
            5 => "R5".to_owned(),
            6 => "R6".to_owned(),
            7 => "R7".to_owned(),
            8 => "R8".to_owned(),
            9 => "R9".to_owned(),
            10 => "R10".to_owned(),
            11 => "R11".to_owned(),
            12 => "R12".to_owned(),
            13 => "SP&".to_owned(),
            14 => "LR".to_owned(),
            15 => "PC+".to_owned(),
            _ => return None,
        })
    }
}

impl Display for ArmV6M {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ARMv6-M")
    }
}

fn map_err(err: Error) -> ArchError {
    ArchError::ParsingError(match err {
        Error::InsufficientInput => ParseError::InvalidRegister,
        Error::Malfromed32BitInstruction => ParseError::MalfromedInstruction,
        Error::Invalid32BitInstruction => ParseError::InvalidInstruction,
        Error::InvalidOpCode => ParseError::InvalidInstruction,
        Error::Unpredictable => ParseError::Unpredictable,
        Error::InvalidRegister => ParseError::InvalidRegister,
        Error::InvalidCondition => ParseError::InvalidCondition,
    })
}
