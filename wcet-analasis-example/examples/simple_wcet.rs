use symex::run_elf::run_elf;

fn main() {
    println!("Simple WCET analasis");

    let path_to_elf_file = "target/thumbv6m-none-eabi/release/examples/rtic_simple_resourse";
    let function_name = "IO_IRQ_BANK0";

    let results = run_elf(path_to_elf_file, function_name, false).unwrap();

    let mut max = 0;
    let paths = results.len();
    for result in results {
        max = max.max(result.max_cycles);
    }

    println!(
        "Found {} paths and the longest path takes {} cycles.",
        paths, max
    );
}
