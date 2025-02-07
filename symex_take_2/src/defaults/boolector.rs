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
/// Default configuration for a defined architecture.
pub struct DefaultComposition<A: Architecture + ?Sized> {
    _a: PhantomData<A>,
}

impl<A: Architecture + StateContainer> Composition for DefaultComposition<A>
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

impl<A: Architecture + ?Sized> StateContainer for Box<A>
where
    Box<A>: Clone,
{
    type Architecture = A;

    fn as_arch(&mut self) -> &mut Self::Architecture {
        self
    }
}

#[derive(Clone, Debug)]
pub struct UserStateDynamicArch<State: StateContainer<Architecture = dyn Architecture>> {
    state: PhantomData<State>,
}

impl<State: StateContainer<Architecture = dyn Architecture>> Composition
    for UserStateDynamicArch<State>
{
    type Architecture = State::Architecture;
    type Logger = NoLogger;
    type Memory = BoolectorMemory;
    type SMT = Boolector;
    type SmtExpression = BoolectorExpr;
    type StateContainer = State;

    fn logger(&mut self) -> &mut Self::Logger {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct UserState<A: Architecture, State: StateContainer<Architecture = A>> {
    state: PhantomData<State>,
}

impl<A: Architecture + Clone, State: StateContainer<Architecture = A> + Clone> Composition
    for UserState<A, State>
{
    type Architecture = A;
    type Logger = NoLogger;
    type Memory = BoolectorMemory;
    type SMT = Boolector;
    type SmtExpression = BoolectorExpr;
    type StateContainer = State;

    fn logger(&mut self) -> &mut Self::Logger {
        todo!()
    }
}
