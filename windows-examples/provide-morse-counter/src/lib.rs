#![allow(unused_variables, non_snake_case)]

#[macro_use]
extern crate lazy_static;

use std::error::Error;
use std::iter;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use winapi::um::winuser::{MB_OK, MessageBoxW};

use morse_stream::*;
use signal_flow::*;
use signal_flow::rtsm::*;
use win_high::format::*;
use win_high::prelude::v1::*;
use win_low::winperf::*;

use crate::worker::WorkerThread;

mod worker;

#[allow(dead_code)]
mod symbols {
    include!(concat!(env!("OUT_DIR"), "/symbols.rs"));
}

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

    start_global_worker();

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

    // TODO

    // *CURRENT_SIGNAL.lock().unwrap()

    ERROR_SUCCESS
}

fn fill() {
    if (*lpcbBytes < type.TotalByteLength) {
// Не влезаем
        *lpcbBytes = 0;
        *lpcObjectTypes = 0;
        return ERROR_MORE_DATA;
    }

    char *temp = (char *) (*lppData);

// Копируем описание объекта
    memcpy(temp, &type, sizeof(type));
    temp += type.HeaderLength;

// Копируем описание первого счётчика
    memcpy(temp, &counter1, sizeof(counter1));
    temp += counter1.ByteLength;

// Копируем описание второго счётчика
    memcpy(temp, &counter2, sizeof(counter2));
    temp += counter2.ByteLength;

// Копируем заголовок блока данных
    memcpy(temp, &bl, sizeof(bl));

// Копируем данные первого счётчика
    memcpy(temp + counter1.CounterOffset, hw, (lstrlenW(hw) + 1) * sizeof(WCHAR));

    DWORD v = rand() % 10;

// Копируем данные второго счётчика
    memcpy(temp + counter2.CounterOffset, &v, sizeof(DWORD));

// Устанавливаем выходные параметры
    *lppData = (*lppData) + typ.TotalByteLength;
    *lpcbBytes = typ.TotalByteLength;
    *lpcObjectTypes = 1;

    return ERROR_SUCCESS;

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

    stop_global_worker();

    ERROR_SUCCESS
}

lazy_static! {
    static ref WORKER: Mutex<Option<WorkerThread<()>>> = Mutex::new(None);
    static ref CURRENT_SIGNAL: Mutex<u32> = Mutex::new(0);
}

fn start_global_worker() {
    let mutex = &*WORKER;
    let mut lock = mutex.lock().unwrap();
    let opt = &mut *lock;

    if let None = opt {
        *opt = Some(WorkerThread::spawn(worker_thread_main));
    }
}

fn stop_global_worker() {
    let mutex = &*WORKER;
    let mut lock = mutex.lock().unwrap();
    let opt = &mut *lock;
    if let Some(worker) = opt.take() {
        if let Err(e) = worker.join() {
            println!("Error: {:?}", e);
        }
    }
}

const MESSAGE: &'static str = "Hello, world! ";

//noinspection DuplicatedCode
fn worker_thread_main(cancellation_token: Arc<AtomicBool>) {
    let mut tx = CustomTx::new(|value: u32| -> Result<(), Box<dyn Error>> {
        println!("tx/rx: {}", value);
        let mut current = CURRENT_SIGNAL.lock().map_err(|_| "Mutex error")?;
        *current = value;
        Ok(())
    })
        .cancel_on(cancellation_token)
        .interval(Duration::from_millis(500))
        .rtsm(RtsmRanges::new(10..40, 60..90).unwrap())
        .morse_encode::<ITU>();

    for char in iter::repeat(MESSAGE).map(str::chars).flatten() {
        println!("Encoding: {}", char);
        match tx.send(char) {
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
            _ => {}
        }
    }
}
