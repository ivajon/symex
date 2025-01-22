//! Defines a transpiler that allows inline pseudo code
//! to be translated in to [`general_assembly`]
extern crate proc_macro;

use language::ast::IR;
use proc_macro::TokenStream;
use syn::parse_macro_input;

#[proc_macro]
/// Extends or creates a vector of [`general_assembly`] operations.
///
/// Usage:
/// ```
/// use general_assembly::{operation::Operation,operand::Operand,condition::Condition};
/// use transpiler::pseudo;
///
/// let a = Operand::Register("a".to_owned());
/// let b = Operand::Register("b".to_owned());
/// let c = Operand::Local("c".to_owned());
/// let cond = false;
/// let ret = pseudo!([
///     c = a+b;
///     let d = a ^ b;
///     
///     if(cond) {
///         d = a | b;
///     }
///     
///     c = d;
///     Jump(c);
/// ]);
/// ```
pub fn pseudo(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as IR);
    let input: proc_macro2::TokenStream = match input.into() {
        Ok(val) => val,
        Err(e) => panic!("{:?}", e),
    };

    input.into()
}
