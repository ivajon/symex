//! Helper functions to read dwarf debug data.

use std::collections::{HashMap, HashSet};

use gimli::{
    AttributeValue,
    DW_AT_low_pc,
    DW_AT_name,
    DW_TAG_subprogram,
    DebugAbbrev,
    DebugInfo,
    DebugPubNames,
    DebugStr,
    Reader,
};
use regex::Regex;
use tracing::{debug, trace};

use super::{PCHook, PCHooks};
use crate::general_assembly::arch::Arch;

/// Constructs a list of address hook pairs from a list of symbol name hook
/// pairs.
///
/// It does this by finding the name of the symbol in the dwarf debug data and
/// if it is a function(subprogram) it adds the address and hook to the hooks
/// list.
#[allow(dead_code)]
pub fn construct_pc_hooks<R: Reader, A: Arch>(
    hooks: &Vec<(Regex, PCHook<A>)>,
    pub_names: &DebugPubNames<R>,
    debug_info: &DebugInfo<R>,
    debug_abbrev: &DebugAbbrev<R>,
) -> PCHooks<A> {
    trace!("Constructing PC hooks");
    let mut ret: PCHooks<A> = HashMap::new();
    let mut name_items = pub_names.items();
    let mut found_hooks = HashSet::new();
    'inner: while let Some(pubname) = name_items.next().unwrap() {
        let item_name = pubname.name().to_string_lossy().unwrap();
        for (name, hook) in hooks {
            if name.is_match(item_name.as_ref()) {
                let unit_offset = pubname.unit_header_offset();
                let die_offset = pubname.die_offset();

                let unit = debug_info.header_from_offset(unit_offset).unwrap();
                let abbrev = unit.abbreviations(debug_abbrev).unwrap();
                let die = unit.entry(&abbrev, die_offset).unwrap();

                let die_type = die.tag();
                if die_type == DW_TAG_subprogram {
                    let addr = match die.attr_value(DW_AT_low_pc).unwrap() {
                        Some(v) => v,
                        None => continue 'inner,
                    };
                    found_hooks.insert(name.as_str());

                    if let AttributeValue::Addr(addr_value) = addr {
                        trace!("found hook for {} att addr: {:#X}", name, addr_value);
                        ret.insert(addr_value, *hook);
                    }
                }
            }
        }
    }
    if found_hooks.len() < hooks.len() {
        debug!("Did not find addresses for all hooks.")
    }
    ret
}

pub fn construct_pc_hooks_no_index<R: Reader, A: Arch>(
    hooks: &Vec<(Regex, PCHook<A>)>,
    debug_info: &DebugInfo<R>,
    debug_abbrev: &DebugAbbrev<R>,
    debug_str: &DebugStr<R>,
) -> PCHooks<A> {
    trace!("Constructing PC hooks");
    let mut ret: PCHooks<A> = HashMap::new();
    let mut found_hooks = HashSet::new();

    let mut units = debug_info.units();
    while let Some(unit) = units.next().unwrap() {
        let abbrev = unit.abbreviations(debug_abbrev).unwrap();
        let mut cursor = unit.entries(&abbrev);

        'inner: while let Some((_dept, entry)) = cursor.next_dfs().unwrap() {
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
            let name_str = entry_name.to_string().unwrap();

            for (name, hook) in hooks {
                if name.is_match(name_str.as_ref()) {
                    let addr = match entry.attr_value(DW_AT_low_pc).unwrap() {
                        Some(v) => v,
                        None => continue 'inner,
                    };
                    found_hooks.insert(name.as_str());

                    if let AttributeValue::Addr(addr_value) = addr {
                        trace!("found hook for {} att addr: {:#X}", name, addr_value);
                        ret.insert(addr_value, *hook);
                    }
                }
            }
        }
    }
    if found_hooks.len() < hooks.len() {
        debug!("Did not find addresses for all hooks.") // fix a proper error
                                                        // here later
    }

    ret
}
