use std::hash::Hash;

use crate::{
    executor::{state::GAState2, PathResult},
    Composition,
};

/// Denotes meta data regarding a region of code.
#[derive(Hash)]
pub struct RegionMetaData {
    /// Region label if any.
    name: Option<String>,
    /// Start value for delimiter.
    start: u64,
    /// End value for delimiter.
    end: u64,
    /// Typically delimited by PC.
    area_delimiter: String,

    /// The instructions contained in the region.
    instructions: Vec<String>,

    /// The instructions contained in the region.
    execution_time: Vec<String>,
}

/// The execution does not use a logger.
#[derive(Hash)]
pub struct NoLogger;

pub trait Region {
    /// Returns the global scope.
    fn global() -> Self;
}

/// A generic logger used to generate reports.
pub trait Logger {
    type RegionIdentifier: Sized + ToString + From<RegionMetaData> + Hash + Region;

    /// Assumes that the constraint holds.
    fn assume<T: ToString>(&mut self, region: Self::RegionIdentifier, assumption: T);

    /// An issue occurred, non terminal but might be problematic.
    fn warn<T: ToString>(&mut self, region: Self::RegionIdentifier, warning: T);

    /// An issue occurred, probably terminal for the current path.
    fn error<T: ToString>(&mut self, region: Self::RegionIdentifier, error: T);

    /// Records the result of the current path.
    fn record_path_result<C: Composition>(&mut self, path_result: PathResult<C>);

    /// Records the final state of the current path.
    fn record_final_state<C: Composition>(&mut self, state: GAState2<C>);

    /// Changes to a new path in the executor.
    ///
    /// If this is path has been partially explored before it will simply append
    /// to the previous logs.
    fn change_path(&mut self, new_path_idx: usize);

    /// Adds constraint info to the currently executing path.
    fn add_constraints(&mut self, constraints: Vec<String>);

    /// Report of execution time, typically this will include a set of meta data
    /// instructions such as start PC end PC etc.
    fn record_execution_time<T: ToString>(&mut self, region: Self::RegionIdentifier, time: T);

    /// Possibly changes region.
    ///
    /// This can be used to generate traces of the program.
    fn register_new_delimiter_value<T: ToString>(&mut self, delimiter: u64, time: T);

    /// Returns the current region if any is detected.
    fn current_region(&self) -> Option<Self::RegionIdentifier>;

    /// Adds a new region to the logger.
    fn register_region(&mut self, region: Self::RegionIdentifier);
}

impl Logger for NoLogger {
    type RegionIdentifier = RegionMetaData;

    fn warn<T: ToString>(&mut self, _region: Self::RegionIdentifier, _warning: T) {}

    fn error<T: ToString>(&mut self, _region: Self::RegionIdentifier, _error: T) {}

    fn assume<T: ToString>(&mut self, _region: Self::RegionIdentifier, _assumption: T) {}

    fn record_execution_time<T: ToString>(&mut self, _region: Self::RegionIdentifier, _time: T) {}

    fn change_path(&mut self, _new_path_idx: usize) {}

    fn add_constraints(&mut self, _constraints: Vec<String>) {}

    fn current_region(&self) -> Option<Self::RegionIdentifier> {
        None
    }

    fn record_final_state<C: Composition>(&mut self, _state: GAState2<C>) {}

    fn register_new_delimiter_value<T: ToString>(&mut self, _delimiter: u64, _time: T) {}

    fn register_region(&mut self, _region: Self::RegionIdentifier) {}

    fn record_path_result<C: Composition>(&mut self, _path_result: PathResult<C>) {}
}

impl From<RegionMetaData> for NoLogger {
    fn from(_value: RegionMetaData) -> Self {
        NoLogger
    }
}

impl ToString for RegionMetaData {
    fn to_string(&self) -> String {
        let area_delimiter = self.area_delimiter.clone();
        format!(
            "region (name: \\bold{{{}}} from $`{area_delimiter} = {}`$ to $`{area_delimiter} = {}`$",
            self.name.as_ref().map_or("No name",|v| v),
            self.start,
            self.end
        )
    }
}

impl Region for RegionMetaData {
    fn global() -> Self {
        Self {
            name: Some("Gloabal scope".to_string()),
            start: 0,
            end: u64::MAX,
            area_delimiter: "PC".to_string(),
            instructions: vec![],
            execution_time: vec![],
        }
    }
}
