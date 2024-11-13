//! Defines transpiling rules for the ast
//! [`Operands`](crate::ast::operand::Operand).
use proc_macro2::TokenStream;
use quote::quote;

use crate::{ast::operand::*, Compile, Error};

impl Compile for Operand {
    type Output = TokenStream;

    fn compile(
        &self,
        state: &mut crate::TranspilerState<Self::Output>,
    ) -> Result<Self::Output, Error> {
        match self {
            Self::Expr(e) => e.compile(state),
            Self::Ident(i) => i.compile(state),
            Self::FieldExtract(f) => f.compile(state),
        }
    }
}
impl Compile for ExprOperand {
    type Output = TokenStream;

    fn compile(
        &self,
        state: &mut crate::TranspilerState<Self::Output>,
    ) -> Result<Self::Output, Error> {
        Ok(match self {
            Self::Paren(p) => quote!((#p)),
            Self::Chain(i, it) => {
                let ident: TokenStream = (*i).compile(state)?;
                let mut ops: Vec<TokenStream> = Vec::new();
                for (ident, args) in it {
                    let mut args_ret = Vec::with_capacity(args.len());
                    for arg in args {
                        let arg = arg.compile(state)?;
                        args_ret.push(arg);
                    }
                    ops.push(quote!(#ident(#(#args_ret),*)));
                }
                quote!(#ident.#(#ops).*)
            }
            Self::Ident(i) => {
                state.access(i.clone());
                quote!(#i.clone())
            }
            Self::Literal(l) => quote!(#l),
            Self::FunctionCall(f) => f.compile(state)?,
        })
    }
}
impl Compile for IdentOperand {
    type Output = TokenStream;

    fn compile(
        &self,
        state: &mut crate::TranspilerState<Self::Output>,
    ) -> Result<Self::Output, Error> {
        match self.define {
            true => state.declare_local(self.ident.clone()),
            false => {
                state.access(self.ident.clone());
            }
        };
        let ident = self.ident.clone();
        Ok(quote!(#ident.clone()))
    }
}

impl Compile for DelimiterType {
    type Output = TokenStream;

    fn compile(
        &self,
        state: &mut crate::TranspilerState<Self::Output>,
    ) -> Result<Self::Output, Error> {
        Ok(match self {
            Self::Const(l) => quote!(#l),
            Self::Ident(i) => {
                state.access(i.clone());
                quote!(#i)
            }
        })
    }
}

impl Compile for FieldExtract {
    type Output = TokenStream;

    fn compile(
        &self,
        state: &mut crate::TranspilerState<Self::Output>,
    ) -> Result<Self::Output, Error> {
        let intermediate = state.intermediate().compile(state)?;
        let operand = self.operand.clone();
        state.access(operand.clone());
        let (start, end) = (
            self.start.clone().compile(state)?,
            self.end.clone().compile(state)?,
        );

        state.to_insert_above.extend([quote! (
            Operation::BitFieldExtract{
                destination: #intermediate.clone(),
                operand: #operand.clone(),
                start_bit: #start,
                stop_bit: #end,
            }
        )]);
        Ok(quote! {#intermediate})

        //let intermediate2 = state.intermediate().compile(state)?;
        //state.access(self.operand.clone());
        //let ty = self.ty.clone().unwrap_or(syn::parse_quote!(u32));
        //state.to_insert_above.extend([
        //    quote!(
        //        Operation::Srl {
        //            destination: #intermediate1.clone(),
        //            operand: #operand.clone(),
        //            shift: Operand::Immediate((#start as #ty).into())
        //        }
        //    ),
        //    quote!(
        //        #[allow(clippy::unnecessary_cast)]
        //        Operation::And {
        //            destination: #intermediate2.clone(),
        //            operand1: #intermediate1.clone(),
        //            operand2: Operand::Immediate(
        //                (
        //                    (
        //                        (
        //                            (0b1u64 << (#end as u64 - #start as u64 +
        // 1u64)) as u64                        ) - (1 as u64)
        //                    )as #ty
        //                ).into()
        //            )
        //
        //        }
        //    ),
        //]);
        //Ok(quote!(#intermediate2))
    }
}
