#![allow(unused_variables, non_snake_case)]

use windows_rust_counters::win::uses::*;
use windows_rust_counters::win::perf::winperf::*;
use windows_rust_counters::win::safe::parse::*;

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
        unsafe { parse_ptr_null_delimited_double_null_terminated(pContext) }
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




