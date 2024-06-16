use std::{collections::HashMap, fmt::Debug};

use general_assembly::operand::{DataHalfWord, DataWord, RawDataWord};
use gimli::{DebugAbbrev, DebugInfo, DebugStr};
use object::{File, Object, ObjectSection, ObjectSymbol};
use tracing::{debug, trace};

use self::segments::Segments;
use super::{
    arch::ArchError,
    instruction::Instruction,
    state::GAState,
    Endianness,
    Result as SuperResult,
    RunConfig,
    WordSize,
};
use crate::{general_assembly::arch::Arch, memory::MemoryError, smt::DExpr};

mod dwarf_helper;
use dwarf_helper::*;

pub mod segments;

pub type Result<T> = std::result::Result<T, ProjectError>;

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProjectError {
    #[error("Unable to parse elf file: {0}")]
    UnableToParseElf(String),

    #[error("Program memory error")]
    ProgrammemoryError(#[from] MemoryError),

    #[error("Unavalable operation")]
    UnabvalableOperation,

    #[error("Architecture specific error")]
    ArchError(#[from] ArchError),
}

#[derive(Debug, Clone, Copy)]
pub enum PCHook<A: Arch> {
    Continue,
    EndSuccess,
    EndFailure(&'static str),
    Intrinsic(fn(state: &mut GAState<A>) -> SuperResult<()>),
    Suppress,
}

pub type PCHooks<A> = HashMap<u64, PCHook<A>>;

/// Hook for a register read.
pub type RegisterReadHook<A> = fn(state: &mut GAState<A>) -> SuperResult<DExpr>;
pub type RegisterReadHooks<A> = HashMap<String, RegisterReadHook<A>>;

/// Hook for a register write.
pub type RegisterWriteHook<A> = fn(state: &mut GAState<A>, value: DExpr) -> SuperResult<()>;
pub type RegisterWriteHooks<A> = HashMap<String, RegisterWriteHook<A>>;

#[derive(Debug, Clone)]
pub enum MemoryHookAddress {
    Single(u64),
    Range(u64, u64),
}

/// Hook for a memory write.
pub type MemoryWriteHook<A> =
    fn(state: &mut GAState<A>, address: u64, value: DExpr, bits: u32) -> SuperResult<()>;
pub type SingleMemoryWriteHooks<A> = HashMap<u64, MemoryWriteHook<A>>;
pub type RangeMemoryWriteHooks<A> = Vec<((u64, u64), MemoryWriteHook<A>)>;

/// Hook for a memory read.
pub type MemoryReadHook<A> = fn(state: &mut GAState<A>, address: u64) -> SuperResult<DExpr>;
pub type SingleMemoryReadHooks<A> = HashMap<u64, MemoryReadHook<A>>;
pub type RangeMemoryReadHooks<A> = Vec<((u64, u64), MemoryReadHook<A>)>;

/// Holds all data read from the ELF file.
// Add all read only memory here later to handle global constants.
pub struct Project<A: Arch + Clone + 'static> {
    segments: Segments,
    word_size: WordSize,
    endianness: Endianness,
    symtab: HashMap<String, u64>,
    pc_hooks: PCHooks<A>,
    reg_read_hooks: RegisterReadHooks<A>,
    reg_write_hooks: RegisterWriteHooks<A>,
    single_memory_read_hooks: SingleMemoryReadHooks<A>,
    range_memory_read_hooks: RangeMemoryReadHooks<A>,
    single_memory_write_hooks: SingleMemoryWriteHooks<A>,
    range_memory_write_hooks: RangeMemoryWriteHooks<A>,
}

fn construct_register_read_hooks<A: Arch + Clone + 'static>(
    hooks: Vec<(String, RegisterReadHook<A>)>,
) -> RegisterReadHooks<A> {
    let mut ret = HashMap::new();
    for (register, hook) in hooks {
        ret.insert(register, hook);
    }
    ret
}

fn construct_register_write_hooks<A: Arch + Clone + 'static>(
    hooks: Vec<(String, RegisterWriteHook<A>)>,
) -> RegisterWriteHooks<A> {
    let mut ret = HashMap::new();

    for (register, hook) in hooks {
        ret.insert(register, hook);
    }

    ret
}

fn construct_memory_write<A: Arch + Clone + 'static>(
    hooks: Vec<(MemoryHookAddress, MemoryWriteHook<A>)>,
) -> (SingleMemoryWriteHooks<A>, RangeMemoryWriteHooks<A>) {
    let mut single_hooks = HashMap::new();
    let mut range_hooks = vec![];

    for (address, hook) in hooks {
        match address {
            MemoryHookAddress::Single(addr) => {
                single_hooks.insert(addr, hook);
            }
            MemoryHookAddress::Range(start, end) => {
                range_hooks.push(((start, end), hook));
            }
        }
    }

    (single_hooks, range_hooks)
}

fn construct_memory_read_hooks<A: Arch + Clone + 'static>(
    hooks: Vec<(MemoryHookAddress, MemoryReadHook<A>)>,
) -> (SingleMemoryReadHooks<A>, RangeMemoryReadHooks<A>) {
    let mut single_hooks = HashMap::new();
    let mut range_hooks = vec![];

    for (address, hook) in hooks {
        match address {
            MemoryHookAddress::Single(addr) => {
                single_hooks.insert(addr, hook);
            }
            MemoryHookAddress::Range(start, end) => {
                range_hooks.push(((start, end), hook));
            }
        }
    }

    (single_hooks, range_hooks)
}

impl<A: Arch + Clone + 'static> Project<A> {
    pub fn manual_project(
        program_memory: Vec<u8>,
        start_addr: u64,
        end_addr: u64,
        word_size: WordSize,
        endianness: Endianness,
        symtab: HashMap<String, u64>,
        pc_hooks: PCHooks<A>,
        reg_read_hooks: RegisterReadHooks<A>,
        reg_write_hooks: RegisterWriteHooks<A>,
        single_memory_read_hooks: SingleMemoryReadHooks<A>,
        range_memory_read_hooks: RangeMemoryReadHooks<A>,
        single_memory_write_hooks: SingleMemoryWriteHooks<A>,
        range_memory_write_hooks: RangeMemoryWriteHooks<A>,
    ) -> Project<A> {
        Project {
            segments: Segments::from_single_segment(program_memory, start_addr, end_addr),
            word_size,
            endianness,
            symtab,
            pc_hooks,
            reg_read_hooks,
            reg_write_hooks,
            single_memory_read_hooks,
            range_memory_read_hooks,
            single_memory_write_hooks,
            range_memory_write_hooks,
        }
    }

    #[cfg(test)]
    pub fn add_hooks(&mut self, arch: &A) {
        let mut cfg = RunConfig {
            memory_read_hooks: Vec::new(),
            memory_write_hooks: Vec::new(),
            pc_hooks: Vec::new(),
            register_read_hooks: Vec::new(),
            register_write_hooks: Vec::new(),
            show_path_results: false,
        };
        arch.add_hooks(&mut cfg);

        let reg_read_hooks = construct_register_read_hooks(cfg.register_read_hooks);
        let reg_write_hooks = construct_register_write_hooks(cfg.register_write_hooks);

        let (single_memory_write_hooks, range_memory_write_hooks) =
            construct_memory_write(cfg.memory_write_hooks);
        let (single_memory_read_hooks, range_memory_read_hooks) =
            construct_memory_read_hooks(cfg.memory_read_hooks);

        self.reg_read_hooks = reg_read_hooks;
        self.reg_write_hooks = reg_write_hooks;
        self.single_memory_read_hooks = single_memory_read_hooks;
        self.range_memory_read_hooks = range_memory_read_hooks;
        self.single_memory_write_hooks = single_memory_write_hooks;
        self.range_memory_write_hooks = range_memory_write_hooks;
    }

    pub fn from_path(cfg: &mut RunConfig<A>, obj_file: File, architecture: &A) -> Result<Self> {
        let segments = Segments::from_file(&obj_file);
        let endianness = if obj_file.is_little_endian() {
            Endianness::Little
        } else {
            Endianness::Big
        };

        // Do not catch 16 or 8 bit architectures but will do for now.
        let word_size = if obj_file.is_64() {
            WordSize::Bit64
        } else {
            WordSize::Bit32
        };

        let mut symtab = HashMap::new();
        for symbol in obj_file.symbols() {
            symtab.insert(
                match symbol.name() {
                    Ok(name) => name.to_owned(),
                    Err(_) => continue, // ignore entry if name can not be read
                },
                symbol.address(),
            );
        }

        let gimli_endian = match endianness {
            Endianness::Little => gimli::RunTimeEndian::Little,
            Endianness::Big => gimli::RunTimeEndian::Big,
        };

        let debug_info = obj_file.section_by_name(".debug_info").unwrap();
        let debug_info = DebugInfo::new(debug_info.data().unwrap(), gimli_endian);

        let debug_abbrev = obj_file.section_by_name(".debug_abbrev").unwrap();
        let debug_abbrev = DebugAbbrev::new(debug_abbrev.data().unwrap(), gimli_endian);

        let debug_str = obj_file.section_by_name(".debug_str").unwrap();
        let debug_str = DebugStr::new(debug_str.data().unwrap(), gimli_endian);

        trace!("Running for Architecture {}", architecture);
        architecture.add_hooks(cfg);
        let pc_hooks = &cfg.pc_hooks;

        let pc_hooks =
            construct_pc_hooks_no_index(pc_hooks, &debug_info, &debug_abbrev, &debug_str);

        debug!("Created pc hooks: {:?}", pc_hooks);

        let reg_read_hooks = construct_register_read_hooks(cfg.register_read_hooks.clone());
        let reg_write_hooks = construct_register_write_hooks(cfg.register_write_hooks.clone());

        let (single_memory_write_hooks, range_memory_write_hooks) =
            construct_memory_write(cfg.memory_write_hooks.clone());
        let (single_memory_read_hooks, range_memory_read_hooks) =
            construct_memory_read_hooks(cfg.memory_read_hooks.clone());

        Ok(Project {
            segments,
            word_size,
            endianness,
            symtab,
            pc_hooks,
            reg_read_hooks,
            reg_write_hooks,
            single_memory_read_hooks,
            range_memory_read_hooks,
            single_memory_write_hooks,
            range_memory_write_hooks,
        })
    }

    pub fn get_pc_hook(&self, pc: u64) -> Option<&PCHook<A>> {
        self.pc_hooks.get(&pc)
    }

    pub fn add_pc_hook(&mut self, pc: u64, hook: PCHook<A>) {
        self.pc_hooks.insert(pc, hook);
    }

    pub fn get_register_read_hook(&self, register: &str) -> Option<RegisterReadHook<A>> {
        self.reg_read_hooks.get(register).copied()
    }

    pub fn get_register_write_hook(&self, register: &str) -> Option<RegisterWriteHook<A>> {
        self.reg_write_hooks.get(register).copied()
    }

    pub fn get_memory_write_hook(&self, address: u64) -> Option<MemoryWriteHook<A>> {
        match self.single_memory_write_hooks.get(&address) {
            Some(hook) => Some(*hook),
            None => {
                for ((start, end), hook) in &self.range_memory_write_hooks {
                    if address >= *start && address < *end {
                        return Some(hook.to_owned());
                    }
                }
                None
            }
        }
    }

    pub fn get_memory_read_hook(&self, address: u64) -> Option<MemoryReadHook<A>> {
        match self.single_memory_read_hooks.get(&address) {
            Some(hook) => Some(*hook),
            None => {
                for ((start, end), hook) in &self.range_memory_read_hooks {
                    if address >= *start && address < *end {
                        return Some(hook.to_owned());
                    }
                }
                None
            }
        }
    }

    pub fn address_in_range(&self, address: u64) -> bool {
        self.segments.read_raw_bytes(address, 1).is_some()
    }

    pub fn get_word_size(&self) -> u32 {
        self.get_ptr_size() // same for now
    }

    pub fn get_endianness(&self) -> Endianness {
        self.endianness.clone()
    }

    pub fn get_ptr_size(&self) -> u32 {
        // This is an oversimplification and not true for some architectures
        // But will do and should map to the addresses in the elf
        match self.word_size {
            WordSize::Bit64 => 64,
            WordSize::Bit32 => 32,
            WordSize::Bit16 => 16,
            WordSize::Bit8 => 8,
        }
    }

    /// Get the address of a symbol from the ELF symbol table
    pub fn get_symbol_address(&self, symbol: &str) -> Option<u64> {
        self.symtab.get(symbol).copied()
    }

    /// Get the instruction att a address
    pub fn get_instruction(&self, address: u64, state: &GAState<A>) -> Result<Instruction<A>> {
        trace!("Reading instruction from address: {:#010X}", address);
        match self.get_raw_word(address)? {
            RawDataWord::Word64(d) => self.instruction_from_array_ptr(&d, state),
            RawDataWord::Word32(d) => self.instruction_from_array_ptr(&d, state),
            RawDataWord::Word16(d) => self.instruction_from_array_ptr(&d, state),
            RawDataWord::Word8(_) => todo!(),
        }
    }

    fn instruction_from_array_ptr(
        &self,
        data: &[u8],
        state: &GAState<A>,
    ) -> Result<Instruction<A>> {
        state.instruction_from_array_ptr(data)
    }

    /// Get a byte of data from program memory.
    pub fn get_byte(&self, address: u64) -> Result<u8> {
        match self.segments.read_raw_bytes(address, 1) {
            Some(v) => Ok(v[0]),
            None => Err(MemoryError::OutOfBounds.into()),
        }
    }

    fn get_word_internal(&self, address: u64, width: WordSize) -> Result<DataWord> {
        Ok(match width {
            WordSize::Bit64 => match self.segments.read_raw_bytes(address, 8) {
                Some(v) => {
                    let mut data = [0; 8];
                    data.copy_from_slice(v);
                    DataWord::Word64(match self.endianness {
                        Endianness::Little => u64::from_le_bytes(data),
                        Endianness::Big => u64::from_be_bytes(data),
                    })
                }
                None => {
                    return Err(MemoryError::OutOfBounds.into());
                }
            },
            WordSize::Bit32 => match self.segments.read_raw_bytes(address, 4) {
                Some(v) => {
                    let mut data = [0; 4];
                    data.copy_from_slice(v);
                    DataWord::Word32(match self.endianness {
                        Endianness::Little => u32::from_le_bytes(data),
                        Endianness::Big => u32::from_be_bytes(data),
                    })
                }
                None => {
                    return Err(MemoryError::OutOfBounds.into());
                }
            },
            WordSize::Bit16 => match self.segments.read_raw_bytes(address, 2) {
                Some(v) => {
                    let mut data = [0; 2];
                    data.copy_from_slice(v);
                    DataWord::Word16(match self.endianness {
                        Endianness::Little => u16::from_le_bytes(data),
                        Endianness::Big => u16::from_be_bytes(data),
                    })
                }
                None => {
                    return Err(MemoryError::OutOfBounds.into());
                }
            },
            WordSize::Bit8 => DataWord::Word8(self.get_byte(address)?),
        })
    }

    /// Get a word from data memory
    pub fn get_word(&self, address: u64) -> Result<DataWord> {
        self.get_word_internal(address, self.word_size)
    }

    pub fn get_half_word(&self, address: u64) -> Result<DataHalfWord> {
        Ok(match self.word_size {
            WordSize::Bit64 => match self.get_word_internal(address, WordSize::Bit32)? {
                DataWord::Word32(d) => DataHalfWord::HalfWord64(d),
                _ => panic!("Should never reach this part."),
            },
            WordSize::Bit32 => match self.get_word_internal(address, WordSize::Bit16)? {
                DataWord::Word16(d) => DataHalfWord::HalfWord32(d),
                _ => panic!("Should never reach this part."),
            },
            WordSize::Bit16 => match self.get_word_internal(address, WordSize::Bit8)? {
                DataWord::Word8(d) => DataHalfWord::HalfWord16(d),
                _ => panic!("Should never reach this part."),
            },
            WordSize::Bit8 => return Err(ProjectError::UnabvalableOperation),
        })
    }

    pub fn get_raw_word(&self, address: u64) -> Result<RawDataWord> {
        Ok(match self.word_size {
            WordSize::Bit64 => match self.segments.read_raw_bytes(address, 8) {
                Some(v) => {
                    let mut data = [0; 8];
                    data.copy_from_slice(v);
                    RawDataWord::Word64(data)
                }
                None => {
                    return Err(MemoryError::OutOfBounds.into());
                }
            },
            WordSize::Bit32 => match self.segments.read_raw_bytes(address, 4) {
                Some(v) => {
                    let mut data = [0; 4];
                    data.copy_from_slice(v);
                    RawDataWord::Word32(data)
                }
                None => {
                    return Err(MemoryError::OutOfBounds.into());
                }
            },
            WordSize::Bit16 => match self.segments.read_raw_bytes(address, 2) {
                Some(v) => {
                    let mut data = [0; 2];
                    data.copy_from_slice(v);
                    RawDataWord::Word16(data)
                }
                None => {
                    return Err(MemoryError::OutOfBounds.into());
                }
            },
            WordSize::Bit8 => RawDataWord::Word8([self.get_byte(address)?]),
        })
    }
}

impl<A: Arch + Clone + 'static> Debug for Project<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("word_size", &self.word_size)
            .field("endianness", &self.endianness)
            //.field("architecture", &hitecture)
            .finish()
    }
}
