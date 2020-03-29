use std::error::Error;

use log::{error, info};
use winapi::um::winnt::KEY_READ;

use win_high::prelude::v1::*;

pub trait StringsProvider {
    fn provide(&mut self) -> String;
}

pub struct ConstString {
    string: String,
}

impl ConstString {
    pub fn new(s: &str) -> Self {
        Self { string: s.to_string() }
    }
}

impl StringsProvider for ConstString {
    fn provide(&mut self) -> String {
        self.string.clone()
    }
}

pub struct RegKeyStringsProvider {
    sub_key: U16CString,
    value_name: String,
}

impl RegKeyStringsProvider {
    pub fn new(sub_key: &str, value_name: &str) -> Self {
        Self {
            sub_key: U16CString::from_str(sub_key).unwrap(),
            value_name: value_name.to_string(),
        }
    }

    fn fetch(&mut self) -> WinResult<String> {
        let hkey = RegOpenKeyEx_Safe(
            HKEY_LOCAL_MACHINE,
            self.sub_key.as_ptr(),
            0,
            KEY_READ,
        )?;
        let buffer = query_value(
            *hkey,
            &self.value_name,
            None,
            None,
        )?;
        Ok(unsafe { U16CStr::from_ptr_str(buffer.as_ptr() as *const _) }.to_string_lossy())
    }
}

impl StringsProvider for RegKeyStringsProvider {
    fn provide(&mut self) -> String {
        self.fetch().unwrap()
    }
}

pub struct RandomJokeProvider;

impl RandomJokeProvider {
    pub fn new() -> Self {
        RandomJokeProvider
    }

    pub fn fetch(&self) -> Result<String, Box<dyn Error>> {
        const URL: &str = "http://api.icndb.com/jokes/random?limitTo=nerdy";

        let resp: serde_json::Value = reqwest::blocking::get(URL)?
            .json()?;
        let joke = Self::get_in(&resp).ok_or("invalid json")?;
        let joke = xml::unescape(&joke).unwrap_or(joke.to_string());
        Ok(joke)
    }

    fn get_in(json: &serde_json::Value) -> Option<&str> {
        json.get("value")?
            .get("joke")?
            .as_str()
    }
}

impl StringsProvider for RandomJokeProvider {
    fn provide(&mut self) -> String {
        match self.fetch() {
            Ok(string) => {
                info!("MOTD joke: {}", string);
                string
            }
            Err(why) => {
                error!("MOTD error: {}", why);
                "".to_string()
            }
        }
    }
}
