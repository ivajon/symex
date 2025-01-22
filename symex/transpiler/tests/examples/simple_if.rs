//! Defines a simple if statement example
use general_assembly::{condition::Condition, operand::Operand, operation::Operation};
use transpiler::pseudo;

fn main() {
    let a = Operand::Register("a".to_owned());
    let b = Operand::Register("b".to_owned());
    let c = Operand::Local("c".to_owned());
    let cond = false;
    let _ret = pseudo!([
        let d = a ^ b;
        // d = a + d;
        if(cond) {
            d = a | b;
        }

        c = d;
        Jump(c);
    ]);
}
