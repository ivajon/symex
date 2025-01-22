//! Defines parsing rules for the ast
//! [`Operands`](crate::ast::operand::Operand).
use syn::{
    parenthesized,
    parse::{discouraged::Speculative, Parse, ParseStream, Result},
    token::Let,
    Expr,
    Ident,
    Token,
    Type,
};

use crate::ast::operand::*;

impl ExprOperand {
    fn parse_first_stage(input: ParseStream) -> Result<Self> {
        let speculative = input.fork();
        if let Ok(function) = speculative.parse() {
            input.advance_to(&speculative);
            return Ok(Self::FunctionCall(function));
        }

        let speculative = input.fork();
        if let Ok(ident) = speculative.parse() {
            if !speculative.peek(syn::token::Paren) {
                input.advance_to(&speculative);
                return Ok(Self::Ident(ident));
            }
        }

        let speculative = input.fork();
        if let Ok(lit) = speculative.parse() {
            input.advance_to(&speculative);
            return Ok(Self::Literal(lit));
        }

        if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            let inner: Expr = content.parse()?;
            if !content.is_empty() {
                return Err(content.error("Expected : (<Expr>)"));
            }
            return Ok(Self::Paren(inner));
        }
        Err(input.error(
            "Expected an ExprOperand here.
    - A function call
    - A literal
    - An idenitifer",
        ))
    }
}
impl Parse for ExprOperand {
    fn parse(input: ParseStream) -> Result<Self> {
        let value = Self::parse_first_stage(input)?;
        if input.peek(Token![.]) {
            let mut ops = vec![];
            while input.peek(Token![.]) {
                let _: Token![.] = input.parse()?;
                let fident: Ident = input.parse()?;
                if input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in input);
                    let operands = content.parse_terminated(Operand::parse, syn::token::Comma)?;
                    ops.push((fident, operands.into_iter().map(Box::new).collect()));
                    continue;
                }
                return Err(input.error("Expected function arguments"));
            }
            return Ok(Self::Chain(Box::new(value), ops));
        }

        Ok(value)
    }
}

impl Parse for Operand {
    fn parse(input: ParseStream) -> Result<Self> {
        let speculative = input.fork();
        if let Ok(val) = speculative.parse() {
            input.advance_to(&speculative);
            return Ok(Self::FieldExtract(val));
        }
        let speculative = input.fork();
        if let Ok(val) = speculative.parse() {
            input.advance_to(&speculative);
            return Ok(Self::Expr(val));
        }

        let speculative = input.fork();
        if let Ok(val) = speculative.parse() {
            input.advance_to(&speculative);
            return Ok(Self::Ident(val));
        }

        Err(input.error("Expected operand"))
    }
}

impl Parse for IdentOperand {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Let) {
            let _: Let = input.parse()?;
            let ident: Ident = input.parse()?;
            return Ok(Self {
                define: true,
                ident,
            });
        }
        let ident: Ident = input.parse()?;
        Ok(Self {
            define: false,
            ident,
        })
    }
}

impl Parse for DelimiterType {
    fn parse(input: ParseStream) -> Result<Self> {
        let speculative = input.fork();
        if let Ok(val) = speculative.parse() {
            input.advance_to(&speculative);
            return Ok(Self::Ident(val));
        }

        Ok(Self::Const(input.parse()?))
    }
}

impl Parse for FieldExtract {
    fn parse(input: ParseStream) -> Result<Self> {
        if !input.peek(Ident) {
            return Err(input.error("Expected Identifier"));
        }
        let operand: Ident = input.parse()?;

        if !input.peek(Token![<]) {
            return Err(input.error("Expected <end:start:ty?>"));
        }

        let _: syn::token::Lt = input.parse()?;

        let end: DelimiterType = input.parse()?;

        if !input.peek(Token![:]) {
            return Err(input.error("Expected <end:start:ty?>"));
        }
        let _: Token![:] = input.parse()?;

        let start: DelimiterType = input.parse()?;

        let speculative = input.fork();
        let ty: Option<Type> = match speculative.parse() {
            Ok(ty) => {
                let _: Token![:] = ty;
                input.advance_to(&speculative);
                Some(input.parse()?)
            }
            Err(_) => None,
        };

        if !input.peek(Token![>]) {
            return Err(input.error("Expected <end:start:ty?>"));
        }
        let _: Token![>] = input.parse()?;

        Ok(Self {
            operand,
            start,
            end,
            ty,
        })
    }
}
