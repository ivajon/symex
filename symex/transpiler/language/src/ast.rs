//! Defines the intermediate representation of the language.

pub mod function;
pub mod operand;
pub mod operations;
use function::Function;
use operations::{Assign, BinOp, UnOp};
use syn::{Expr, Ident};

use self::function::Jump;

#[derive(Debug, Clone)]
/// Top level intermediate representation of the program.
pub struct IR {
    /// The symbol to insert the generated code in to.
    pub ret: Option<Ident>,
    /// The values to insert into `ret`.
    pub extensions: Vec<Statement>,
}

#[derive(Debug, Clone)]
/// Top level syntactical element.
pub enum Statement {
    /// A general if statement.
    ///
    /// If condition is a rust expression following the same syntax as normal
    /// rust. The body of the if statement contains [`Statement`]s
    /// so does the optional else block.
    If(Expr, Box<Vec<Statement>>, Option<Box<Vec<Statement>>>),
    /// A general for loop.
    ///
    /// The for loop follows normal rust syntax.
    /// The body of the for statement contains [`Statement`]s.
    For(Ident, Expr, Box<Vec<Statement>>),
    /// A collection of [`IRExpr`]s.
    Exprs(Vec<Box<IRExpr>>),
}

#[derive(Debug, Clone)]
/// Intermediate representation expression.
///
/// Defines the valid expressions in the intermediate language
pub enum IRExpr {
    /// A unary operation.
    UnOp(UnOp),
    /// A binary operation.
    BinOp(BinOp),
    /// A simple assignment operation.
    Assign(Assign),
    /// A function call.
    Function(Function),
    /// A jump function call.
    Jump(Jump),
}
