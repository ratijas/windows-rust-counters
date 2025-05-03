use win_high::prelude::v2::*;

fn main() {
    match RegConnectRegistryW_Safe(PCWSTR::null(), HKEY_PERFORMANCE_DATA) {
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
