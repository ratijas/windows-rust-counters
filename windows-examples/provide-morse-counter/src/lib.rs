#![allow(unused_variables, non_snake_case)]

use win_high::prelude::v1::*;
use win_high::format::*;
use win_low::winperf::*;
use winapi::um::winuser::{MessageBoxW, MB_OK};

// type assertions
const _: PM_OPEN_PROC = MyOpenProc;
const _: PM_COLLECT_PROC = MyCollectProc;
const _: PM_CLOSE_PROC = MyCloseProc;

#[no_mangle]
extern "system" fn MyOpenProc(pContext: LPWSTR) -> DWORD {
    let ctx = if pContext.is_null() {
        vec![]
    } else {
        // SAFETY: we have to trust the system
        unsafe { split_nul_delimited_double_nul_terminated_ptr(pContext) }.collect()
    };
    let caption = U16CString::from_str("Open from counter.dll").unwrap();
    let message = format!("Context is: {:?}", ctx);
    let message = "Context is unknown";
    let text = U16CString::from_str(&message).unwrap();
    unsafe {
        let _ = MessageBoxW(
            null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK,
        );
    }
    ERROR_SUCCESS
}

#[no_mangle]
extern "system" fn MyCollectProc(
    lpValueName: LPWSTR,
    lppData: *mut LPVOID,
    lpcbTotalBytes: LPDWORD,
    lpNumObjectTypes: LPDWORD,
) -> DWORD {
    unsafe {
        let caption = U16CString::from_str("Collect from counter.dll").unwrap();
        let value_name = U16CStr::from_ptr_str(lpValueName).to_string_lossy();
        let message = format!("Requested value: {}", value_name);
        let text = U16CString::from_str(&message).unwrap();
        let _ = MessageBoxW(
            null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK,
        );

        lpcbTotalBytes.write(0);
        lpNumObjectTypes.write(0);
    }
    ERROR_SUCCESS
}

#[no_mangle]
extern "system" fn MyCloseProc() -> DWORD {
    let caption = U16CString::from_str("Close from counter.dll").unwrap();
    let text = U16CString::from_str("Close").unwrap();
    unsafe {
        let _ = MessageBoxW(
            null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK,
        );
    }
    ERROR_SUCCESS
}
