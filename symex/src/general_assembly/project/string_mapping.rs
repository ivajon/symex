//! TODO!
//!
//! Get variable locations from all possible locations, I think we are missing a
//! few indirections as we have no way of perfoming run-time determination at
//! this time.
//!
//!
//!
//!
//! ## goal
//!
//! Move away from
//! ```text
//! r1: 0x12311
//! r2: 0x12341
//! ```
//! to
//! ```text
//! <filename>::<function_name>::<variable_name> = <some value>
//! // Or of it is a struct
//! <filename>::<function_name>::<variable_name> = {
//!     <field_name>: <field_value> // This should reflect the eniterty of the struct at the
//!     latest write.
//! }
//! ```
//!
//! For any non-mappable value we should keep the register name.

#![allow(warnings)]

use std::{
    env::Vars,
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::Index,
    rc::Rc,
};

use gimli::{
    DW_AT_decl_column,
    DW_AT_decl_file,
    DW_AT_decl_line,
    DW_AT_location,
    DW_AT_type,
    DW_TAG_enumeration_type,
    DW_TAG_formal_parameter,
    DW_TAG_member,
    DW_TAG_structure_type,
    DW_TAG_subprogram,
    DW_TAG_variable,
    DW_TAG_variant,
    DW_TAG_variant_part,
    DebugAddr,
    DebugAddrIndex,
    DebugInfoUnitHeadersIter,
    DebugStr,
    DebuggingInformationEntry,
    Dwarf,
    EndianSlice,
    Endianity,
    EntriesCursor,
    Evaluation,
    EvaluationResult,
    Expression,
    Reader,
    Unit,
    UnitHeader,
};

#[derive(Debug)]
pub enum StackError {
    ErrorWhenLoadingUnits,
    InvalidUnitHeader,
    ErrorWhenLoadingEntries,
    UnsupportedTag,
    InvalidFunction,
    InvalidArgument,
    InvalidVariable,
}

#[derive(Clone, Debug)]
pub enum Member {
    Argument(VariableMeta),
    Variable(VariableMeta),
}

/// Option of either A or B.
#[derive(Debug)]
pub enum AorB<A: Sized + Debug, B: Sized + Debug> {
    A(A),
    B(B),
}

impl<A: Sized + Debug + Clone, B: Sized + Debug + Clone> Clone for AorB<A, B> {
    fn clone(&self) -> Self {
        match self {
            Self::A(a) => Self::A(a.clone()),
            Self::B(b) => Self::B(b.clone()),
        }
    }
}

impl<A: Sized + Debug, B: Sized + Debug> AorB<A, B> {
    fn a<'a>(&'a mut self) -> Option<&'a A> {
        match self {
            Self::A(a) => Some(a),
            Self::B(_b) => None,
        }
    }

    fn b<'b>(&'b mut self) -> Option<&'b B> {
        match self {
            Self::B(b) => Some(b),
            Self::A(_a) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stack<'parent> {
    lower_bound: u64,
    upper_bound: u64,
    stack: Vec<Box<Stack<'parent>>>,
    members: Vec<Member>,
    meta: FunctionMeta,
    parent: Option<&'parent Stack<'parent>>,
}

impl<'parent> Default for Stack<'parent> {
    fn default() -> Self {
        Self {
            lower_bound: 0,
            upper_bound: u64::max_value(),
            stack: vec![],
            members: vec![],
            meta: FunctionMeta {
                name: "global_scope".to_string(),
                pc_bound: (0, u64::MAX),
                path: None,
            },
            parent: None,
        }
    }
}

pub trait NameAccessor {
    fn get_function(&self, pc: u64) -> Option<FunctionMeta>;

    fn get_register(&self, pc: u64, register: u8) -> Option<VariableMeta>;

    fn get_address<'a>(&'a self, pc: u64, address: u64) -> Option<&'a VariableMeta>;
}

impl<'parent> NameAccessor for Stack<'parent> {
    fn get_function(&self, pc: u64) -> Option<FunctionMeta> {
        if self.lower_bound <= pc && self.upper_bound >= pc {
            for element in self.stack.iter() {
                if self.lower_bound <= pc && self.upper_bound >= pc {
                    // Possible lower bound.
                    return element.get_function(pc);
                }
            }
            return Some(self.meta.clone());
        }
        // No valid meta.
        None
    }

    fn get_register(&self, pc: u64, register: u8) -> Option<VariableMeta> {
        let mut tightest = self.tightest_fit(pc);
        while let Some(frame) = tightest {
            println!("Looking for {register} in frame :\n{frame}");
            for member in frame.members.iter() {
                match member {
                    Member::Variable(var) | Member::Argument(var) => {
                        if let Some(val) = var.compare_to_register(register) {
                            // We should probably just add in the parent functions name to the
                            // variable name.
                            let mut val = val.clone();
                            val.name = format!("{}::{}({})", frame.meta.name, val.name, val.ty);
                            return Some(val);
                        }
                    }
                }
            }
            // If we have not returned yet we simply go to parent frame.
            tightest = frame.parent;
        }
        None
    }

    fn get_address<'a>(&'a self, pc: u64, address: u64) -> Option<&'a VariableMeta> {
        let mut tightest = self.tightest_fit(pc);

        while let Some(frame) = tightest {
            println!("Checking frame :\n{frame}");
            for member in frame.members.iter() {
                match member {
                    Member::Variable(var) | Member::Argument(var) => {
                        if let Some(val) = var.compare_to_memory_address(address) {
                            return Some(val);
                        }
                    }
                }
            }
            // If we have not returned yet we simply go to parent frame.
            tightest = frame.parent;
        }
        None
    }
}

impl<'parent> Stack<'parent> {
    fn tightest_fit_mut(&mut self, pc: u64) -> Option<&mut Self> {
        if self.lower_bound < pc && self.upper_bound > pc {
            // We are a child of the previous function.
            {
                for idx in 0..(self.stack.len()) {
                    let el = &self.stack[idx];
                    if el.lower_bound <= pc && el.upper_bound >= pc {
                        // This is safe as the loop never exceeds the array bounds.
                        let el = unsafe { self.stack.get_unchecked_mut(idx) };
                        // We are a child of this structure so we should re-run it
                        // in that structure
                        return el.tightest_fit_mut(pc);
                    }
                }
            }
            // Unwrap is safe here as we just pushed an element to the vec.

            return Some(self);
        }
        None
    }

    fn tightest_fit(&self, pc: u64) -> Option<&Self> {
        if self.lower_bound < pc && self.upper_bound > pc {
            // We are a child of the previous function.
            {
                for idx in 0..(self.stack.len()) {
                    let el = &self.stack[idx];
                    if el.lower_bound <= pc && el.upper_bound >= pc {
                        // This is safe as the loop never exceeds the array bounds.
                        let el = unsafe { self.stack.get_unchecked(idx) };
                        // We are a child of this structure so we should re-run it
                        // in that structure
                        return el.tightest_fit(pc);
                    }
                }
            }
            // Unwrap is safe here as we just pushed an element to the vec.

            println!("Tightest fit : \n{}", self);
            return Some(self);
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct FunctionMeta {
    /// The name of the function.
    pub name: String,
    /// The lower and upper bound of the function.
    pub pc_bound: (u64, u64),
    /// The Path in the file system including an optional line number.
    pub path: Option<(String, Option<u64>)>,
}

#[derive(Clone, Debug)]
pub struct VariableMeta {
    pub name: String,
    pub path: Option<(String, Option<u64>)>,
    pub ty: String,
    pub location: Option<Location>,
}

enum Meta {
    Function(FunctionMeta),
    Argument(VariableMeta),
    Variable(VariableMeta),
}

impl PartialEq for FunctionMeta {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

pub struct StructMeta {
    name: String,
    members: Vec<StructMeta>,
}

pub struct EnumMeta {
    name: String,
    members: Vec<StructMeta>,
}

pub enum StructType {
    Struct(StructMeta),
    Enum(EnumMeta),
}
pub struct StructLookup {
    structs: Vec<StructType>,
}

impl VariableMeta {
    fn compare_to_register(&self, reg: u8) -> Option<&Self> {
        match self.location {
            Some(Location::Register(r)) => {
                if reg == r {
                    return Some(self);
                }
            }
            _ => {}
        }
        None
    }

    fn compare_to_memory_address(&self, addr: u64) -> Option<&Self> {
        match self.location {
            Some(Location::Memory(l, h)) => {
                if l <= addr && h >= addr {
                    return Some(self);
                }
            }
            _ => {}
        }
        None
    }
}

impl Meta {
    fn parse_function<'unit, 'abbrev, R: gimli::Reader>(
        dwarf: &Dwarf<R>,
        unit: &Unit<R, R::Offset>,
        entry: &DebuggingInformationEntry<'abbrev, 'unit, R>,
    ) -> Option<Self> {
        // Assumes that we are actually parsing a function.

        let debug_str = &dwarf.debug_str;

        // 1. We must have a name for the function, a lot of the other meta data is
        //    optional.
        let name = resolve_name(entry, debug_str)?;

        // 2. Retrive the pc ranges where this is valid, if there is none we should not
        //    really bother with this function for symex purposes.
        let pc_bound = resolve_pc_bound(entry, debug_str)?;

        let path = resolve_path(entry, debug_str, unit);
        Some(Self::Function(FunctionMeta {
            name,
            pc_bound,
            path,
        }))
    }

    fn parse_struct<'unit, 'abbrev, R: gimli::Reader>(
        dwarf: &Dwarf<R>,
        unit: &Unit<R, R::Offset>,
        entries: &mut EntriesCursor<'abbrev, 'unit, R>,
        entry: &DebuggingInformationEntry<'abbrev, 'unit, R>,
    ) -> Option<Self> {
        // Assumes that we are actually parsing a struct.
        let debug_str = &dwarf.debug_str;
        let name = resolve_name(entry, debug_str)?;
        let tag = entry.tag();

        #[derive(Debug, Clone)]
        struct Member {
            name: String,
        }

        #[derive(Debug, Clone)]
        struct Variant {
            name: String,
            denominator_value: usize,
            contains: Option<AorB<Box<Struct>, Box<Enum>>>,
        }

        #[derive(Default, Debug, Clone)]
        struct Struct {
            name: String,
            fields: Vec<Member>,
        }

        #[derive(Debug, Clone)]
        struct Enum {
            name: String,
            denominator_location: (),
            denominator_type: (),
            variants: Vec<Variant>,
        }

        // These operations are purely helpers to eliviate a bit of the verbosity
        // needed.
        impl AorB<Box<Struct>, Box<Enum>> {
            fn set_name(&mut self, name: String) {
                match self {
                    Self::A(s) => s.name = name,
                    Self::B(e) => e.name = name,
                }
            }

            fn add_field(&mut self, field: Member) {
                match self {
                    Self::A(s) => s.fields.push(field),
                    Self::B(e) => panic!("Tried to add field to enum {e:?}"),
                }
            }

            fn add_variant(&mut self, variant: Variant) {
                match self {
                    Self::A(_s) => panic!("Tried to add variant to struct."),
                    Self::B(e) => e.variants.push(variant),
                }
            }

            fn set_denominator(&mut self, variant: ()) {
                todo!("Figure out denominators");
                //match self {
                //    Self::A(_s) => panic!("Tried to add variant to struct."),
                //    Self::B(e) => e.variants.push(variant)
                //}
            }
        }

        fn enum_from_struct(structure: Struct) -> AorB<Box<Struct>, Box<Enum>> {
            todo!()
        }

        let mut parsed = AorB::A(Box::new(Struct::default()));

        parsed.set_name(name.clone());

        //let mut members = Vec::new();
        // For enums https://github.com/rust-lang/rust/issues/32920
        //

        #[derive(PartialEq)]
        enum Parseable {
            Enum {
                denominator_complete: bool,
                variants_complete: bool,
                waiting_for_vairant: bool,
            },
            Struct,
            None,
        }
        let mut parsing = Parseable::Struct;

        let mut parsing_enum = false;
        let mut was_leaf = false;

        let mut started = false;
        while let Ok(Some((_, entry))) = entries.next_dfs() {
            // Enum structure in dwarf:
            //
            //
            // structure_type <name>
            //  DW_TAG_variant_part
            //      DW_TAG_member <-- Describes the denominator i.e. 0..max enum len
            //      DW_TAG_variant
            //          DW_TAG_member <-- Enum variants, these have name and type, the type
            // reffers          forward in to the comming fields.
            //
            //      DW_TAG_structure_type (parse struct recursily) <-- Contains name,
            // previous members reffer to these          <optional members> <--
            // Contain name, alignment type etc etc
            //

            // If we have a struct we might be parsing an enum.
            if parsing == Parseable::Struct && tag == DW_TAG_variant_part {
                parsing = Parseable::Enum {
                    denominator_complete: false,
                    variants_complete: false,
                    waiting_for_vairant: false,
                };
                let structure = parsed.a().unwrap();
                let structure = *structure.clone();
                parsed = enum_from_struct(structure);
                continue;
            }

            match &mut parsing {
                Parseable::None => {}
                Parseable::Enum {
                    denominator_complete,
                    variants_complete,
                    waiting_for_vairant,
                } => 'm: {
                    // First field is the denominator.
                    if !*denominator_complete && tag == DW_TAG_member {
                        println!("Denominators is {entry:?}");
                        *denominator_complete = true;
                        break 'm;
                    }

                    if !*variants_complete {
                        'parse_variant: {
                            // Now we need to manage parsing of variants.
                            if tag == DW_TAG_variant {
                                *waiting_for_vairant = true;
                                break 'm;
                            }

                            if !*waiting_for_vairant {
                                println!("Parsing variant before DW_TAG_variant");
                                break 'parse_variant;
                            }

                            // Now we should have members, there might be other valid tags here
                            //
                            // TODO: Find all valid tags here.

                            if tag != DW_TAG_member {
                                *waiting_for_vairant = false;
                                // This is not a valid variant.
                                break 'm;
                            }

                            // This is a member.
                            //
                            //

                            *variants_complete = true;
                        }
                        *variants_complete = true;
                    }

                    // Now we need to handle member structs.
                    //
                    // 1. Clone cursor and peek next element, if it is a struct,
                    //    parse a struct from it, keep that meta here as a sub
                    //    struct.
                    // 2. Repeat 1 until no more member structs. We need to be
                    //    carefull here as to ensure that the struct parsing
                    //    never goes in to any other scope.
                }

                Parseable::Struct => {}
            }

            if entry.tag() == DW_TAG_variant_part {
                println!("Structure {name} is an enum");
                break;
            }

            if entry.tag() != DW_TAG_member {
                // We found all members.
                break;
            }

            // We have a struct member.
            let member_name = match resolve_name(entry, debug_str) {
                Some(name) => name,
                None => continue,
            };
            // We have a struct member.
            let ty_name = match resolve_type(entry, debug_str, unit) {
                Some(name) => name,
                None => continue,
            };

            println!("Found member: {name}::{member_name} : {ty_name}");
        }
        println!("Found struct with name {:?}", name);

        None
    }

    fn parse_enum<'unit, 'abbrev, R: gimli::Reader>(
        dwarf: &Dwarf<R>,
        unit: &Unit<R, R::Offset>,
        entry: &DebuggingInformationEntry<'abbrev, 'unit, R>,
    ) -> Option<Self> {
        // Assumes that we are actually parsing a struct.
        let debug_str = &dwarf.debug_str;
        let name = resolve_name(entry, debug_str)?;

        // This is slightly ineffiecient but it is fine fow now.

        let mut entries = unit.entries();

        let mut found_struct = false;

        #[derive(Debug, Clone)]
        struct Member {
            name: String,
        }

        #[derive(Debug, Clone)]
        struct Variant {
            name: String,
            denominator_value: usize,
            contains: Option<AorB<Box<Struct>, Box<Enum>>>,
        }

        #[derive(Default, Debug, Clone)]
        struct Struct {
            name: String,
            fields: Vec<Member>,
        }

        #[derive(Debug, Clone)]
        struct Enum {
            name: String,
            denominator_location: (),
            denominator_type: (),
            variants: Vec<Variant>,
        }

        // These operations are purely helpers to eliviate a bit of the verbosity
        // needed.
        impl AorB<Box<Struct>, Box<Enum>> {
            fn set_name(&mut self, name: String) {
                match self {
                    Self::A(s) => s.name = name,
                    Self::B(e) => e.name = name,
                }
            }

            fn add_field(&mut self, field: Member) {
                match self {
                    Self::A(s) => s.fields.push(field),
                    Self::B(e) => panic!("Tried to add field to enum {e:?}"),
                }
            }

            fn add_variant(&mut self, variant: Variant) {
                match self {
                    Self::A(_s) => panic!("Tried to add variant to struct."),
                    Self::B(e) => e.variants.push(variant),
                }
            }

            fn set_denominator(&mut self, variant: ()) {
                todo!("Figure out denominators");
                //match self {
                //    Self::A(_s) => panic!("Tried to add variant to struct."),
                //    Self::B(e) => e.variants.push(variant)
                //}
            }
        }

        fn enum_from_struct(structure: Struct) -> AorB<Box<Struct>, Box<Enum>> {
            todo!()
        }

        let mut parsed = AorB::A(Box::new(Struct::default()));

        parsed.set_name(name.clone());

        //let mut members = Vec::new();
        // For enums https://github.com/rust-lang/rust/issues/32920
        //

        #[derive(PartialEq)]
        enum Parseable {
            Enum {
                denominator_complete: bool,
                variants_complete: bool,
                waiting_for_vairant: bool,
            },
            Struct,
            None,
        }
        let mut parsing = Parseable::None;

        let mut parsing_enum = false;
        let mut was_leaf = false;

        let mut started = false;
        while let Ok(Some((_, current_entry))) = entries.next_dfs() {
            if !started && entry.code() == current_entry.code() {
                println!("Found that {:?} == {:?}", entry, current_entry);
                started = true;
            }
            let entry = current_entry;
            let tag = entry.tag();
            // Enum structure in dwarf:
            //
            //
            // structure_type <name>
            //  DW_TAG_variant_part
            //      DW_TAG_member <-- Describes the denominator i.e. 0..max enum len
            //      DW_TAG_variant
            //          DW_TAG_member <-- Enum variants, these have name and type, the type
            // reffers          forward in to the comming fields.
            //
            //      DW_TAG_structure_type (parse struct recursily) <-- Contains name,
            // previous members reffer to these          <optional members> <--
            // Contain name, alignment type etc etc
            //

            if parsing == Parseable::None && tag == DW_TAG_structure_type {
                match resolve_name(entry, debug_str) {
                    Some(inner_name) => {
                        if inner_name != name {
                            println!("This should not happen, inner_name = {inner_name} and outer_name = {name}");
                            panic!();
                            continue;
                        }
                    }
                    None => continue,
                };
                parsing = Parseable::Struct;
                continue;
            }
            // If we have a struct we might be parsing an enum.
            if parsing == Parseable::Struct && tag == DW_TAG_variant_part {
                parsing = Parseable::Enum {
                    denominator_complete: false,
                    variants_complete: false,
                    waiting_for_vairant: false,
                };
                let structure = parsed.a().unwrap();
                let structure = *structure.clone();
                parsed = enum_from_struct(structure);
                continue;
            }

            match &mut parsing {
                Parseable::None => {}
                Parseable::Enum {
                    denominator_complete,
                    variants_complete,
                    waiting_for_vairant,
                } => 'm: {
                    // First field is the denominator.
                    if !*denominator_complete && tag == DW_TAG_member {
                        println!("Denominators is {entry:?}");
                        *denominator_complete = true;
                        break 'm;
                    }

                    if !*variants_complete {
                        'parse_variant: {
                            // Now we need to manage parsing of variants.
                            if tag == DW_TAG_variant {
                                *waiting_for_vairant = true;
                                break 'm;
                            }

                            if !*waiting_for_vairant {
                                println!("Parsing variant before DW_TAG_variant");
                                break 'parse_variant;
                            }

                            // Now we should have members, there might be other valid tags here
                            //
                            // TODO: Find all valid tags here.

                            if tag != DW_TAG_member {
                                *waiting_for_vairant = false;
                                // This is not a valid variant.
                                break 'm;
                            }

                            // This is a member.
                        }
                        *variants_complete = true;
                    }
                }

                Parseable::Struct => {}
            }

            if entry.tag() == DW_TAG_variant_part {
                println!("Structure {name} is an enum");
                break;
            }

            if entry.tag() != DW_TAG_member {
                // We found all members.
                break;
            }

            // We have a struct member.
            let member_name = match resolve_name(entry, debug_str) {
                Some(name) => name,
                None => continue,
            };
            // We have a struct member.
            let ty_name = match resolve_type(entry, debug_str, unit) {
                Some(name) => name,
                None => continue,
            };

            println!("Found member: {name}::{member_name} : {ty_name}");
        }
        println!("Found struct with name {:?}", name);

        None
    }

    fn parse_variable<'unit, 'abbrev, R: gimli::Reader>(
        dwarf: &Dwarf<R>,
        unit: &Unit<R, R::Offset>,
        entry: &DebuggingInformationEntry<'abbrev, 'unit, R>,
    ) -> Option<Self> {
        // Assumes that we are actually parsing a variable.
        let debug_str = &dwarf.debug_str;
        let name = resolve_name(entry, debug_str)?;

        // TODO! Check if we can actaully get PC ranges for variables.

        let path = resolve_path(entry, debug_str, unit);

        let location = resolve_location(entry, dwarf, unit);

        let ty = match resolve_type(entry, debug_str, unit) {
            Some(ty) => ty,
            None => {
                println!("Cannot find type for {name} stored in {location:?}");
                "".to_string()
            }
        };

        Some(Self::Variable(VariableMeta {
            name,
            path,
            ty,
            location,
        }))
    }

    fn parse_argument<'unit, 'abbrev, R: gimli::Reader>(
        dwarf: &Dwarf<R>,
        unit: &Unit<R, R::Offset>,
        entry: &DebuggingInformationEntry<'abbrev, 'unit, R>,
    ) -> Option<Self> {
        // Assumes that we are actually parsing a function.

        let debug_str = &dwarf.debug_str;

        // 1. We must have a name for the function, a lot of the other meta data is
        //    optional.
        let name = resolve_name(entry, debug_str)?;

        // TODO! Check if we can actaully get PC ranges for variables.

        let path = resolve_path(entry, debug_str, unit);

        let location = resolve_location(entry, dwarf, unit);

        let ty = match resolve_type(entry, debug_str, unit) {
            Some(ty) => ty,
            None => {
                println!("Cannot find type for {name} stored in {location:?}");
                return None;
            }
        };

        Some(Self::Argument(VariableMeta {
            name,
            path,
            ty,
            location,
        }))
    }
}

impl<'unit, 'cursor, 'abbrev, 'gimli, 'entry, R: gimli::Reader>
    TryFrom<(
        &'gimli Dwarf<R>,
        &'unit Unit<R, R::Offset>,
        &'cursor mut EntriesCursor<'abbrev, 'unit, R>,
        &'entry DebuggingInformationEntry<'abbrev, 'unit, R>,
    )> for Meta
{
    type Error = StackError;

    fn try_from(
        value: (
            &'gimli Dwarf<R>,
            &'unit Unit<R, R::Offset>,
            &'cursor mut EntriesCursor<'abbrev, 'unit, R>,
            &'entry DebuggingInformationEntry<'abbrev, 'unit, R>,
        ),
    ) -> Result<Self, Self::Error> {
        let (dwarf, unit, cursor, entry) = value;

        match entry.tag() {
            DW_TAG_subprogram => Ok(match Self::parse_function(dwarf, unit, entry) {
                Some(func) => func,
                None => return Err(StackError::InvalidFunction),
            }),
            DW_TAG_formal_parameter => Ok(match Self::parse_argument(dwarf, unit, entry) {
                Some(func) => func,
                None => return Err(StackError::InvalidArgument),
            }),
            DW_TAG_variable => Ok(match Self::parse_variable(dwarf, unit, entry) {
                Some(func) => func,
                None => return Err(StackError::InvalidVariable),
            }),
            DW_TAG_structure_type => Ok(match Self::parse_struct(dwarf, unit, cursor, entry) {
                Some(func) => func,
                None => return Err(StackError::InvalidVariable),
            }),
            DW_TAG_enumeration_type => Ok(match Self::parse_enum(dwarf, unit, entry) {
                Some(e) => e,
                None => return Err(StackError::InvalidVariable),
            }),
            _ => Err(StackError::UnsupportedTag),
        }
    }
}

impl<'parent, R: gimli::Reader> TryFrom<&Dwarf<R>> for Stack<'parent> {
    type Error = StackError;

    fn try_from(dwarf: &Dwarf<R>) -> Result<Self, Self::Error> {
        let mut units = dwarf.units();

        let current_function = FunctionMeta {
            name: "global scope".to_owned(),
            pc_bound: (0, 0),
            path: None,
        };

        let mut stack = Stack {
            lower_bound: 0,
            upper_bound: u64::max_value(),
            stack: Vec::new(),
            members: Vec::new(),
            meta: current_function,
            parent: None,
        };
        {
            let mut current = Some(&mut stack);
            while let Some(unit_header) = units
                .next()
                .map_err(|_| StackError::ErrorWhenLoadingUnits)?
            {
                let unit = dwarf
                    .unit(unit_header)
                    .map_err(|_| StackError::InvalidUnitHeader)?;
                let mut entries = unit.entries();

                loop {
                    let entry = if let Some((_, entry)) = entries
                        .next_dfs()
                        .map_err(|_| StackError::ErrorWhenLoadingEntries)?
                    {
                        entry.clone()
                    } else {
                        break;
                    };

                    let res = Meta::try_from((dwarf, &unit, &mut entries, &entry));
                    let val = match res {
                        Ok(val) => val,
                        // Not fatal, we should probably notify the user here.
                        Err(StackError::UnsupportedTag)
                        | Err(StackError::InvalidArgument)
                        | Err(StackError::InvalidVariable) => continue,
                        Err(StackError::InvalidFunction) => {
                            current = None;
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    match val {
                        Meta::Function(func) => {
                            current =
                                Some(get_tightest_fit(&mut stack, func).expect(
                                    "Function is malformed, we should never reach this error",
                                ));
                        }
                        Meta::Argument(arg) => {
                            if let Some(current) = &mut current {
                                current.members.push(Member::Argument(arg));
                            }
                        }
                        Meta::Variable(var) => {
                            if let Some(current) = &mut current {
                                current.members.push(Member::Variable(var));
                            }
                        }
                    }
                }
            }
        }
        println!("PROGRAM STACK : {stack}");

        Ok(stack)
    }
}

fn get_tightest_fit<'shorter, 'parent>(
    structure: &'shorter mut Stack<'parent>,
    meta: FunctionMeta,
) -> Result<&'shorter mut Stack<'parent>, ()> {
    if structure.lower_bound <= meta.pc_bound.0 && structure.upper_bound >= meta.pc_bound.1 {
        // We are a child of the previous function.
        {
            for idx in 0..(structure.stack.len()) {
                let el = &structure.stack[idx];
                if el.lower_bound < meta.pc_bound.0 && el.upper_bound > meta.pc_bound.1 {
                    // This is safe as the loop never exceeds the array bounds.
                    let el = unsafe { structure.stack.get_unchecked_mut(idx) };
                    // We are a child of this structure so we should re-run it
                    // in that structure
                    return get_tightest_fit(el, meta);
                }
            }
        }
        // there were not nested children so we can simply append to stack.
        structure.stack.push(Box::new(Stack {
            lower_bound: meta.pc_bound.0,
            upper_bound: meta.pc_bound.1,
            stack: Vec::new(),
            members: Vec::new(),
            meta,
            parent: structure.parent.clone(),
        }));
        // Unwrap is safe here as we just pushed an element to the vec.
        return Ok(structure.stack.last_mut().unwrap());
    }
    //println!("{structure:?},\nto_add : {meta:?}");
    Err(())
}

fn resolve_name<R: gimli::Reader>(
    entry: &DebuggingInformationEntry<R>,
    debug_str: &DebugStr<R>,
) -> Option<String> {
    if let Some(name_attr) = entry
        .attr(gimli::DW_AT_name)
        .expect("Failed to read name attribute")
    {
        Some(match name_attr.value() {
            gimli::AttributeValue::DebugStrRef(offset) => {
                let intermediate = debug_str.get_str(offset).expect("Invalid offset");
                let string = intermediate.to_string_lossy().ok()?;
                string.to_string()
            }
            gimli::AttributeValue::String(raw) => raw.to_string_lossy().ok()?.to_string(),
            _ => {
                panic!()
            }
        })
    } else {
        None
    }
}

fn resolve_pc_bound<R: gimli::Reader>(
    entry: &DebuggingInformationEntry<R>,
    debug_str: &DebugStr<R>,
) -> Option<(u64, u64)> {
    let low_pc_attr = entry.attr(gimli::DW_AT_low_pc).ok()??;
    //println!("LOWPC : {:?}", low_pc_attr);
    let high_pc_attr = entry.attr(gimli::DW_AT_high_pc).ok()??;
    //println!("HIGHPC : {:?}", high_pc_attr);

    ////println!("FOUND A NAME!");

    let low_pc = match low_pc_attr.value() {
        gimli::AttributeValue::Addr(pc) => pc,
        gimli::AttributeValue::Udata(pc) => {
            todo!("How do I interpret this?");
            //pc
        }
        v => {
            //println!("Hmmm, {:?}", v);
            return None;
        }
    };

    let high_pc = match high_pc_attr.value() {
        gimli::AttributeValue::Addr(pc) => pc,
        gimli::AttributeValue::Udata(pc) => low_pc + pc,
        v => {
            //println!("Hmmm, {:?}", v);
            return None;
        }
    };

    Some((low_pc, high_pc))
}

pub fn resolve_path<R: gimli::Reader>(
    entry: &DebuggingInformationEntry<R>,
    debug_str: &DebugStr<R>,
    unit: &Unit<R>,
) -> Option<(String, Option<u64>)> {
    let file = entry.attr(DW_AT_decl_file).ok()??;
    let line = entry.attr(DW_AT_decl_line).ok()?;
    let col = entry.attr(DW_AT_decl_column).ok()?;

    let file_name = match file.value() {
        gimli::AttributeValue::String(raw) => raw.to_string_lossy().ok()?.to_string(),
        gimli::AttributeValue::DebugStrRef(offset) => debug_str
            .get_str(offset)
            .ok()?
            .to_string_lossy()
            .ok()?
            .to_string(),
        gimli::AttributeValue::FileIndex(idx) => {
            let lp = unit.line_program.clone()?;
            let header = lp.header();
            let fp = header.file(idx)?;
            match fp.path_name() {
                gimli::AttributeValue::String(s) => s.to_string_lossy().ok()?.to_string(),
                op => todo!("{:?}", op),
            }
        }
        opt => todo!("{:?}", opt),
    };

    if line.is_none() {
        return Some((file_name, None));
    }
    let line = match line {
        Some(line) => line,
        None => unreachable!(),
    };

    // TODO! Validate that this mapping is correct.
    let line = match line.value() {
        gimli::AttributeValue::Data1(line) => line as u64,
        gimli::AttributeValue::Data2(line) => line as u64,
        gimli::AttributeValue::Data4(line) => line as u64,
        gimli::AttributeValue::Data8(line) => line as u64,
        val => {
            //println!("Manage lines defined elsewhere {:?}", val);
            0
        }
    };
    Some((file_name, Some(line)))
}

fn resolve_type<R: gimli::Reader>(
    entry: &DebuggingInformationEntry<R>,
    debug_str: &DebugStr<R>,
    unit: &Unit<R>,
) -> Option<String> {
    let attr = entry.attr(DW_AT_type).ok()??;

    ////println!("unit_ref: {:?}", unit_ref);
    if let gimli::AttributeValue::UnitRef(offset) = attr.value() {
        let entry = unit.entry(offset).unwrap();
        return resolve_name(&entry, debug_str);
    }

    None
}

#[derive(Clone, Debug)]
pub enum Location {
    Memory(u64, u64),
    Register(u8),
    List(Vec<Location>),
}

fn resolve_location<R: gimli::Reader>(
    entry: &DebuggingInformationEntry<R>,
    dwarf: &Dwarf<R>,
    unit: &Unit<R>,
) -> Option<Location> {
    let loc = entry.attr(DW_AT_location).ok()??;

    match loc.value() {
        gimli::AttributeValue::Exprloc(location) => {
            //println!("Location expression : {:?}", location);
            let mut res = location.evaluation(unit.encoding());

            let result = res.evaluate();
            if let Ok(EvaluationResult::Complete) = result {
                let result = res.result();
                return match &result.first()?.location {
                    gimli::Location::Register { register } => {
                        Some(Location::Register(register.0 as u8))
                    }
                    gimli::Location::Address { address } => {
                        Some(Location::Memory(*address, *address))
                    }
                    loc => todo!("Unhandled location {:?}", loc),
                };
            } else {
                //println!("Result : {:?}", result);
            }
        }
        gimli::AttributeValue::SecOffset(offset) => {
            let section = &dwarf.debug_addr;
            // TODO! Fix this assumeption
            let base = section
                .get_address(32, unit.addr_base, DebugAddrIndex(offset))
                .unwrap();
            return Some(Location::Memory(base, base));
        }
        gimli::AttributeValue::LocationListsRef(localtion_list_ref) => {
            let section = &dwarf.locations;
            let mut locations = section
                .raw_locations(localtion_list_ref, unit.encoding())
                .unwrap();
            let mut res_locations = vec![];
            while let Ok(Some(location)) = locations.next() {
                let new_location = match location {
                    // TODO! Pretty sure that these begin/end values are PC values not data memory,
                    // so this should be corrected, the Expressions are actually the register
                    // mapping for the different field parts, so correct this.
                    gimli::RawLocListEntry::AddressOrOffsetPair { begin, end, data } => {
                        //println!("From AddressOrOffsetPair");
                        Location::Memory(begin, end)
                    }
                    gimli::RawLocListEntry::StartEnd { begin, end, data } => {
                        //println!("From StartEnd");
                        Location::Memory(begin, end)
                    }
                    gimli::RawLocListEntry::StartxEndx { begin, end, data } => {
                        //println!("From StartxEndx");
                        let section = &dwarf.debug_addr;
                        let begin = section.get_address(32, unit.addr_base, begin).unwrap();
                        let end = section.get_address(32, unit.addr_base, end).unwrap();
                        Location::Memory(begin, end)
                    }
                    // Here we should try to figure out the width of the data to get a start/stop
                    // bound.
                    gimli::RawLocListEntry::BaseAddress { addr } => Location::Memory(addr, addr),
                    // TODO! Again, here we need to resolve the "data" part.
                    gimli::RawLocListEntry::StartLength {
                        begin,
                        length,
                        data,
                    } => Location::Memory(begin, begin + length),
                    gimli::RawLocListEntry::BaseAddressx { addr } => {
                        ////println!("From BaseAddressx");
                        todo!()
                    }
                    gimli::RawLocListEntry::StartxLength {
                        begin,
                        length,
                        data,
                    } => {
                        ////println!("From StartxLength");
                        todo!()
                    }
                    gimli::RawLocListEntry::DefaultLocation { data } => {
                        ////println!("From DefaultLocation");
                        todo!()
                    }
                    gimli::RawLocListEntry::OffsetPair { begin, end, data } => {
                        ////println!("From OffsetPair");
                        todo!()
                    }
                };
                res_locations.push(new_location);
            }
            return Some(Location::List(res_locations));
        }
        e => {
            todo!("Unexpected location : {:?}", e);
        }
    }
    if let gimli::AttributeValue::Exprloc(location) = loc.value() {}

    None
}

//
//pub enum EvalutaionIntermediary<R:Reader,Result:From<gimli::Value>> {
//    RequiresRegister{
//        register:String,
//        callback:Box<dyn FnOnce(u64) -> EvalutaionIntermediary<R,Result>>
//    },
//    RequiresAddress{
//        address:u64,
//        callback:Box<dyn FnOnce(u64) -> EvalutaionIntermediary<R,Result>>
//    },
//    Complete(Result)
//}
//
//impl<R:Reader,Result:From<gimli::Value>>
// From<(gimli::Encoding,Expression<R>)> for  EvalutaionIntermediary<R,Result> {
//
//    fn from(value: (gimli::Encoding,Expression<R>)) -> Self {
//        let (enc,expr) = value;
//        let res = expr.evaluation(enc);
//        match res.evaluate().unwrap() {
//            gimli::EvaluationResult::Complete =>
// Self::Complete(res.value_result().unwrap().into()),
// gimli::EvaluationResult::        }
//    }
//}

impl Member {
    fn inner<'a>(&'a self) -> &VariableMeta {
        match self {
            Self::Argument(val) => val,
            Self::Variable(val) => val,
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory(start, stop) => write!(f, "{start},{stop}"),
            Self::Register(reg) => write!(f, "Register({reg})"),
            Self::List(locations) => write!(
                f,
                "List({})",
                locations
                    .iter()
                    .map(|el| format!("{el}"))
                    .collect::<Vec<String>>()
                    .join(",")
            ),
        }
    }
}

impl<'parent> Stack<'parent> {
    fn fmt_internal(&self, prefix: String) -> String {
        let args: String = self
            .members
            .iter()
            .filter(|el| {
                if let Member::Argument(arg) = el {
                    true
                } else {
                    false
                }
            })
            .map(|el| el.inner())
            .map(|el| format!("{}({:?}):{}", el.name, el.location, el.ty))
            .collect::<Vec<String>>()
            .join(",");

        let vars: Vec<String> = self
            .members
            .iter()
            .filter(|el| {
                if let Member::Variable(arg) = el {
                    true
                } else {
                    false
                }
            })
            .map(|el| el.inner())
            .map(|el| format!("var {}({:?}):{}", el.name, el.location, el.ty))
            .collect::<Vec<String>>();

        let fn_name = &self.meta.name;

        let pc_start = format!("PC : {}", self.meta.pc_bound.0);
        let pc_end = format!("    PC : {}", self.meta.pc_bound.1);

        let n_spaces = pc_end.len().max(pc_start.len());
        let spaces = " ".repeat(n_spaces);
        let pc_start_padding =
            " ".repeat(((pc_start.len() as isize) - (n_spaces as isize)).abs() as usize);
        let pc_end_padding =
            " ".repeat(((pc_end.len() as isize) - (n_spaces as isize)).abs() as usize);

        let first = format!("{prefix}|{pc_start_padding}{pc_start} fn {fn_name}({args}):");
        let last_line = format!("{prefix}|{pc_end_padding}{pc_end} end");

        let old_prefix = prefix.clone();
        let prefix = format!("{prefix}|{spaces}");
        let vars = format!("{prefix}{}", vars.join(format!("\r\n{prefix}").as_str()));

        let subs = self
            .stack
            .iter()
            .map(|el| (*el).fmt_internal(prefix.clone()))
            .collect::<Vec<String>>()
            .join("\r\n");

        format!("{first}\r\n{vars}\r\n{subs}{last_line}\r\n{old_prefix}\r\n")
    }
}

impl<'parent> Display for Stack<'parent> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fmt_internal("".to_string()))
    }
}
