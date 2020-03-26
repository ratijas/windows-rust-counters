use std::collections::BTreeMap;
use std::mem;

use itertools::Itertools;
use winapi::um::sysinfoapi::*;
use winapi::um::winnls::*;

use crate::prelude::v1::*;
use winapi::um::winnt::{OSVERSIONINFOW};
use std::str::FromStr;

/// Translated function `GetLanguageId` from [MSDN].
///
/// [MSDN]: https://docs.microsoft.com/en-us/windows/win32/perfctrs/retrieving-counter-names-and-explanations
pub fn get_language_id() -> WinResult<LANGID> {
    // From the docs:
    // > Before calling the GetVersionEx function, set the dwOSVersionInfoSize member of the
    // > structure as appropriate to indicate which data structure is being passed to this function.
    let mut osvi: OSVERSIONINFOW = unsafe { mem::zeroed() };
    osvi.dwOSVersionInfoSize = mem::size_of::<OSVERSIONINFOW>() as DWORD;

    // SAFETY: dwOSVersionInfoSize member is set
    if unsafe { GetVersionExW(&mut osvi as *mut _) } == 0 {
        return Err(WinError::get_with_message().with_comment("GetVersionExW failed"));
    }

    // Complete language identifier.
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    // Primary language identifier.
    let primary = PRIMARYLANGID(lang_id);

    Ok(if (LANG_PORTUGUESE == primary && osvi.dwBuildNumber > 5) || // Windows Vista and later
        (LANG_CHINESE == primary && (osvi.dwMajorVersion == 5 && osvi.dwMinorVersion >= 1)) // XP and Windows Server 2003
    {
        // Use the complete language identifier.
        lang_id
    } else {
        // Use only primary
        MAKELANGID(primary, 0)
    })
}

#[derive(Clone, Debug)]
pub struct CounterMeta {
    // Name index is always even.
    pub name_index: DWORD,
    pub name_value: String,
    // Help index is always odd, and equals name index + 1.
    pub help_index: DWORD,
    pub help_value: String,
}

pub struct AllCounters {
    // Key is the name index
    table: BTreeMap<DWORD, CounterMeta>
}

impl AllCounters {
    pub fn new() -> AllCounters {
        AllCounters {
            table: BTreeMap::new(),
        }
    }

    pub fn get(&self, name_index: DWORD) -> Option<&CounterMeta> {
        self.table.get(&name_index)
    }

    pub fn entry(&mut self, name_index: DWORD) -> &mut CounterMeta {
        let default = CounterMeta {
            name_index,
            name_value: String::new(),
            help_index: name_index + 1,
            help_value: String::new(),
        };
        self.table.entry(name_index).or_insert(default)
    }

    pub fn map(&self) -> &BTreeMap<DWORD, CounterMeta> {
        &self.table
    }
}

/// Determines which localization strategy will be used to retrieve text data.
#[derive(Clone, Copy, Debug)]
pub enum UseLocale {
    /// `HKEY_PERFORMANCE_TEXT` will be used to get english strings.
    English,

    /// `HKEY_PERFORMANCE_NLSTEXT` will be used to get strings based on the default UI language of
    /// the current user.
    UIDefault,

    /// `HKEY_PERFORMANCE_DATA` will be used to get strings based on the language identifier.
    LangId(LANGID),
}

impl Default for UseLocale {
    fn default() -> Self {
        Self::UIDefault
    }
}

impl UseLocale {
    pub fn raw_hkey(&self) -> HKEY {
        match self {
            UseLocale::English => HKEY_PERFORMANCE_TEXT,
            UseLocale::UIDefault => HKEY_PERFORMANCE_NLSTEXT,
            UseLocale::LangId(_) => HKEY_PERFORMANCE_DATA,
        }
    }

    pub fn format_query(&self, query: &str) -> String {
        match self {
            UseLocale::LangId(lang_id) =>
            // LANGID must formatted as hex value, 3 chars wide, padded with 0 on the left.
                format!("{} {:03x}", query, lang_id),
            _ => query.to_owned(),
        }
    }
}

/// Get names and help text of all registered performance counters.
pub fn get_counters_info(machine: Option<String>, locale: UseLocale) -> WinResult<AllCounters> {
    let mut all = AllCounters::new();

    let wsz_machine_name = machine.map(|s| U16CString::from_str(s).unwrap());
    let lp_machine_name = match wsz_machine_name.as_ref() {
        Some(string_w) => string_w.as_ptr(),
        None => null(),
    };

    let text_hkey = RegConnectRegistryW_Safe(lp_machine_name, locale.raw_hkey())?;
    let query = locale.format_query("Counter");
    let counters_raw = query_value(*text_hkey, query.as_str(), None, None)?;

    // save buffer size to use later as an optimization opportunity for similar call
    let buffer_size = counters_raw.len();
    let pairs = parse_performance_text_pairs(counters_raw.as_ref());
    for (index, value) in pairs {
        let counter = all.entry(index);
        counter.name_index = index;
        counter.name_value = value;
    }

    // re-use buffer
    let mut help_raw = counters_raw;

    // length of Help text is supposedly much longer than the names.
    let buffer_size_hint = Some(4 * buffer_size);
    let query = locale.format_query("Help");
    query_value_with_buffer(*text_hkey, query.as_str(), None, buffer_size_hint, &mut help_raw)?;

    let pairs = parse_performance_text_pairs(help_raw.as_ref());
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

fn parse_performance_text_pairs(raw: &[u8]) -> Vec<(DWORD, String)> {
    let raw_u16str = u8_as_u16_str(raw.as_ref());
    let pairs = parse_null_separated_key_value_pairs(raw_u16str);
    pairs
}

fn parse_null_separated_key_value_pairs<T: FromStr>(raw: &U16Str) -> Vec<(T, String)> {
    let mut vec = Vec::new();

    for (index, value)
    in crate::format::split_nul_delimited_double_nul_terminated(raw)
        .tuples::<(&U16CStr, &U16CStr)>()
        .skip(1) // drop useless header with total count
    {
        let index_parsed = match index
            .to_string()
            .map_err(drop)
            .and_then(|it| it.parse::<T>().map_err(drop))
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
                println!("Error parsing UTF-16 value for index {}", index.to_string_lossy());
                continue;
            }
        };

        vec.push((index_parsed, value));
    }

    vec
}
