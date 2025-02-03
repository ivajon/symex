//! Defines all types of operands that are valid in [Symex](../../../) General
//! Assembly.

#[derive(Debug, Clone, Copy)]
/// [Symex](../../../) representation for immediate fields.
#[allow(missing_docs)]
pub enum DataWord {
    Word64(u64),
    Word32(u32),
    Word16(u16),
    Word8(u8),
}

#[derive(Debug, Clone, Copy)]
/// Symex representation of a halfword immediate field.
///
/// These immediate fields occupy the least significant half of
/// a machine word.
#[allow(missing_docs)]
pub enum DataHalfWord {
    HalfWord64(u32),
    HalfWord32(u16),
    HalfWord16(u8),
}

impl From<DataHalfWord> for DataWord {
    fn from(value: DataHalfWord) -> DataWord {
        match value {
            DataHalfWord::HalfWord64(v) => DataWord::Word64(v as u64),
            DataHalfWord::HalfWord32(v) => DataWord::Word32(v as u32),
            DataHalfWord::HalfWord16(v) => DataWord::Word16(v as u16),
        }
    }
}

/// Enumerates the valid operands.
#[derive(Debug, Clone)]
pub enum Operand {
    /// Representing a value in a register.
    Register(String),

    /// Representing an immediate value.
    Immediate(DataWord),

    /// Representing the value stored in memory
    /// at the address stored in a local.
    ///
    /// Reads the specified number of bits from memory.
    AddressInLocal(String, u32),

    /// Representing the value stored in memory
    /// at the constant address.
    Address(DataWord, u32),

    /// Representing the value stored in memory
    /// at the address stored in a register offset
    /// by a constant value.
    #[allow(missing_docs)]
    AddressWithOffset {
        address: DataWord,
        offset_reg: String,
        width: u32,
    },

    /// Represent the value that is local to the instruction.
    Local(String),

    /// Represents a flag in the core.
    Flag(String),
}

impl From<u64> for DataWord {
    fn from(value: u64) -> Self {
        Self::Word64(value)
    }
}

impl From<u32> for DataWord {
    fn from(value: u32) -> Self {
        Self::Word32(value)
    }
}

impl From<u16> for DataWord {
    fn from(value: u16) -> Self {
        Self::Word16(value)
    }
}

impl From<u8> for DataWord {
    fn from(value: u8) -> Self {
        Self::Word8(value)
    }
}

impl From<DataWord> for u64 {
    fn from(value: DataWord) -> u64 {
        match value {
            DataWord::Word64(v) => v,
            DataWord::Word32(v) => v as u64,
            DataWord::Word16(v) => v as u64,
            DataWord::Word8(v) => v as u64,
        }
    }
}

#[derive(Debug, Clone, Copy)]
/// Represents a [`DataWord`] as a vector of u8s.
pub enum RawDataWord {
    /// A 64 bit word.
    Word64([u8; 8]),
    /// A 32 bit word.
    Word32([u8; 4]),
    /// A 16 bit word.
    Word16([u8; 2]),
    /// A 8 bit word.
    Word8([u8; 1]),
}
