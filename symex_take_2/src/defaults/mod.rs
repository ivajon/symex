use std::marker::PhantomData;

use crate::{
    arch::Architecture,
    executor::hooks::StateContainer,
    logging::NoLogger,
    memory::array_memory::BoolectorMemory,
    smt::smt_boolector::{Boolector, BoolectorExpr},
    Composition,
};

#[derive(Clone, Debug)]
pub struct BoolectorBacked<A: Architecture + ?Sized> {
    _a: PhantomData<A>,
}

impl<A: Architecture> Composition for BoolectorBacked<A>
where
    Box<A>: Clone,
    A: Clone,
{
    type Architecture = A;
    type Logger = NoLogger;
    type Memory = BoolectorMemory;
    type SMT = Boolector;
    type SmtExpression = BoolectorExpr;
    type StateContainer = Box<A>;

    fn logger(&mut self) -> &mut Self::Logger {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct DynamicBoolectorBacked;

impl Composition for DynamicBoolectorBacked {
    type Architecture = dyn Architecture;
    type Logger = NoLogger;
    type Memory = BoolectorMemory;
    type SMT = Boolector;
    type SmtExpression = BoolectorExpr;
    type StateContainer = Box<Self::Architecture>;

    fn logger(&mut self) -> &mut Self::Logger {
        todo!()
    }
}

impl<A: Architecture + ?Sized> StateContainer for Box<A> {
    type Architecture = A;

    fn as_arch(&mut self) -> &mut Self::Architecture {
        self
    }
}
