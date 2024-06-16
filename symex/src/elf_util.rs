//! Utility structures mostly related to passing information to runner and
//! display to user.
use core::fmt::{self, Write};

use colored::*;
use indenter::indented;

use crate::{
    general_assembly::{arch::Arch, state::GAState, GAError},
    smt::DExpr,
};

/// Result for a single path of execution.
///
/// This contains which path it was, if it succeded or not. If it failed the
/// error will have a stack trace to where the error occured.
///
/// All input variables and variables used in `symbolic` calls will also have
/// solutions available.
#[derive(Debug)]
pub struct VisualPathResult {
    /// Which path this is.
    pub path: usize,

    /// The final value from the path.
    ///
    /// If the path failed the reason vill be in the error. Otherwise there will
    /// be a value unless the analyzed function returned void.
    pub result: PathStatus,

    /// Variables explicitly marked as symbolic.
    pub symbolics: Vec<Variable>,

    /// All register values att the end of execution.
    pub end_state: Vec<Variable>,

    /// The number of instructions executed in the path
    pub instruction_count: usize,

    /// the maximum number of clock cycles the path can contain
    pub max_cycles: usize,

    /// cycle counts at marked events
    pub cycle_laps: Vec<(usize, String)>,
}

fn elf_get_values<'a, I>(vars: I, state: &GAState<impl Arch>) -> Result<Vec<Variable>, GAError>
where
    I: Iterator<Item = &'a Variable>,
{
    let mut results = Vec::new();
    for var in vars {
        let constant = state.constraints.get_value(&var.value)?;
        let var = Variable {
            name: var.name.clone(),
            value: constant,
            ty: var.ty.clone(),
        };
        results.push(var);
    }

    Ok(results)
}

impl VisualPathResult {
    /// Creates a result from a state.
    pub fn from_state(
        state: GAState<impl Arch>,
        path_num: usize,
        result: PathStatus,
    ) -> Result<Self, GAError> {
        let symbolics = elf_get_values(state.marked_symbolic.iter(), &state)?;
        let registers: Vec<Variable> = state
            .registers
            .iter()
            .map(|(reg_name, value)| Variable {
                name: Some(reg_name.to_owned()),
                value: value.to_owned(),
                ty: ExpressionType::Integer(state.project.get_word_size() as usize),
            })
            .collect();
        let end_state = elf_get_values(registers.iter(), &state)?;

        Ok(VisualPathResult {
            path: path_num,
            result,
            symbolics,
            end_state,
            instruction_count: state.get_instruction_count(),
            max_cycles: state.cycle_count,
            cycle_laps: state.cycle_laps.clone(),
        })
    }
}

impl fmt::Display for VisualPathResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ PATH {} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            self.path
        )?;

        match &self.result {
            PathStatus::Ok(None) => {
                writeln!(f, "{}: returned void", "Success".green())?;
            }
            PathStatus::Ok(Some(value)) => {
                writeln!(f, "{}: returned {}", "Success".green(), value)?;
            }
            PathStatus::Failed(err) => {
                writeln!(f, "{}: {}", "Error".red(), err.error_message)?;
            }
        }

        if !self.symbolics.is_empty() {
            writeln!(f, "\nSymbolic:")?;
            for value in self.symbolics.iter() {
                let name = if let Some(name) = value.name.as_ref() {
                    name
                } else {
                    "_"
                };
                writeln!(indented(f), "{name}: {}", value)?;
            }
        }

        if !self.end_state.is_empty() {
            writeln!(f, "\nEnd state:")?;
            for value in self.end_state.iter() {
                let name = if let Some(name) = value.name.as_ref() {
                    name
                } else {
                    "_"
                };
                writeln!(indented(f), "{name}: {}", value)?;
            }
        }

        writeln!(f, "Instructions executed: {}", self.instruction_count)?;

        writeln!(f, "Max number of cycles: {}", self.max_cycles)?;

        Ok(())
    }
}

/// Status of the path.
///
/// If the path succeeded the return value (if any) is contained in that
/// variant. Otherwise, the reason for failure is contained in the `Failure`
/// variant.
#[derive(Debug, Clone, PartialEq)]
pub enum PathStatus {
    /// The path finished successfully.
    Ok(Option<Variable>),

    /// The path failed.
    Failed(ErrorReason),
}

/// Detailed description of why a run failed.
///
/// Contains the error message, where the error happend and the stack trace from
/// the point of failure.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorReason {
    /// Error message from the received error.
    pub error_message: String,
}

/// One line in the stack trace. Contains the name of the function and the line
/// where it occurred.
#[derive(Debug, Clone, PartialEq)]
pub struct LineTrace {
    /// Name of the function.
    pub function_name: String,

    /// Source location if available.
    pub line: Option<String>,
}

/// Symbolic variable that should be able to be displayed to an end user.
///
/// Variable can be things such as inputs, variables marked as symbolic and
/// outputs. To show this to an end user, the variable must have been solved
/// before trying to show it.
#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    /// `name` is the source name of the variable, if it exists.
    pub name: Option<String>,

    /// Expression of the variable. This can be multiple values, and the solver
    /// should be invoked before presenting to the end-user. This allows to
    /// skip a (possible expensive) solve if not required.
    pub value: DExpr,

    /// Simple representation of the variable.
    pub ty: ExpressionType,
}

impl fmt::Display for Variable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw = self.value.to_binary_string();
        match self.ty.to_typed_variable(&raw) {
            Some(typed_variable) => {
                write!(f, "{typed_variable}")
            }
            None => write!(f, "{raw} (unknown)"),
        }
    }
}

/// Type information for a an expression. This should be generic enough for all
/// kinds of executor to support.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionType {
    /// Integer value of a certain size in bits.
    Integer(usize),

    /// Floating point of a certain size in bits.
    Float(usize),

    /// Array or vector of a certain type with a specific number of values.
    Array(Box<ExpressionType>, usize),

    /// Structure
    Struct(Vec<ExpressionType>),

    /// Type is unknown.
    Unknown,
}

impl ExpressionType {
    fn size_in_bits(&self) -> Option<usize> {
        match self {
            ExpressionType::Integer(bits) => Some(*bits),
            ExpressionType::Float(bits) => Some(*bits),
            ExpressionType::Array(e, n) => {
                let element_size = e.size_in_bits()?;
                Some(*n * element_size)
            }
            ExpressionType::Struct(elements) => {
                let mut size_in_bits = 0;
                for element in elements.iter() {
                    size_in_bits += element.size_in_bits()?;
                }
                Some(size_in_bits)
            }
            ExpressionType::Unknown => None,
        }
    }

    fn to_typed_variable<'a>(&self, raw: &'a str) -> Option<TypedVariable<'a>> {
        match self {
            ExpressionType::Integer(bits) => {
                assert!(raw.len() == *bits);
                Some(TypedVariable::Integer(raw, *bits))
            }
            ExpressionType::Float(bits) => Some(TypedVariable::Float(raw, *bits)),
            ExpressionType::Array(ty, num_elements) => {
                let mut vars = Vec::with_capacity(*num_elements);
                let size = ty.size_in_bits()?;

                // Reverse the order, as elements begin at the end.
                for i in (0..*num_elements).rev() {
                    let start = i * size;
                    let end = (i + 1) * size;
                    let e = ty.to_typed_variable(&raw[start..end])?;
                    vars.push(e);
                }

                Some(TypedVariable::Array(vars))
            }
            ExpressionType::Struct(fields) => {
                let mut elements = Vec::with_capacity(fields.len());

                // First field is located at the end of the raw string.
                let mut offset = raw.len();

                for field in fields.iter() {
                    let size = field.size_in_bits()?;
                    let (start, end) = (offset - size, offset);

                    let element = field.to_typed_variable(&raw[start..end])?;
                    elements.push(element);

                    offset -= size;
                }

                Some(TypedVariable::Struct(elements))
            }
            ExpressionType::Unknown => None,
        }
    }
}

/// Helper for displaying a [Variable].
#[derive(Debug, Clone)]
enum TypedVariable<'a> {
    /// Integer value of a certain size in bits.
    Integer(&'a str, usize),

    /// Floating point of a certain size in bits.
    Float(&'a str, usize),

    /// Array or vector of a certain type with a specific number of values.
    Array(Vec<TypedVariable<'a>>),

    /// Structure
    Struct(Vec<TypedVariable<'a>>),
}

impl<'a> fmt::Display for TypedVariable<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TypedVariable::*;

        match self {
            Integer(value, bits) => {
                let bits_str = if *bits == 1 { "bit" } else { "bits" };
                let value = match u128::from_str_radix(value, 2) {
                    Ok(v) => v,
                    Err(_) => {
                        return write!(f, "unable to display: ({bits}-{bits_str})");
                    }
                };

                const BITS_IN_BYTES: usize = 8;
                const BITS_PER_HEX_CHAR: usize = 4;
                match *bits % BITS_IN_BYTES {
                    0 => {
                        // Get number of hex chars and add two for "0x" start.
                        let width = *bits / BITS_PER_HEX_CHAR + 2;
                        write!(f, "{value:#0width$x} ({bits}-{bits_str})")
                    }
                    _ => {
                        // Add two to number of bits for "0b" start.
                        let width = *bits + 2;
                        write!(f, "{value:#0width$b} ({bits}-{bits_str})")
                    }
                }
            }
            Float(value, bits) => match bits {
                32 => {
                    let value = u32::from_str_radix(value, 2).unwrap();
                    let value = f32::from_bits(value);
                    write!(f, "{value} (f32)")
                }
                64 => {
                    let value = u64::from_str_radix(value, 2).unwrap();
                    let value = f64::from_bits(value);
                    write!(f, "{value} (f64)")
                }
                _ => {
                    write!(f, "{value} (float)")
                }
            },
            Array(elements) => {
                let elements = elements
                    .iter()
                    .map(|e| format!("{e}"))
                    .reduce(|acc, s| format!("{acc}, {s}"));

                match elements {
                    Some(elements) => write!(f, "[{elements}]"),
                    None => write!(f, "[]"),
                }
            }
            Struct(elements) => match elements.len() {
                0 => {
                    write!(f, "Struct {{}}")
                }
                _ => {
                    writeln!(f, "Struct {{")?;
                    for element in elements {
                        writeln!(indented(f), "{element}")?;
                    }
                    write!(f, "}}")
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TypedVariable;

    #[test]
    fn i64_works() {
        // 123_456 = 0b0001_1110_0010_0100_0000 = 0x1e240
        let typed_variable = TypedVariable::Integer("00011110001001000000", 64);
        let s = format!("{typed_variable}");
        assert_eq!(s, "0x000000000001e240 (64-bits)");
    }

    #[test]
    fn i1_works() {
        let typed_variable = TypedVariable::Integer("1", 1);
        let s = format!("{typed_variable}");
        assert_eq!(s, "0b1 (1-bit)");
    }
}
