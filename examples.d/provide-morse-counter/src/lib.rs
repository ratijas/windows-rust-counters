#![allow(unused_variables, non_snake_case)]

#[macro_use]
extern crate lazy_static;

use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use log::{info, error};

use morse_stream::*;
use signal_flow::*;
use signal_flow::rtsm::*;
use win_high::format::*;
use win_high::perf::provide::*;
use win_high::perf::types::*;
use win_high::perf::values::CounterValue;
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
    winlog::init("Morse").unwrap();

    let ctx = if pContext.is_null() {
        vec![]
    } else {
        // SAFETY: we have to trust the system
        unsafe { split_nul_delimited_double_nul_terminated_ptr(pContext) }.collect()
    };
    info!("Received request to open with context: {:?}", ctx);

    match start_global_workers() {
        Ok(_) => info!("Started global worker"),
        Err(_) => {
            error!("Could not start global worker");
            return ERROR_ACCESS_DENIED;
        }
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
    // panics across FFI boundary is UB, hence must handle any errors here
    match collect(
        lpValueName,
        lppData,
        lpcbTotalBytes,
        lpNumObjectTypes,
    ) {
        Ok(_) => {
            ERROR_SUCCESS
        }
        Err(error) => {
            if error.error_code() != ERROR_MORE_DATA {
                error!("Error while collecting data: {:?}", error.with_message());
            }
            unsafe {
                lpcbTotalBytes.write(0);
                lpNumObjectTypes.write(0);
            }
            error.error_code()
        }
    }
}

fn collect(
    lpValueName: LPWSTR,
    lppData: *mut LPVOID,
    lpcbTotalBytes: LPDWORD,
    lpNumObjectTypes: LPDWORD,
) -> WinResult<()> {
    unsafe {
        let query = U16CStr::from_ptr_str(lpValueName).to_string_lossy();
        let query_type = QueryType::from_str(&query).map_err(|_| WinError::new(ERROR_BAD_ARGUMENTS))?;

        info!("query is: {:?}", query_type);

        let buffer = std::slice::from_raw_parts_mut(lppData.cast::<*mut u8>().read() as *mut u8, lpcbTotalBytes.read() as usize);

        let p = PROVIDER.lock().map_err(|_| WinError::new(ERROR_LOCK_FAILED))?;
        let collected: Collected = p.collect(query_type, buffer)?;

        lppData.cast::<*mut u8>().write(buffer.as_mut_ptr().add(collected.total_bytes));
        lpcbTotalBytes.write(collected.total_bytes as _);
        lpNumObjectTypes.write(collected.num_object_types as _);
    }
    Ok(())
}

#[no_mangle]
extern "system" fn MyCloseProc() -> DWORD {
    info!("Received request to close");

    let _ = stop_global_workers().ok();
    info!("Stopped global worker");

    ERROR_SUCCESS
}

pub struct MorseCountersProvider {
    timer: ZeroTimeProvider,
    objects: Vec<PerfObjectTypeTemplate>,
    counters: Vec<PerfCounterDefinitionTemplate>,
}

impl MorseCountersProvider {
    pub fn new() -> Self {
        let typ = CounterTypeDefinition::from_raw(0).unwrap();
        // let typ = CounterTypeDefinition::new(
        //     Size::Dword,
        //     CounterType::Number(Number::Decimal),
        //     Timer::ObjectTimer,
        //     CalculationModifiers::empty(),
        //     DisplayFlags::NoSuffix,
        // );
        Self {
            timer: ZeroTimeProvider,
            objects: vec![PerfObjectTypeTemplate::new(symbols::MORSE_OBJECT)],
            counters: vec![
                PerfCounterDefinitionTemplate::new(symbols::CHANNEL_SOS, typ),
                PerfCounterDefinitionTemplate::new(symbols::CHANNEL_MOTD, typ),
                PerfCounterDefinitionTemplate::new(symbols::CHANNEL_CUSTOM, typ),
            ],
        }
    }
}

impl PerfProvider for MorseCountersProvider {
    fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str {
        "Morse"
    }

    fn objects(&self) -> &[PerfObjectTypeTemplate] {
        &*self.objects
    }

    fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider {
        &self.timer
    }

    fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate] {
        &*self.counters
    }

    fn instances(&self, for_object: &PerfObjectTypeTemplate) -> Option<&[PerfInstanceDefinitionTemplate]> {
        None
    }

    fn data(&self,
            for_object: &PerfObjectTypeTemplate,
            per_counter: &PerfCounterDefinitionTemplate,
            per_instance: Option<&PerfInstanceDefinitionTemplate>,
            now: PerfClock,
    ) -> CounterValue {
        let lock = CURRENT_SIGNAL.lock().unwrap();
        let signal = match per_counter.name_offset {
            symbols::CHANNEL_SOS => lock[0],
            symbols::CHANNEL_CUSTOM => lock[1],
            _ => 0,
        };
        let data = CounterValue::Dword(signal);
        info!("get data: object name #{}, counter name #{} => {:?}", for_object.name_offset, per_counter.name_offset, data);
        data
    }
}

lazy_static! {
    static ref WORKERS: Mutex<Vec<WorkerThread<()>>> = Mutex::new(vec![]);
    static ref CURRENT_SIGNAL: Mutex<[u32; 2]> = Mutex::new([0, 0]);
    static ref PROVIDER: Mutex<CachingPerfProvider<MorseCountersProvider>> = Mutex::new(CachingPerfProvider::new(MorseCountersProvider::new()));
}

fn start_global_workers() -> Result<(), ()> {
    let mutex = &*WORKERS;
    let mut lock = mutex.lock().map_err(drop)?;
    if lock.is_empty() {
        lock.push(WorkerThread::spawn(|token|
            worker_thread_main(token, 0, ConstString::new(SOS))));
        lock.push(WorkerThread::spawn(|token|
            worker_thread_main(token, 1,
                RegKeyStringsProvider::new(
                    r"SYSTEM\CurrentControlSet\Services\Morse",
                    "CustomMessage",
                )
            )));
    }
    Ok(())
}

fn stop_global_workers() -> Result<(), ()> {
    let mutex = &*WORKERS;
    let mut lock = mutex.lock().map_err(drop)?;
    for worker in lock.iter() {
        worker.cancel();
    }
    for worker in lock.drain(..) {
        if let Err(e) = worker.join() {
            error!("Error while stopping global worker: {:?}", e);
        }
    }
    Ok(())
}

const SOS: &'static str = "SOS";

//noinspection DuplicatedCode
fn worker_thread_main(cancellation_token: Arc<AtomicBool>, index: usize, mut string_provider: impl StringsProvider) {
    let mut tx = CustomTx::new(|value: u32| -> Result<(), Box<dyn Error>> {
        let mut current = CURRENT_SIGNAL.lock().map_err(|_| "Mutex error")?;
        current[index] = value;
        Ok(())
    })
        .cancel_on(cancellation_token)
        .interval(Duration::from_millis(1250))
        .rtsm(RtsmRanges::new(10..40, 60..90).unwrap())
        .morse_encode::<ITU>();

    'outer: loop {
        let string = string_provider.provide();
        for char in string.chars().chain(" ".chars()) {
            match tx.send(char) {
                Err(e) => {
                    error!("Worker error : {}", e);
                    break 'outer;
                }
                _ => {}
            }
        }
    }
}

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
        use winapi::um::winnt::KEY_READ;

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
