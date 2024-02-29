use crate::{
    arch::{Arch, ArchError, ParseError},
    general_assembly::{
        instruction::Instruction,
        translator::{Hookable, Translatable},
        RunConfig,
    },
};
use armv6_m_instruction_parser::Error;
use dissarmv7::prelude::*;

/// Type level denotation for the
/// [Armv6-M](https://developer.arm.com/documentation/ddi0419/latest/) ISA.
#[derive(Debug)]
pub struct ArmV6M {}

impl Arch for ArmV6M {
    fn add_hooks(&self, cfg: &mut RunConfig) {
        armv6_m_instruction_parser::instructons::Instruction::add_hooks(cfg)
    }
    fn translate(&self, buff: &[u8]) -> Result<Instruction, ArchError> {
        let b2 = buff.clone();
        let ret = armv6_m_instruction_parser::parse(buff).map_err(|e| e.into())?;
        let mut buff: dissarmv7::buffer::PeekableBuffer<u8, _> =
            b2.iter().cloned().to_owned().into();

        let instr = dissarmv7::ASM::parse_exact::<_, 1>(&mut buff);
        println!("{ret:?}, {instr:?}");
        Ok(ret.translate())
    }
}

impl Into<ArchError> for Error {
    fn into(self) -> ArchError {
        ArchError::ParsingError(match self {
            Self::InsufficientInput => ParseError::InvalidRegister,
            Self::Malfromed32BitInstruction => ParseError::MalfromedInstruction,
            Self::Invalid32BitInstruction => ParseError::InvalidInstruction,
            Self::InvalidOpCode => ParseError::InvalidInstruction,
            Self::Unpredictable => ParseError::Unpredictable,
            Self::InvalidRegister => ParseError::InvalidRegister,
            Self::InvalidCondition => ParseError::InvalidCondition,
        })
    }
}