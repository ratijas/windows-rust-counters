extern crate windows_rust_test;

use windows_rust_test::win::perf::consume::*;

fn main() {
    let table = match build_counters_table() {
        Ok(table) => table,
        Err(e) => {
            println!("Error while building counters table: {}", e);
            return;
        }
    };

    for counter in table.map().values() {
        println!("[{}]: Name = {}", counter.name_index, counter.name_value);
        println!("[{}]: Help = {}", counter.help_index, counter.help_value);
        println!();
    }
}
