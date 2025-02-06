use std::marker::PhantomData;

use crate::{
    arch::Architecture,
    logging::NoLogger,
    memory::array_memory::BoolectorMemory,
    smt::smt_boolector::{Boolector, BoolectorExpr},
    Composition,
};

#[derive(Clone, Debug)]
pub struct BoolectorBacked<A: Architecture> {
    _a: PhantomData<A>,
}

impl<A: Architecture> Composition for BoolectorBacked<A> {
    type Architecture = A;
    type Logger = NoLogger;
    type Memory = BoolectorMemory<A>;
    type SMT = Boolector<A>;
    type SmtExpression = BoolectorExpr;
    type StateContainer = A;

    fn logger(&mut self) -> &mut Self::Logger {
        todo!()
    }
}
