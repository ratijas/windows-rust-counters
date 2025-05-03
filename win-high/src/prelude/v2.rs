#![allow(unused_imports)]

pub use std::ptr::{self, null, null_mut};

pub use widestring::{U16CStr, U16CString, U16Str, U16String, u16cstr, u16str};

pub use windows_core::w;
pub use windows_core::{PCWSTR, PWSTR};

pub use windows::Win32::Foundation::*;
pub use windows::Win32::System::Diagnostics::Debug::*;
pub use windows::Win32::System::Performance::*;
pub use windows::Win32::System::Registry::*;
pub use windows::Win32::System::SystemInformation::*;
pub use windows::Win32::System::SystemServices::*;

pub use crate::error::*;
pub use crate::hkey::*;
pub use crate::reg::*;
