use std::marker::PhantomData;

use super::logger::SimpleLogger;
use crate::{
    logging::NoLogger,
    manager::SymexArbiter,
    smt::bitwuzla::{expr::BitwuzlaExpr, Bitwuzla, BitwuzlaMemory},
    Composition,
    UserStateContainer,
};

pub type Symex = SymexArbiter<DefaultComposition>;
pub type SymexWithState<Data> = SymexArbiter<UserState<Data>>;

#[derive(Clone, Debug)]
/// Default configuration for a defined architecture.
pub struct DefaultComposition {}

impl Composition for DefaultComposition {
    type Logger = SimpleLogger;
    type Memory = BitwuzlaMemory;
    type SMT = Bitwuzla;
    type SmtExpression = BitwuzlaExpr;
    type StateContainer = ();

    fn logger<'a>() -> &'a mut Self::Logger {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct UserState<State: UserStateContainer> {
    state: PhantomData<State>,
}

impl<State: UserStateContainer> Composition for UserState<State> {
    type Logger = NoLogger;
    type Memory = BitwuzlaMemory;
    type SMT = Bitwuzla;
    type SmtExpression = BitwuzlaExpr;
    type StateContainer = State;

    fn logger<'a>() -> &'a mut Self::Logger {
        todo!()
    }
}
