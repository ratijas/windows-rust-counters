extern crate windows_rust_counters;

use windows_rust_counters::win::uses::*;

fn main() {
    let buf = do_get_values().expect("Get values");

    for (i, byte) in buf.into_iter().enumerate() {
        print!("{:02X} ", byte);
        // line break before next row
        if (i + 1) % 16 == 0 {
            println!();
        }
    }
    println!();
}

fn do_get_values() -> WinResult<Vec<u8>> {
    let hkey = RegConnectRegistryW_Safe(null(), HKEY_PERFORMANCE_DATA)?;

    query_value(
        *hkey,
        "Global",
        Some(2_000_000)
    )
}
