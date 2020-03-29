use std::convert::TryInto;
use std::time::Duration;

use winapi::um::winnt::KEY_READ;

use win_high::perf::useful::*;
use win_high::prelude::v1::*;

use crate::strings_providers::*;

const SUB_KEY_MORSE: &str = r"SYSTEM\CurrentControlSet\Services\Morse";
const VALUE_NAME_CUSTOM_MESSAGE: &str = "CustomMessage";
const VALUE_NAME_NUM_INSTANCES: &str = "NumInstances";
const VALUE_NAME_TICK_INTERVAL: &str = "TickIntervalMillis";

pub fn get_number_of_instances() -> NumInstances {
    let sub_key = U16CString::from_str(SUB_KEY_MORSE).unwrap();
    if let Ok(hkey) = RegOpenKeyEx_Safe(
        HKEY_LOCAL_MACHINE,
        sub_key.as_ptr(),
        0,
        KEY_READ,
    ) {
        if let Ok(dword) = query_value_dword(
            *hkey,
            VALUE_NAME_NUM_INSTANCES,
        ) {
            let num = LONG::from_ne_bytes(dword.to_ne_bytes());
            if let Ok(value) = num.try_into() {
                return match value {
                    NumInstances::NoInstances => NumInstances::NoInstances,
                    NumInstances::N(n) => NumInstances::N(n.min(1024).max(1))
                };
            }
        }
    }
    return NumInstances::NoInstances;
}

pub fn get_tick_interval() -> Duration {
    let sub_key = U16CString::from_str(SUB_KEY_MORSE).unwrap();
    if let Ok(hkey) = RegOpenKeyEx_Safe(
        HKEY_LOCAL_MACHINE,
        sub_key.as_ptr(),
        0,
        KEY_READ,
    ) {
        if let Ok(dword) = query_value_dword(
            *hkey,
            VALUE_NAME_TICK_INTERVAL,
        ) {
            // at least 10ms == at most 100 updates per second.
            let millis = dword.max(10);
            return Duration::from_millis(millis as _);
        }
    }
    Duration::from_millis(1250)
}

pub fn get_reg_key_strings_provider() -> RegKeyStringsProvider {
    RegKeyStringsProvider::new(
        SUB_KEY_MORSE,
        VALUE_NAME_CUSTOM_MESSAGE,
    )
}
