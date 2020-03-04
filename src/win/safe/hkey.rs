use crate::win::uses::*;

pub fn RegConnectRegistryW_Safe(
    lpMachineName: LPCWSTR,
    hKey: HKEY,
) -> WinResult<HKeyWrapper> {
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
    assert_ne!(phkResult, null_mut());
    Ok(HKeyWrapper(phkResult))
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

#[derive(Debug)]
pub struct HKeyWrapper(HKEY);

impl Drop for HKeyWrapper {
    fn drop(&mut self) {
        if let Err(e) = RegCloseKey_Safe(self.0) {
            println!("RegCloseKey Error: {}", e);
        }
    }
}
