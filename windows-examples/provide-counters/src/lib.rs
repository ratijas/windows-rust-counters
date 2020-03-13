#![crate_type = "cdylib"]
#![allow(unused_variables, non_snake_case)]

use win_high::prelude::v1::*;
use win_high::format::*;
use win_low::winperf::*;

#[no_mangle]
static MyOpenProc: PM_OPEN_PROC = open_proc;
#[no_mangle]
static MyCollectProc: PM_COLLECT_PROC = collect_proc;
#[no_mangle]
static MyCloseProc: PM_CLOSE_PROC = close_proc;

extern "system" fn open_proc(pContext: LPWSTR) -> DWORD {
    let ctx = if pContext.is_null() {
        vec![]
    } else {
        // SAFETY: we have to trust the system
        unsafe { split_nul_delimited_double_nul_terminated_ptr(pContext) }.collect()
    };
    println!("Hello, context is: {:?}", ctx);
    ERROR_SUCCESS
}

extern "system" fn collect_proc(
    lpValueName: LPWSTR,
    lppData: *mut LPVOID,
    lpcbTotalBytes: LPDWORD,
    lpNumObjectTypes: LPDWORD,
) -> DWORD {
    unsafe {
        lpcbTotalBytes.write(0);
        lpNumObjectTypes.write(0);
    }
    ERROR_SUCCESS
}

extern "system" fn close_proc() -> DWORD {
    println!("Goodbye");
    ERROR_SUCCESS
}




