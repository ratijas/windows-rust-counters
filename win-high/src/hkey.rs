#![allow(non_camel_case_types, non_snake_case)]

use std::rc::Rc;

use crate::prelude::v1::*;

pub fn RegConnectRegistryW_Safe(
    lpMachineName: LPCWSTR,
    hKey: HKEY,
) -> WinResult<HKey_Safe> {
    let mut phkResult: HKEY = null_mut();

    unsafe {
        let error_code = RegConnectRegistryW(
            lpMachineName,
            hKey,
            &mut phkResult as PHKEY,
        ) as DWORD;

        if error_code != ERROR_SUCCESS {
            return Err(WinError::new_with_message(error_code));
        }
    }
    assert!(!phkResult.is_null());
    Ok(HKey_Safe::owned(phkResult))
}

pub fn RegOpenKeyEx_Safe(
    hKey: HKEY,
    lpSubKey: LPCWSTR,
    ulOptions: DWORD,
    samDesired: REGSAM,
) -> WinResult<HKey_Safe> {
    let mut phkResult: HKEY = null_mut();
    unsafe {
        let error_code = RegOpenKeyExW(
            hKey,
            lpSubKey,
            ulOptions,
            samDesired,
            &mut phkResult as PHKEY,
        ) as DWORD;

        if error_code != ERROR_SUCCESS {
            return Err(WinError::new_with_message(error_code));
        }
    }

    assert!(!phkResult.is_null());
    Ok(HKey_Safe::owned(phkResult))
}

pub fn RegCloseKey_Safe(
    hKey: HKEY,
) -> WinResult<()> {
    unsafe {
        let error_code = RegCloseKey(
            hKey
        ) as DWORD;

        if error_code != ERROR_SUCCESS {
            return Err(WinError::new_with_message(error_code));
        }
    }
    Ok(())
}

/// Memory-safe auto-closing wrapper for HKEY. To access underlying raw HKEY value from `HKey_Safe`
/// value, use deref operator: `*hkey`; and for `&HKey_Safe` use double deref: `**hkey_ref`.
#[derive(Debug)]
pub enum HKey_Safe {
    Owned(Rc<HKEY>),
}

impl HKey_Safe {
    pub fn owned(hkey: HKEY) -> Self {
        HKey_Safe::Owned(Rc::new(hkey))
    }
}

impl Clone for HKey_Safe {
    fn clone(&self) -> Self {
        match self {
            Self::Owned(rc) => Self::Owned(Rc::clone(rc)),
        }
    }
}

impl std::ops::Deref for HKey_Safe {
    type Target = HKEY;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(hkey) => hkey
        }
    }
}

impl Drop for HKey_Safe {
    fn drop(&mut self) {
        match self {
            HKey_Safe::Owned(_) =>
                if let Err(e) = RegCloseKey_Safe(**self) {
                    println!("RegCloseKey Error: {}", e);
                }
        }
    }
}
