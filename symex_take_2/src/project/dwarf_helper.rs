//! Helper functions to read dwarf debug data.

use gimli::{
    AttributeValue,
    DW_AT_decl_file,
    DW_AT_decl_line,
    DW_AT_high_pc,
    DW_AT_low_pc,
    DW_AT_name,
    DebugAbbrev,
    DebugInfo,
    DebugStr,
    Reader,
};
use hashbrown::HashMap;
use regex::Regex;
use tracing::trace;

#[derive(Clone, Debug, Hash)]
pub struct SubProgram {
    pub name: String,
    pub bounds: (u64, u64),
    pub file: Option<(String, usize)>,
    /// Call site for an inlined sub routine.
    pub call_file: Option<(String, usize)>,
}

#[derive(Clone, Debug)]
pub struct SubProgramMap {
    index_1: HashMap<String, u64>,
    index_2: HashMap<u64, u64>,
    map: HashMap<u64, SubProgram>,
    counter: u64,
}

impl SubProgramMap {
    fn _new() -> Self {
        Self {
            index_1: HashMap::new(),
            index_2: HashMap::new(),
            map: HashMap::new(),
            counter: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            index_1: HashMap::new(),
            index_2: HashMap::new(),
            map: HashMap::new(),
            counter: 0,
        }
    }

    fn insert(&mut self, name: String, address: u64, value: SubProgram) {
        let _ = self.index_1.insert(name, self.counter);
        let _ = self.index_2.insert(address, self.counter);
        let _ = self.map.insert(self.counter, value);
        self.counter += 1;
    }

    pub fn get_by_name(&mut self, name: &str) -> Option<&SubProgram> {
        let idx = self.index_1.get(name)?;
        self.map.get(idx)
    }

    pub fn get_by_address(&mut self, address: &u64) -> Option<&SubProgram> {
        let idx = self.index_2.get(address)?;
        self.map.get(idx)
    }

    pub fn get_by_regex(&self, pattern: &'static str) -> Option<&SubProgram> {
        let regex = Regex::new(pattern).ok()?;
        for (idx, prog) in self.index_1.iter() {
            if regex.is_match(idx) {
                return Some(self.map.get(prog)?);
            }
        }
        None
    }

    pub fn new<R: Reader>(
        debug_info: &DebugInfo<R>,
        debug_abbrev: &DebugAbbrev<R>,
        debug_str: &DebugStr<R>,
    ) -> SubProgramMap {
        trace!("Constructing PC hooks");
        let mut ret: SubProgramMap = SubProgramMap::_new();
        let mut units = debug_info.units();
        while let Some(unit) = units.next().unwrap() {
            let abbrev = unit.abbreviations(debug_abbrev).unwrap();
            let mut cursor = unit.entries(&abbrev);

            while let Some((_dept, entry)) = cursor.next_dfs().unwrap() {
                let tag = entry.tag();
                if tag != gimli::DW_TAG_subprogram {
                    // is not a function continue the search
                    continue;
                }
                let attr = match entry.attr_value(DW_AT_name).unwrap() {
                    Some(a) => a,
                    None => continue,
                };

                let entry_name = match attr {
                    AttributeValue::DebugStrRef(s) => s,
                    _ => continue,
                };
                let entry_name = debug_str.get_str(entry_name).unwrap();
                let name = entry_name.to_string().unwrap().to_string();

                let addr = match entry.attr_value(DW_AT_low_pc).unwrap() {
                    Some(AttributeValue::Addr(v)) => v,
                    Some(AttributeValue::Data1(v)) => v as u64,
                    Some(AttributeValue::Data2(v)) => v as u64,
                    Some(AttributeValue::Data4(v)) => v as u64,
                    Some(AttributeValue::Data8(v)) => v,
                    Some(AttributeValue::Udata(val)) => val,
                    _ => continue,
                };
                let addr_end = match entry.attr_value(DW_AT_high_pc).unwrap() {
                    Some(AttributeValue::Data1(v)) => v as u64,
                    Some(AttributeValue::Data2(v)) => v as u64,
                    Some(AttributeValue::Data4(v)) => v as u64,
                    Some(AttributeValue::Data8(v)) => v,
                    Some(AttributeValue::Udata(val)) => val,
                    _val => 0,
                };
                let file = match entry.attr_value(DW_AT_decl_file).unwrap() {
                    Some(AttributeValue::String(s)) => s.to_string().unwrap().to_string(),
                    _ => "".to_string(),
                };
                let line = match entry.attr_value(DW_AT_decl_line).unwrap() {
                    Some(AttributeValue::Data1(v)) => v as usize,
                    Some(AttributeValue::Data2(v)) => v as usize,
                    Some(AttributeValue::Data4(v)) => v as usize,
                    Some(AttributeValue::Data8(v)) => v as usize,
                    Some(AttributeValue::Udata(val)) => val as usize,
                    _ => 0,
                };

                ret.insert(name.clone(), addr, SubProgram {
                    name,
                    bounds: (addr, addr + addr_end as u64),
                    file: Some((file, line)),
                    call_file: None,
                });
            }
        }
        ret
    }
}
