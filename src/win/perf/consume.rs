use std::collections::BTreeMap;
use std::thread::sleep;

use itertools::Itertools;
use libc::wcslen;

use crate::win::safe::hkey::*;
use crate::win::uses::*;

/// Use the HKEY_PERFORMANCE_NLSTEXT key to get the strings.
fn query_value(localized: bool, value_name: &str) -> WinResult<Vec<u8>> {
    let value_name_w = U16String::from_str(value_name);
    let key = if localized { HKEY_PERFORMANCE_NLSTEXT } else { HKEY_PERFORMANCE_TEXT };

    unsafe {
        let mut buffer_size: DWORD = 0;

        // Query the size of the text data so you can allocate the buffer.
        let error_code = RegQueryValueExW(
            key, // hKey
            value_name_w.as_ptr(), // lpValueName
            null_mut(), // lpReserved
            null_mut(), // lpType
            null_mut(), // lpData
            &mut buffer_size as LPDWORD, // lpcbData
        ) as DWORD;
        if error_code != ERROR_SUCCESS {
            return Err(WinError::new_with_message(error_code));
        }

        let mut buffer: Vec<u8> = Vec::with_capacity(buffer_size as usize);

        let error_code = RegQueryValueExW(
            key, // hKey
            value_name_w.as_ptr(), // lpValueName
            null_mut(), // lpReserved
            null_mut(), // lpType
            buffer.as_mut_ptr() as LPBYTE, // lpData
            &mut buffer_size as LPDWORD, // lpcbData
        ) as DWORD;
        if error_code != ERROR_SUCCESS {
            return Err(WinError::new_with_message(error_code));
        }

        buffer.set_len(buffer_size as usize);
        Ok(buffer)
    }
}

#[derive(Clone, Debug)]
pub struct CounterMeta {
    // Name index is always even.
    pub name_index: usize,
    pub name_value: String,
    // Help index is always odd, and equals name index + 1.
    pub help_index: usize,
    pub help_value: String,
}

pub struct AllCounters {
    // Key is the name index
    table: BTreeMap<usize, CounterMeta>
}

impl AllCounters {
    pub fn new() -> AllCounters {
        AllCounters {
            table: BTreeMap::new(),
        }
    }

    pub fn get(&self, name_index: usize) -> Option<&CounterMeta> {
        self.table.get(&name_index)
    }

    pub fn entry(&mut self, name_index: usize) -> &mut CounterMeta {
        let default = CounterMeta {
            name_index,
            name_value: String::new(),
            help_index: name_index + 1,
            help_value: String::new(),
        };
        self.table.entry(name_index).or_insert(default)
    }

    pub fn map(&self) -> &BTreeMap<usize, CounterMeta> {
        &self.table
    }
}

pub fn build_counters_table() -> WinResult<AllCounters> {
    let mut all = AllCounters::new();

    let counters_raw = query_value(true,  "Counter")?;

    let pairs = query_pairs(counters_raw.as_ref());
    for (index, value) in pairs {
        let counter = all.entry(index);
        counter.name_index = index;
        counter.name_value = value;
    }
    // free memory
    drop(counters_raw);

    let help_raw = query_value(true, "Help")?;

    let pairs = query_pairs(help_raw.as_ref());
    for (index, value) in pairs {
        // help text index is bigger than entry index by 1
        let counter = all.entry(index - 1);
        counter.help_index = index;
        counter.help_value = value;
    }

    Ok(all)
}

fn u8_as_u16_str(source: &[u8]) -> &U16Str {
    assert_eq!(source.len() % 2, 0);
    unsafe {
        let raw_ptr = source.as_ptr() as *const u16;
        let raw_len = source.len() / 2;
        U16Str::from_ptr(raw_ptr, raw_len)
    }
}

fn query_pairs(raw: &[u8]) -> Vec<(usize, String)> {
    let raw_u16str = u8_as_u16_str(raw.as_ref());
    let pairs = parse_null_separated_key_value_pairs(raw_u16str.as_slice());
    pairs
}

fn parse_null_separated_key_value_pairs(raw: &[u16]) -> Vec<(usize, String)> {
    // string is double-null terminated
    assert_eq!(&raw[raw.len() - 2..], &[0, 0]);
    // strip extra nulls
    let raw = &raw[..raw.len() - 2];

    let mut vec = Vec::new();

    for (index, value)
    in raw
        .split(|&wchar| wchar == 0)
        .map(U16Str::from_slice)
        .tuples::<(&U16Str, &U16Str)>()
        .skip(1) // drop useless header with total count
    {
        let index = match index
            .to_string()
            .map_err(drop)
            .and_then(|it| it.parse::<usize>().map_err(drop))
        {
            Ok(index) => index,
            Err(_) => {
                println!("Error parsing index {} for value {}", index.to_string_lossy(), value.to_string_lossy());
                continue;
            }
        };
        let value = match value.to_string() {
            Ok(value) => value,
            Err(_) => {
                println!("Error parsing UTF-16 value for index {}", index);
                continue;
            }
        };

        vec.push((index, value));
    }

    vec
}
