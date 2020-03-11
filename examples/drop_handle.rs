extern crate windows_rust_counters;

use windows_rust_counters::win::safe::hkey::*;
use windows_rust_counters::win::uses::*;

fn main() {
    match RegConnectRegistryW_Safe(
        null(),
        HKEY_PERFORMANCE_DATA,
    ) {
        Err(err) => {
            println!("RegConnectRegistryW Error: {}", err.with_message());
            return;
        }
        Ok(hkey) => {
            println!("HK Result: {:?}", hkey);
            // Will close automatically when hkey goes out of scope.
        }
    }
}
