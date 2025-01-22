//! Defines a simple backed to transpile the [`ast`](crate::ast)
//! into [`Operations`](general_assembly::operation::Operation).

pub mod function;
pub mod operand;
pub mod operations;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

use crate::{ast::*, Compile, Error, TranspilerState};

impl From<IR> for Result<TokenStream, Error> {
    fn from(value: IR) -> Result<TokenStream, Error> {
        // let mut declarations: Vec<TokenStream> = vec![];
        // self.extensions
        //     .iter()
        //     .for_each(|el| el.declare(&mut declarations));
        let mut state = TranspilerState::new();
        state.enter_scope();

        let ret = value.ret.clone().unwrap_or(format_ident!("ret"));
        let mut ext = Vec::new();

        for el in value.extensions {
            ext.push((ret.clone(), el).compile(&mut state)?);
        }
        let declarations = state.to_declare()?;
        let declaration_strings = declarations.iter().map(|el| el.to_string());
        state.to_declare()?;
        Ok(match value.ret {
            Some(_) => quote!(
                #(let #declarations =
                  Operand::Local(#declaration_strings.to_owned());)*
                #(#ext;)*
            ),
            None => quote!(
                {
                    let mut ret =  Vec::new();
                    #(let #declarations =
                      Operand::Local(#declaration_strings.to_owned());)*
                    #(#ext;)*
                    ret
                }
            ),
        })
    }
}

impl Compile for IRExpr {
    type Output = TokenStream;

    fn compile(
        &self,
        state: &mut crate::TranspilerState<Self::Output>,
    ) -> Result<Self::Output, Error> {
        match self {
            Self::Assign(assign) => assign.compile(state),
            Self::UnOp(unop) => unop.compile(state),
            Self::BinOp(binop) => binop.compile(state),
            Self::Function(f) => f.compile(state),
            Self::Jump(j) => j.compile(state),
        }
    }
}

impl Compile for (Ident, Statement) {
    type Output = TokenStream;

    fn compile(&self, state: &mut TranspilerState<Self::Output>) -> Result<Self::Output, Error> {
        let ret = match self.1.clone() {
            Statement::If(e, happy_case_in, Some(sad_case_in)) => {
                state.enter_scope();
                // let to_declare_global: Vec<Ident> = state.to_declare()?;
                // let declaration_strings_global = to_declare_global.iter().map(|el|
                // el.to_string());

                let mut happy_case: Vec<TokenStream> = Vec::new();
                for el in (*happy_case_in).into_iter() {
                    happy_case.push((self.0.clone(), el).compile(state)?);
                }
                let to_declare_happy: Vec<Ident> = state.to_declare()?;
                let declaration_strings_happy = to_declare_happy.iter().map(|el| el.to_string());

                state.enter_scope();
                let mut sad_case: Vec<TokenStream> = Vec::new();
                for el in (*sad_case_in).into_iter() {
                    sad_case.push((self.0.clone(), el).compile(state)?);
                }
                let to_declare_sad: Vec<Ident> = state.to_declare()?;
                let declaration_strings_sad = to_declare_sad.iter().map(|el| el.to_string());

                Ok(quote!(
                    // #(let #to_declare_global =
                        // Operand::Local(#declaration_strings_global.to_owned());)*
                    if #e {
                        #(let #to_declare_happy =
                            Operand::Local(#declaration_strings_happy.to_owned());)*
                        #(#happy_case;)*
                    } else {
                        #(let #to_declare_sad =
                            Operand::Local(#declaration_strings_sad.to_owned());)*
                        #(#sad_case;)*
                    }
                ))
            }
            Statement::If(e, happy_case_in, None) => {
                state.enter_scope();
                // let to_declare_global: Vec<Ident> = state.to_declare()?;
                // let declaration_strings_global = to_declare_global.iter().map(|el|
                // el.to_string());

                let mut happy_case: Vec<TokenStream> = Vec::new();
                for el in (*happy_case_in).into_iter() {
                    happy_case.push((self.0.clone(), el).compile(state)?);
                }
                let to_declare_happy: Vec<Ident> = state.to_declare()?;
                let declaration_strings_happy = to_declare_happy.iter().map(|el| el.to_string());
                Ok(quote!(
                    // #(let #to_declare_global =
                        // Operand::Local(#declaration_strings_global.to_owned());)*
                    if #e {
                        #(let #to_declare_happy =
                            Operand::Local(#declaration_strings_happy.to_owned());)*
                        #(#happy_case;)*
                    }
                ))
            }
            Statement::For(i, e, block_in) => {
                state.enter_scope();
                // let to_declare_global: Vec<Ident> = state.to_declare()?;
                // let declaration_strings_global = to_declare_global.iter().map(|el|
                // el.to_string());
                let mut block: Vec<TokenStream> = Vec::new();
                for el in (*block_in).into_iter() {
                    block.push((self.0.clone(), el).compile(state)?);
                }
                let to_declare_inner: Vec<Ident> = state.to_declare()?;
                let declaration_strings_inner = to_declare_inner.iter().map(|el| el.to_string());
                Ok(quote!(
                    // #(let #to_declare_global =
                        // Operand::Local(#declaration_strings_global.to_owned());)*
                    for #i in #e {
                        #(let #to_declare_inner =
                            Operand::Local(#declaration_strings_inner.to_owned());)*
                        #(#block;)*
                    }
                ))
            }
            Statement::Exprs(extensions) => {
                let mut ext = Vec::new();
                for el in extensions {
                    ext.push(el.compile(state)?);
                }
                let ret = self.0.clone();
                let declarations: Vec<Ident> =
                    state.to_declare.last_mut().unwrap().drain(..).collect();
                let to_insert_above: Vec<TokenStream> = state.to_insert_above.drain(..).collect();
                let declaration_strings = declarations.iter().map(|el| el.to_string());
                Ok(quote!(
                #(let #declarations =
                    Operand::Local(#declaration_strings.to_owned());)*
                #ret.extend([
                    #(#to_insert_above,)*
                    #(#ext,)*
                ])
                ))
            }
        };
        ret
    }
}
