//! Theories-of-array memory.
//!
//! This memory model uses theories-of-arrays and supports arbitrary read and
//! writes with symbolic values. It uses a linear address space which is byte
//! addressable. A single write will split the symbolic value into byte sized
//! chunks, and write each individually into memory. A read will concatenate
//! multiple bytes into a single large symbol.
//!
//! The concatenation on reads will generate more complex expressions compared
//! to other memory models, and in general this memory model is slower compared
//! to e.g. object memory. However, it may provide better performance in certain
//! situations.
use hashbrown::HashMap;
use tracing::trace;

use super::{MemoryError, BITS_IN_BYTE};
use crate::{
    smt::{DArray, DContext, DExpr, SmtMap, SolverError},
    Endianness,
    WordSize,
};

#[derive(Debug, Clone)]
pub struct ArrayMemory {
    /// Reference to the context so new symbols can be created.
    ctx: &'static DContext,

    /// Size of a pointer.
    ptr_size: u32,

    /// The actual memory. Stores all values written to memory.
    memory: DArray,

    /// Memory endianness
    endianness: Endianness,
}

impl ArrayMemory {
    #[tracing::instrument(skip(self))]
    pub fn resolve_addresses(
        &self,
        addr: &DExpr,
        _upper_bound: usize,
    ) -> Result<Vec<DExpr>, MemoryError> {
        Ok(vec![addr.clone()])
    }

    #[tracing::instrument(skip(self))]
    pub fn read(&self, addr: &DExpr, bits: u32) -> Result<DExpr, MemoryError> {
        assert_eq!(addr.len(), self.ptr_size, "passed wrong sized address");

        let value = self.internal_read(addr, bits, self.ptr_size)?;
        trace!("Read value: {value:?}");
        Ok(value)
    }

    #[tracing::instrument(skip(self))]
    pub fn write(&mut self, addr: &DExpr, value: DExpr) -> Result<(), MemoryError> {
        assert_eq!(addr.len(), self.ptr_size, "passed wrong sized address");
        self.internal_write(addr, value, self.ptr_size)
    }

    /// Creates a new memory containing only uninitialized memory.
    pub fn new(ctx: &'static DContext, ptr_size: u32, endianness: Endianness) -> Self {
        let memory = DArray::new(ctx, ptr_size as usize, BITS_IN_BYTE as usize, "memory");

        Self {
            ctx,
            ptr_size,
            memory,
            endianness,
        }
    }

    /// Reads an u8 from the given address.
    fn read_u8(&self, addr: &DExpr) -> DExpr {
        self.memory.read(addr)
    }

    /// Writes an u8 value to the given address.
    fn write_u8(&mut self, addr: &DExpr, val: DExpr) {
        self.memory.write(addr, &val);
    }

    /// Reads `bits` from `addr.
    ///
    /// If the number of bits are less than `BITS_IN_BYTE` then individual bits
    /// can be read, but if the number of bits exceed `BITS_IN_BYTE` then
    /// full bytes must be read.
    fn internal_read(&self, addr: &DExpr, bits: u32, ptr_size: u32) -> Result<DExpr, MemoryError> {
        let value = if bits < BITS_IN_BYTE {
            self.read_u8(addr).slice(bits - 1, 0)
        } else {
            // Ensure we only read full bytes now.
            assert_eq!(bits % BITS_IN_BYTE, 0, "Must read bytes, if bits >= 8");
            let num_bytes = bits / BITS_IN_BYTE;

            let mut bytes = Vec::new();

            for byte in 0..num_bytes {
                let offset = self.ctx.from_u64(byte as u64, ptr_size);
                let read_addr = addr.add(&offset);
                let value = self.read_u8(&read_addr);
                bytes.push(value);
            }

            match self.endianness {
                Endianness::Little => bytes.into_iter().reduce(|acc, v| v.concat(&acc)).unwrap(),
                Endianness::Big => bytes
                    .into_iter()
                    .rev()
                    .reduce(|acc, v| v.concat(&acc))
                    .unwrap(),
            }
        };

        Ok(value)
    }

    fn internal_write(
        &mut self,
        addr: &DExpr,
        value: DExpr,
        ptr_size: u32,
    ) -> Result<(), MemoryError> {
        // Check if we should zero extend the value (if it less than 8-bits).
        let value = if value.len() < BITS_IN_BYTE {
            value.zero_ext(BITS_IN_BYTE)
        } else {
            value
        };

        // Ensure the value we write is a multiple of `BITS_IN_BYTE`.
        assert_eq!(value.len() % BITS_IN_BYTE, 0);

        let num_bytes = value.len() / BITS_IN_BYTE;
        for n in 0..num_bytes {
            let low_bit = n * BITS_IN_BYTE;
            let high_bit = (n + 1) * BITS_IN_BYTE - 1;
            let byte = value.slice(low_bit, high_bit);

            let offset = match self.endianness {
                Endianness::Little => self.ctx.from_u64(n as u64, ptr_size),
                Endianness::Big => self.ctx.from_u64((num_bytes - 1 - n) as u64, ptr_size),
            };
            let addr = addr.add(&offset);
            self.write_u8(&addr, byte);
        }

        Ok(())
    }
}

pub struct BoolectorMemory {
    ram: ArrayMemory,
    register_file: HashMap<String, DExpr>,
    flags: HashMap<String, DExpr>,
    variables: HashMap<String, DExpr>,
    // TODO: Expose these.
    locals: HashMap<String, DExpr>,
    pc: u64,
}

impl SmtMap for BoolectorMemory {
    type Idx = DExpr;
    type ReturnValue = DExpr;

    fn get(
        &self,
        idx: &Self::Idx,
        size: usize,
    ) -> Result<Self::ReturnValue, crate::smt::MemoryError> {
        Ok(self.ram.read(idx, size as u32)?)
    }

    fn set(
        &mut self,
        idx: &Self::Idx,
        value: Self::ReturnValue,
    ) -> Result<(), crate::smt::MemoryError> {
        Ok(self.ram.write(idx, value)?)
    }

    fn get_pc(&self) -> Result<Self::ReturnValue, crate::smt::MemoryError> {
        Ok(self.ram.ctx.from_u64(self.pc, 32))
    }

    fn set_pc(&mut self, value: u32) -> Result<(), crate::smt::MemoryError> {
        self.pc = value as u64;
        Ok(())
    }

    fn set_flag(
        &mut self,
        idx: &str,
        value: Self::ReturnValue,
    ) -> Result<(), crate::smt::MemoryError> {
        self.flags.insert(idx.to_string(), value);
        Ok(())
    }

    fn get_flag(&self, idx: &str) -> Result<Self::ReturnValue, crate::smt::MemoryError> {
        Ok(self
            .flags
            .get(idx)
            .unwrap_or(&self.ram.ctx.from_u64(0, 1))
            .clone())
    }

    fn set_register(
        &mut self,
        idx: &str,
        value: Self::ReturnValue,
    ) -> Result<(), crate::smt::MemoryError> {
        self.register_file.insert(idx.to_string(), value);
        Ok(())
    }

    fn get_register(&self, idx: &str) -> Result<Self::ReturnValue, crate::smt::MemoryError> {
        // TODO: Set this to
        Ok(self
            .register_file
            .get(idx)
            .unwrap_or(&self.ram.ctx.from_u64(0, 1))
            .clone())
    }
}

impl From<MemoryError> for crate::smt::MemoryError {
    fn from(value: MemoryError) -> Self {
        Self::MemoryFileError(value)
    }
}
#[cfg(test)]
mod test {
    use super::ArrayMemory;
    use crate::{smt::DContext, Endianness};

    fn setup_test_memory(endianness: Endianness) -> ArrayMemory {
        let ctx = Box::new(DContext::new());
        let ctx = Box::leak(ctx);
        ArrayMemory::new(ctx, 32, endianness)
    }

    #[test]
    fn test_little_endian_write() {
        let mut memory = setup_test_memory(Endianness::Little);
        let indata = memory.ctx.from_u64(0x01020304, 32);
        let addr = memory.ctx.from_u64(0, 32);
        let one = memory.ctx.from_u64(1, 32);
        memory.write(&addr, indata).ok();
        let b1 = memory.read_u8(&addr);
        let addr = addr.add(&one);
        let b2 = memory.read_u8(&addr);
        let addr = addr.add(&one);
        let b3 = memory.read_u8(&addr);
        let addr = addr.add(&one);
        let b4 = memory.read_u8(&addr);

        assert_eq!(b1.get_constant().unwrap(), 0x04);
        assert_eq!(b2.get_constant().unwrap(), 0x03);
        assert_eq!(b3.get_constant().unwrap(), 0x02);
        assert_eq!(b4.get_constant().unwrap(), 0x01);
    }

    #[test]
    fn test_big_endian_write() {
        let mut memory = setup_test_memory(Endianness::Big);
        let indata = memory.ctx.from_u64(0x01020304, 32);
        let addr = memory.ctx.from_u64(0, 32);
        let one = memory.ctx.from_u64(1, 32);
        memory.write(&addr, indata).ok();
        let b1 = memory.read_u8(&addr);
        let addr = addr.add(&one);
        let b2 = memory.read_u8(&addr);
        let addr = addr.add(&one);
        let b3 = memory.read_u8(&addr);
        let addr = addr.add(&one);
        let b4 = memory.read_u8(&addr);

        assert_eq!(b1.get_constant().unwrap(), 0x01);
        assert_eq!(b2.get_constant().unwrap(), 0x02);
        assert_eq!(b3.get_constant().unwrap(), 0x03);
        assert_eq!(b4.get_constant().unwrap(), 0x04);
    }

    #[test]
    fn test_little_endian_read() {
        let mut memory = setup_test_memory(Endianness::Little);
        let b1 = memory.ctx.from_u64(0x04, 8);
        let b2 = memory.ctx.from_u64(0x03, 8);
        let b3 = memory.ctx.from_u64(0x02, 8);
        let b4 = memory.ctx.from_u64(0x01, 8);

        let one = memory.ctx.from_u64(1, 32);
        let addr = memory.ctx.from_u64(0, 32);
        memory.write_u8(&addr, b1);
        let addr = addr.add(&one);
        memory.write_u8(&addr, b2);
        let addr = addr.add(&one);
        memory.write_u8(&addr, b3);
        let addr = addr.add(&one);
        memory.write_u8(&addr, b4);

        let addr = memory.ctx.from_u64(0, 32);
        let result = memory.read(&addr, 32).ok().unwrap();
        assert_eq!(result.get_constant().unwrap(), 0x01020304);
    }

    #[test]
    fn test_big_endian_read() {
        let mut memory = setup_test_memory(Endianness::Big);
        let b1 = memory.ctx.from_u64(0x01, 8);
        let b2 = memory.ctx.from_u64(0x02, 8);
        let b3 = memory.ctx.from_u64(0x03, 8);
        let b4 = memory.ctx.from_u64(0x04, 8);

        let one = memory.ctx.from_u64(1, 32);
        let addr = memory.ctx.from_u64(0, 32);
        memory.write_u8(&addr, b1);
        let addr = addr.add(&one);
        memory.write_u8(&addr, b2);
        let addr = addr.add(&one);
        memory.write_u8(&addr, b3);
        let addr = addr.add(&one);
        memory.write_u8(&addr, b4);

        let addr = memory.ctx.from_u64(0, 32);
        let result = memory.read(&addr, 32).ok().unwrap();
        assert_eq!(result.get_constant().unwrap(), 0x01020304);
    }
}
