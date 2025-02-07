//! Defines armv6 hooks, instruction translation and timings.

use std::fmt::Display;

use armv6_m_instruction_parser::Error;
use tracing::trace;

use crate::{
    arch::{ArchError, Architecture, ParseError},
    executor::{hooks::PCHook2, state::GAState2},
    smt::{SmtExpr, SmtMap},
};

pub mod decoder;
pub mod timing;

/// Type level denotation for the
/// [Armv6-M](https://developer.arm.com/documentation/ddi0419/latest/) ISA.
#[derive(Clone, Copy, Debug)]
pub struct ArmV6M {}

impl Architecture for ArmV6M {
    fn add_hooks<
        ArchitechtureImplementation: AsMut<Self> + ?Sized,
        C: crate::Composition<Architecture = ArchitechtureImplementation>,
    >(
        &self,
        cfg: &mut crate::executor::hooks::HookContainer<C>,
        sub_program_lookup: &mut crate::project::dwarf_helper::SubProgramMap,
    ) where
        Self: Sized,
    {
        let symbolic_sized = |state: &mut GAState2<C>| {
            let value_ptr = state.get_register("R0".to_owned())?;
            let size = state.get_register("R1".to_owned())?.get_constant().unwrap() * 8;
            trace!(
                "trying to create symbolic: addr: {:?}, size: {}",
                value_ptr,
                size
            );
            let name = state.label_new_symbolic("any");
            let symb_value = state.memory.unconstrained(&name, size as usize);
            //state.marked_symbolic.push(Variable {
            //    name: Some(name),
            //    value: symb_value.clone(),
            //    ty: ExpressionType::Integer(size as usize),
            //});
            state.memory.set(&value_ptr, symb_value)?;

            let lr = state.get_register("LR".to_owned())?;
            state.set_register("PC".to_owned(), lr)?;
            Ok(())
        };

        cfg.add_pc_hook_regex(
            &sub_program_lookup,
            r"^symbolic_size<.+>$",
            PCHook2::Intrinsic(symbolic_sized),
        );

        let read_pc = |state: &mut GAState2<C>| {
            let two = state.memory.from_u64(1, 32);
            let pc = state.get_register("PC".to_owned()).unwrap();
            Ok(pc.add(&two))
        };

        let write_pc = |state: &mut GAState2<C>, value: C::SmtExpression| {
            state.set_register("PC".to_owned(), value)
        };

        cfg.add_register_read_hook("PC+".to_owned(), read_pc);
        cfg.add_register_write_hook("PC+".to_owned(), write_pc);

        // reset always done
        let read_reset_done = |state: &mut GAState2<C>, _addr| {
            let value = state.memory.from_u64(0xffff_ffff, 32);
            Ok(value)
        };
        cfg.add_memory_read_hook(0x4000c008, read_reset_done);
    }

    fn translate<
        ArchitechtureImplementation: AsMut<Self> + ?Sized,
        C: crate::Composition<Architecture = ArchitechtureImplementation>,
    >(
        &self,
        buff: &[u8],
        _state: &GAState2<C>,
    ) -> Result<crate::executor::instruction::Instruction2<C>, ArchError>
    where
        Self: Sized,
    {
        let ret = armv6_m_instruction_parser::parse(buff).map_err(map_err)?;
        let to_exec = Self::expand(ret);
        Ok(to_exec)
    }

    //fn discover(file: &File<'_>) -> Result<Option<Self>, ArchError> {
    //    let f = match file {
    //        File::Elf32(f) => Ok(f),
    //        _ => Err(ArchError::IncorrectFileType),
    //    }?;
    //    let section = match f.section_by_name(".ARM.attributes") {
    //        Some(section) => Ok(section),
    //        None => Err(ArchError::MissingSection(".ARM.attributes")),
    //    }?;
    //    let isa = arm_isa(&section)?;
    //    match isa {
    //        ArmIsa::ArmV6M => Ok(Some(ArmV6M {})),
    //        ArmIsa::ArmV7EM => Ok(None),
    //    }
    //}

    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
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
