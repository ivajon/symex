use std::fmt::Display;

use colored::Colorize;

use crate::{
    executor::PathResult,
    logging::{Logger, Region, RegionMetaData},
    manager::SymexArbiter,
    project::dwarf_helper::{SubProgram, SubProgramMap},
};

#[derive(Default)]
pub struct PathLog {
    statements: Vec<(Option<SubProgram>, String)>,
    final_state: String,
    result: String,
    constraints: Vec<String>,
    execution_time: String,
    visited: Vec<String>,
}

struct PathLogger<'logger> {
    sub_program: &'logger Option<SubProgram>,
    path: &'logger mut PathLog,
}

pub struct SimpleLogger {
    regions: SubProgramMap,
    current_region: Option<SubProgram>,
    paths: Vec<PathLog>,
    pc: u64,
    path_idx: usize,
}

impl SimpleLogger {
    fn path_logger(&mut self) -> PathLogger<'_> {
        while self.path_idx >= self.paths.len() {
            self.paths.push(PathLog::default());
        }
        PathLogger {
            path: &mut self.paths[self.path_idx],
            sub_program: &self.current_region,
        }
    }
}

impl<'a> PathLogger<'a> {
    fn write(&mut self, statement: String) {
        self.path
            .statements
            .push((self.sub_program.clone(), statement));
    }

    fn finalize(&mut self, state: String) {
        self.path.result = state;
    }

    fn final_state(&mut self, state: String) {
        self.path.final_state = state;
    }

    fn constrain(&mut self, assumption: String) {
        self.path.constraints.push(assumption);
    }

    fn execution_time(&mut self, time: String) {
        self.path.execution_time = time;
    }

    fn visit(&mut self, func: String) {
        println!("Executing in {func}");
        if Some(&func) != self.path.visited.last() {
            self.path.visited.push(func);
        }
    }
}

impl Logger for SimpleLogger {
    type RegionDelimiter = u64;
    type RegionIdentifier = SubProgram;

    fn update_delimiter<T: Into<Self::RegionDelimiter>>(&mut self, region: T) {
        self.pc = region.into();
        self.current_region = self.regions.get_by_address(&self.pc).cloned();
        if let Some(region) = &self.current_region {
            let name = region.name.clone();
            self.path_logger().visit(name);
        }
    }

    fn warn<T: ToString>(&mut self, warning: T) {
        self.path_logger()
            .write(format!("[{}]: {}", "WARN".yellow(), warning.to_string()));
    }

    fn record_path_result<C: crate::Composition>(
        &mut self,
        path_result: crate::executor::PathResult<C>,
    ) {
        let res = format!("Result: {}", match path_result {
            PathResult::Suppress => "Path supressed".yellow(),
            PathResult::Success(Some(expression)) => format!("Success ({:?})", expression).green(),
            PathResult::Success(None) => format!("Success").green(),
            PathResult::Failure(cause) => format!("Failure {cause}",).red(),
            PathResult::AssumptionUnsat => "Unsatisfiable".red(),
        });
        self.path_logger().finalize(res);
    }

    fn change_path(&mut self, new_path_idx: usize) {
        while new_path_idx >= self.paths.len() {
            self.paths.push(PathLog::default());
        }
        self.path_idx = new_path_idx;
    }

    fn error<T: ToString>(&mut self, error: T) {
        self.path_logger()
            .write(format!("[{}]: {}", "ERROR".red(), error.to_string()));
    }

    fn assume<T: ToString>(&mut self, assumption: T) {
        self.path_logger()
            .write(format!("[{}]: {}", "ASSUME".blue(), assumption.to_string()));
    }

    fn current_region(&self) -> Option<Self::RegionIdentifier> {
        self.current_region.clone()
    }

    fn add_constraints(&mut self, constraints: Vec<String>) {
        for constrain in constraints {
            self.path_logger().constrain(constrain);
        }
    }

    fn register_region(&mut self, _region: Self::RegionIdentifier) {
        todo!();
    }

    fn record_final_state<C: crate::Composition>(
        &mut self,
        state: crate::executor::state::GAState<C>,
    ) {
        let memory = state.memory;

        self.path_logger().final_state(format!("{memory}"));
    }

    fn record_execution_time<T: ToString>(&mut self, time: T) {
        self.path_logger().execution_time(time.to_string());
    }

    fn new<C: crate::Composition>(state: &SymexArbiter<C>) -> Self {
        Self {
            regions: state.get_symbol_map().clone(),
            current_region: None,
            paths: vec![PathLog::default()],
            pc: 0,
            path_idx: 0,
        }
    }
}

impl SimpleLogger {
    pub fn from_sub_programs(state: &SubProgramMap) -> Self {
        Self {
            regions: state.clone(),
            current_region: None,
            paths: vec![PathLog::default()],
            pc: 0,
            path_idx: 0,
        }
    }
}

impl ToString for SubProgram {
    fn to_string(&self) -> String {
        let file = match &self.file {
            Some((name, line)) => format!("<({name}@{line})>"),
            None => String::new(),
        };
        format!("Subprogram({}) {file}", self.name)
    }
}
impl Region for SubProgram {
    fn global() -> Self {
        Self {
            name: "GLOBAL:".to_string(),
            bounds: (0, u64::MAX),
            file: None,
            call_file: None,
        }
    }
}

impl From<RegionMetaData> for SubProgram {
    fn from(value: RegionMetaData) -> Self {
        let RegionMetaData {
            name,
            start,
            end,
            area_delimiter: _,
            instructions: _,
            execution_time: _,
        } = value;

        Self {
            name: name.unwrap_or("".to_string()),
            bounds: (start, end),
            file: None,
            call_file: None,
        }
    }
}

impl Display for SimpleLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (
            idx,
            PathLog {
                statements,
                final_state,
                result,
                constraints,
                execution_time,
                visited,
            },
        ) in self.paths.iter().enumerate()
        {
            write!(
                f,
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ PATH {} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\r\n",
                idx
            )?;
            if !statements.is_empty() {
                write!(f, "Logs:\r\n")?;
                for (program, statement) in statements {
                    write!(f, "\t")?;
                    if let Some(SubProgram {
                        name,
                        bounds: _,
                        file: Some((file, _line)),
                        call_file: _,
                    }) = program
                    {
                        write!(f, "<{name} ({file})> -> ")?;
                    }
                    if let Some(SubProgram {
                        name,
                        bounds: _,
                        file: None,
                        call_file: _,
                    }) = program
                    {
                        write!(f, "<{name}> -> ")?;
                    }

                    write!(f, "{}\r\n", statement)?;
                }
            }
            if !visited.is_empty() {
                write!(f, "Visited:\r\n")?;
                for constraint in visited {
                    write!(f, "{}", constraint)?;
                }
            }
            if !constraints.is_empty() {
                write!(f, "Constraints:\r\n")?;
                for constraint in constraints {
                    write!(f, "{}\r\n", constraint)?;
                }
            }

            write!(f, "Final state : \r\n\t{}\r\n", final_state)?;
            write!(f, "Execution took : {}\r\n", execution_time)?;

            write!(f, "{}\r\n", result)?;
        }

        Ok(())
    }
}
