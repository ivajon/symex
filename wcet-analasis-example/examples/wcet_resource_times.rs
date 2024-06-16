use symex::{
    general_assembly::{
        arch::arm::v6::ArmV6M, project::MemoryHookAddress, state::GAState, Result, RunConfig,
    },
    run_elf::{run_elf, run_elf_configured},
    smt::DExpr,
};

use srp::common::Trace;

// This example show how hooks can be used to get at which cycle a resource is locked and unlocked in a simple
// RTIC application. To keep in mind is that cycles are added after the instruction is executed and the hook
// is run during instruction execution. Therefore care needs to be taken to measure the critical section
// correctly.

// To run the example first build the "rtic_simple_resourse" in armv6-m-examples by doing:
// cd armv6-m-examples
// cargo build --release --example rtic_simple_resourse
// cd ..
//
// Then run the analysis by: cargo run -p wcet-analasis-examples --release --example wcet_resource_times

fn make_trace(start: usize, end: usize, laps: &[(usize, String)], id: String) -> Trace {
    let mut inner = vec![];

    let mut current = "";
    let mut inner_start = 0;
    let mut in_inner = false;
    let mut start_i = 0;
    for i in 0..laps.len() {
        if !in_inner {
            current = &laps[i].1;
            inner_start = laps[i].0;
            start_i = i;
            in_inner = true
        } else {
            if current == &laps[i].1 {
                inner.push(make_trace(
                    inner_start,
                    laps[i].0,
                    &laps[(start_i + 1)..i],
                    laps[i].1.to_owned(),
                ));
                in_inner = false;
            }
        }
    }

    Trace {
        id,
        start: start as u32,
        end: end as u32,
        inner,
    }
}

fn main() {
    println!("Simple WCET analasis");

    // path to the elf file to analyse.
    let path_to_elf_file =
        "armv6-m-examples/target/thumbv6m-none-eabi/release/examples/rtic_simple_resourse";
    // name of the task in the elf file (same as associated interrupt vector for HW tasks).
    let function_name = "IO_IRQ_BANK0";

    // Hook to run when the interrupt mask is reset (looked).
    let lock_hook: fn(
        state: &mut GAState<ArmV6M>,
        addr: u64,
        value: DExpr,
        bits: u32,
    ) -> Result<()> = |state, _addr, value, _bits| {
        // save the current cycle count to the laps vector.
        let val = value.get_constant().unwrap().to_string();
        state.cycle_laps.push((state.cycle_count, val));
        Ok(())
    };

    // Hook to run when the interrupt mask is set (unlocked).
    let unlock_hook: fn(
        state: &mut GAState<ArmV6M>,
        addr: u64,
        value: DExpr,
        bits: u32,
    ) -> Result<()> = |state, _addr, value, _bits| {
        // save the current cycle count to the laps vector.
        let val = value.get_constant().unwrap().to_string();
        let current_instruction_cycle_count =
            match state.current_instruction.as_ref().unwrap().max_cycle {
                symex::general_assembly::instruction::CycleCount::Value(v) => v,
                symex::general_assembly::instruction::CycleCount::Function(f) => f(state),
            };

        // add the current instruction to the cycle count to compensate for cycles added after instruction completed
        let cycle_count = state.cycle_count + current_instruction_cycle_count;
        state.cycle_laps.push((cycle_count, val));
        Ok(())
    };

    // create a run configuration with the hooks associated with the correct addresses.
    let config = RunConfig {
        pc_hooks: vec![],
        register_read_hooks: vec![],
        register_write_hooks: vec![],
        memory_write_hooks: vec![
            (MemoryHookAddress::Single(0xe000e100), unlock_hook),
            (MemoryHookAddress::Single(0xe000e180), lock_hook),
        ],
        memory_read_hooks: vec![],
        show_path_results: false,
    };

    // run the symbolic execution
    let results = run_elf_configured(path_to_elf_file, function_name, ArmV6M {}, config).unwrap();

    // Find the longest path and print out the saved cycle counts for lock and unlock.
    let mut max = 0;
    let paths = results.len();
    for result in results {
        //println!("cycle laps: {:?}", result.cycle_laps);
        max = max.max(result.max_cycles);
        let trace = make_trace(
            0,
            result.max_cycles,
            &result.cycle_laps,
            function_name.to_owned(),
        );
        println!("trace: {:#?}", trace);
    }

    println!(
        "Found {} paths and the longest path takes {} cycles.",
        paths, max
    );
}
