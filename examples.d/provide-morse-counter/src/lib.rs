#![allow(unused_variables, non_snake_case)]

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use log::{error, info};

use lazy_static::lazy_static;
use win_high::format::*;
use win_high::perf::provide::*;
use win_high::perf::useful::*;
use win_high::prelude::v2::*;

use crate::app::*;
use crate::morse::*;

mod app;
mod morse;
mod reg;
mod strings_providers;
mod worker;

#[allow(dead_code)]
mod symbols {
    include!(concat!(env!("OUT_DIR"), "/symbols.rs"));
}

// type assertions
const _: PM_OPEN_PROC = Some(MyOpenProc);
const _: PM_COLLECT_PROC = Some(MyCollectProc);
const _: PM_CLOSE_PROC = Some(MyCloseProc);

#[unsafe(no_mangle)]
extern "system" fn MyOpenProc(pContext: PCWSTR) -> u32 {
    eventlog::init("Morse", log::Level::Info).unwrap();

    let ctx = if pContext.is_null() {
        vec![]
    } else {
        // SAFETY: we have to trust the system
        unsafe { split_nul_delimited_double_nul_terminated_ptr(pContext.0) }.collect()
    };
    info!("Received request to open with context: {:?}", ctx);

    match start_global_workers() {
        Ok(_) => info!("Started global worker"),
        Err(_) => {
            error!("Could not start global worker");
            return ERROR_ACCESS_DENIED.0;
        }
    }

    ERROR_SUCCESS.0
}

#[unsafe(no_mangle)]
extern "system" fn MyCollectProc(
    lpValueName: PCWSTR,
    lppData: *mut *mut core::ffi::c_void,
    lpcbTotalBytes: *mut u32,
    lpNumObjectTypes: *mut u32,
) -> u32 {
    // panics across FFI boundary is UB, hence must handle any errors here
    match collect(lpValueName, lppData, lpcbTotalBytes, lpNumObjectTypes) {
        Ok(_) => ERROR_SUCCESS.0,
        Err(error) => {
            if error.error_code() != ERROR_MORE_DATA {
                error!("Error while collecting data: {:?}", error.with_message());
            }
            unsafe {
                lpcbTotalBytes.write(0);
                lpNumObjectTypes.write(0);
            }
            error.error_code().0
        }
    }
}

fn collect(
    lpValueName: PCWSTR,
    lppData: *mut *mut core::ffi::c_void,
    lpcbTotalBytes: *mut u32,
    lpNumObjectTypes: *mut u32,
) -> WinResult<()> {
    unsafe {
        let query = U16CStr::from_ptr_str(lpValueName.0).to_string_lossy();
        let query_type =
            QueryType::from_str(&query).map_err(|_| WinError::new(ERROR_BAD_ARGUMENTS))?;

        info!("query is: {:?}", query_type);

        let buffer = std::slice::from_raw_parts_mut(
            lppData.cast::<*mut u8>().read() as *mut u8,
            lpcbTotalBytes.read() as usize,
        );

        let mut p = PROVIDER
            .lock()
            .map_err(|_| WinError::new(ERROR_LOCK_FAILED))?;
        let collected: Collected = p.collect(query_type, buffer)?;

        lppData
            .cast::<*mut u8>()
            .write(buffer.as_mut_ptr().add(collected.total_bytes));
        lpcbTotalBytes.write(collected.total_bytes as _);
        lpNumObjectTypes.write(collected.num_object_types as _);
    }
    Ok(())
}

#[unsafe(no_mangle)]
extern "system" fn MyCloseProc() -> u32 {
    info!("Received request to close");

    let _ = stop_global_workers().ok();
    info!("Stopped global worker");

    ERROR_SUCCESS.0
}

lazy_static! {
    pub static ref DATA: SharedObjectData = SharedObjectData::new();
    pub static ref APP: Arc<Mutex<App>> = Arc::new(Mutex::new(App::new(DATA.clone())));
    pub static ref PROVIDER: Mutex<MorseCountersProvider> =
        Mutex::new(MorseCountersProvider::new(APP.clone(), DATA.clone()));
}

fn start_global_workers() -> Result<(), ()> {
    APP.lock().map_err(drop)?.start();
    Ok(())
}

fn stop_global_workers() -> Result<(), ()> {
    APP.lock().map_err(drop)?.stop();
    Ok(())
}
