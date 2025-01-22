//! Defines all valid operand types.

use syn::{Expr, Ident, Lit, Type};

use super::function::Function;

/// Enumerates all valid operand types.
#[derive(Debug, Clone)]
pub enum Operand {
    /// A general expression [`ExprOperand`].
    Expr(ExprOperand),
    /// A plain identifier.
    Ident(IdentOperand),
    /// Field extraction.
    FieldExtract(FieldExtract),
}

#[derive(Debug, Clone)]
/// Enumerates a set of different operands.
///
/// These operands are not new identifiers but can be already defined
/// [`Ident`](struct@Ident)ifiers.
pub enum ExprOperand {
    /// A parenthesis containing an ordinary rust expression.
    ///
    /// This allows inline rust expressions the the DSL.
    Paren(Expr),
    /// A chain like
    /// ```ignore
    /// a.local(<args>).unwrap()
    /// ```
    Chain(Box<ExprOperand>, Vec<(Ident, Vec<Box<Operand>>)>),
    /// A plain identifier.
    Ident(Ident),
    /// A plain literal.
    Literal(Lit),
    /// A function call, this can be either a intrinsic function or a rust
    /// function.
    FunctionCall(Function),
}

/// A (possibly) new identifier.
#[derive(Debug, Clone)]
pub struct IdentOperand {
    /// Whether or not to insert this in to the local scope or not
    pub define: bool,
    /// The identifier used
    pub ident: Ident,
}

#[derive(Debug, Clone)]
/// Valid delimiters for a [`FieldExtract`].
pub enum DelimiterType {
    /// Can be a plain number.
    Const(Lit),
    /// Can be a rust variable.
    Ident(Ident),
}

#[derive(Debug, Clone)]
/// Field extraction.
///
/// This extracts the specified number of bits
/// from the operand and right justifies the result.
pub struct FieldExtract {
    /// The operand to extract from.
    pub operand: Ident,
    /// The first bit to include.
    pub start: DelimiterType,
    /// The last bit to include.
    pub end: DelimiterType,
    /// The type for the mask.
    pub ty: Option<Type>,
}
