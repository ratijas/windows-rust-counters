#![allow(unused_imports)]
#![allow(ambiguous_glob_reexports)]

//! Everything you need to get satrted with WinAPI.
pub use std::ptr::{self, null, null_mut};

pub use widestring::*;
pub use winapi::shared::minwindef::*;
pub use winapi::shared::ntdef::*;
pub use winapi::shared::winerror::*;
pub use winapi::um::errhandlingapi::*;
pub use winapi::um::winbase::*;
pub use winapi::um::winreg::*;

pub use crate::error::*;
pub use crate::hkey::*;
pub use crate::reg::*;
