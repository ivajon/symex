//! Defines the supported arithmetic operations.

use super::operand::Operand;

/// A generic operation,
///
/// This allows syntax like
/// ```ignore
/// let a = b + c + d;
/// ```
pub enum Operation {
    /// A binary operation.
    BinOp(Operand, BinaryOperation, Operand),
    /// A unary operation.
    UnOp(UnaryOperation, Operand),
}

/// Enumerates all valid binary operations.
///
/// This is merely a type-level denotation of
/// operations such as + or -.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum BinaryOperation {
    Sub,
    SSub,
    Add,
    SAdd,
    AddWithCarry,
    SDiv,
    UDiv,
    Mul,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    LogicalLeftShift,
    LogicalRightShift,
    ArithmeticRightShift,
}

/// Enumerates all valid unary operations.
///
/// This is merely a type-level denotation of
/// operations such as !.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum UnaryOperation {
    BitwiseNot,
}

/// An assign statement.
///
/// This is syntactically equivalent to
/// ```ignore
/// a = b;
/// ```
#[derive(Debug, Clone)]
pub struct Assign {
    /// Where to store the rhs.
    pub dest: Operand,
    /// The value to be copied in to the
    /// destination.
    pub rhs: Operand,
}

/// A unary operation.
///
/// This is syntactically equivalent to
/// ```ignore
/// a = !b;
/// ```
#[derive(Debug, Clone)]
pub struct UnOp {
    /// Where to store the result.
    pub dest: Operand,
    /// What operation to apply.
    pub op: UnaryOperation,
    /// The operand to apply the operation to.
    pub rhs: Operand,
}

/// A binary operation.
///
/// This is syntactically equivalent to
/// ```ignore
/// a = b + c; // Or any other binary operation
/// ```
#[derive(Debug, Clone)]
pub struct BinOp {
    /// Where to store the result.
    pub dest: Operand,
    /// Which operation to apply.
    pub op: BinaryOperation,
    /// The lhs of the operation.
    pub lhs: Operand,
    /// The rhs of the operation.
    pub rhs: Operand,
}

impl BinaryOperation {
    /// Converts the operation to be signed.
    pub fn signed(&mut self) {
        *self = match &self {
            Self::UDiv => Self::SDiv,
            e => (*e).clone(),
        };
    }
}

impl BinOp {
    /// Converts the operation to be signed.
    pub fn signed(&mut self) {
        self.op.signed();
    }
}
