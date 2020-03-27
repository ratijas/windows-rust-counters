#![allow(unused_variables, non_snake_case)]

#[macro_use]
extern crate lazy_static;

use std::error::Error;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use std::mem::size_of;

use log::{error, info};
use winapi::um::winnt::KEY_READ;

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
use std::borrow::Cow;

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

    fn instances(&self, for_object: &PerfObjectTypeTemplate) -> Option<Vec<PerfInstanceDefinitionTemplate>> {
        match *NUM_INSTANCES {
            -1 => None,
            _ => {
                Some((0..*INSTANCES_USIZE).map(|i: usize| {
                    PerfInstanceDefinitionTemplate::new(
                        Cow::from(U16CString::from_str(i.to_string()).unwrap())
                    ).with_unique_id(i as _)
                }).collect())
            }
        }
    }

    fn data(&self,
            for_object: &PerfObjectTypeTemplate,
            per_counter: &PerfCounterDefinitionTemplate,
            per_instance: Option<&PerfInstanceDefinitionTemplate>,
            now: PerfClock,
    ) -> CounterValue {
        let lock = CURRENT_SIGNAL.lock().unwrap();
        let deref: &[Vec<DWORD>; 3] = &*lock;

        let vec = match per_counter.name_offset {
            symbols::CHANNEL_SOS => &deref[0],
            symbols::CHANNEL_CUSTOM => &deref[1],
            symbols::CHANNEL_MOTD => &deref[2],
            _ => return CounterValue::Dword(0),
        };
        let index = match per_instance {
            Some(inst) => inst.UniqueID,
            None => 0,
        } as usize;
        let signal = vec[index];
        let data = CounterValue::Dword(signal);
        info!("get data: object name #{}, counter name #{} => {:?}", for_object.name_offset, per_counter.name_offset, data);
        data
    }
}

lazy_static! {
    static ref WORKERS: Mutex<Vec<WorkerThread<()>>> = Mutex::new(vec![]);
    /// Number of instances stored in registry
    static ref NUM_INSTANCES: LONG = get_number_of_instances();
    /// Size of CURRENT_SIGNAL Vec buffers.
    static ref INSTANCES_USIZE: usize = if *NUM_INSTANCES == -1 { 1 } else { *NUM_INSTANCES as _ };
    /// Storage of current signal per counter per instance.
    /// When NUM_INSTANCES is -1 (PERF_NO_INSTANCES, implicit global instance), Vec len is 1.
    static ref CURRENT_SIGNAL: Mutex<[Vec<DWORD>; 3]> = init_current_signal();
    static ref PROVIDER: Mutex<CachingPerfProvider<MorseCountersProvider>> = Mutex::new(CachingPerfProvider::new(MorseCountersProvider::new()));
}

const SUB_KEY_MORSE: &str = r"SYSTEM\CurrentControlSet\Services\Morse";
const VALUE_NAME_CUSTOM_MESSAGE: &str = "CustomMessage";
const VALUE_NAME_NUM_INSTANCES: &str = "NumInstances";

fn start_global_workers() -> Result<(), ()> {
    let mutex = &*WORKERS;
    let mut lock = mutex.lock().map_err(drop)?;
    if lock.is_empty() {
        lock.push(WorkerThread::spawn(|token|
            worker_thread_main(token, 0, ConstString::new(SOS))));
        lock.push(WorkerThread::spawn(|token|
            worker_thread_main(token, 1,
                               RegKeyStringsProvider::new(
                                   SUB_KEY_MORSE,
                                   VALUE_NAME_CUSTOM_MESSAGE,
                               ))));
        lock.push(WorkerThread::spawn(|token|
            worker_thread_main(token, 2, RandomJokeProvider)));
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

fn get_number_of_instances() -> LONG {
    let sub_key = U16CString::from_str(SUB_KEY_MORSE).unwrap();
    if let Ok(hkey) = RegOpenKeyEx_Safe(
        HKEY_LOCAL_MACHINE,
        sub_key.as_ptr(),
        0,
        KEY_READ,
    ) {
        if let Ok(vec) = query_value(
            *hkey,
            VALUE_NAME_NUM_INSTANCES,
            None,
            None,
        ) {
            if vec.len() == size_of::<LONG>() {
                // read vector as int
                let mut bytes = [0; 4];
                bytes.copy_from_slice(&vec[..size_of::<LONG>()]);
                return LONG::from_ne_bytes(bytes).min(1024).max(1);
            }
        }
    }
    return -1;
}

fn init_current_signal() -> Mutex<[Vec<DWORD>; 3]> {
    let vec = vec![0 as DWORD; *INSTANCES_USIZE];
    Mutex::new([vec.clone(), vec.clone(), vec])
}

const SOS: &'static str = "SOS";

//noinspection DuplicatedCode
fn worker_thread_main(cancellation_token: Arc<AtomicBool>, index: usize, mut string_provider: impl StringsProvider) {
    let mut tx = CustomTx::new(|values: Vec<DWORD>| -> Result<(), Box<dyn Error>> {
        let mut lock = CURRENT_SIGNAL.lock().map_err(|_| "Mutex error")?;
        let deref: &mut [Vec<DWORD>; 3] = &mut *lock;
        deref[index] = values;
        Ok(())
    })
        .cancel_on(cancellation_token)
        .interval(Duration::from_millis(1250))
        .chunks(*INSTANCES_USIZE)
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

pub struct RandomJokeProvider;

impl RandomJokeProvider {
    pub fn fetch(&self) -> Result<String, Box<dyn Error>> {
        const URL: &str = "http://api.icndb.com/jokes/random?limitTo=nerdy";

        let resp: serde_json::Value = reqwest::blocking::get(URL)?
            .json()?;
        let joke = Self::get_in(&resp).ok_or("invalid json")?;
        let joke = xml::unescape(&joke).unwrap_or(joke.to_string());
        Ok(joke)
    }

    fn get_in(json: &serde_json::Value) -> Option<&str> {
        json.get("value")?
            .get("joke")?
            .as_str()
    }
}

impl StringsProvider for RandomJokeProvider {
    fn provide(&mut self) -> String {
        match self.fetch() {
            Ok(string) => string,
            Err(why) => {
                error!("MOTD error: {}", why);
                "".to_string()
            }
        }
    }
}
