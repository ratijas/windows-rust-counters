use std::time::Duration;

use win_high::perf::consume::*;
use win_high::prelude::v2::*;

const SUB_KEY_MORSE: &str = r"SYSTEM\CurrentControlSet\Services\Morse";
const VALUE_NAME_TICK_INTERVAL: &str = "TickIntervalMillis";

/// Similar to provider, but divides duration by 2.
pub fn get_tick_interval() -> Duration {
    let sub_key = U16CString::from_str(SUB_KEY_MORSE).unwrap();
    if let Ok(hkey) = RegOpenKeyEx_Safe(
        HKEY_LOCAL_MACHINE,
        PCWSTR(sub_key.as_ptr()),
        None,
        KEY_READ,
    ) {
        if let Ok(dword) = query_value_dword(
            *hkey,
            VALUE_NAME_TICK_INTERVAL,
        ) {
            // at least 10ms == at most 100 updates per second.
            let millis = dword.max(10);
            return Duration::from_millis((millis / 2) as _);
        }
    }
    Duration::from_millis(1250 / 2)
}

pub fn get_object_name_index() -> u32 {
    let all = get_counters_info(None, UseLocale::English).unwrap();
    all.map().iter().find_map(|(_, meta)| {
        if &meta.name_value == "Morse code" {
            Some(meta.name_index)
        } else {
            None
        }
    }).unwrap()
}
