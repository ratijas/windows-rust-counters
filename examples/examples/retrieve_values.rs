use std::io::{self, Cursor};
use std::mem::align_of;

use hexyl::*;
use winapi::um::sysinfoapi::GetTickCount;

use win_high::perf::{
    consume::*,
    display::*,
    nom::*,
    types::*,
    values::*,
};
use win_high::prelude::v1::*;
use win_low::winperf::*;

fn main() {
    println!("Align of PERF_DATA_BLOCK          = {}", align_of::<PERF_DATA_BLOCK>());
    println!("Align of PERF_OBJECT_TYPE         = {}", align_of::<PERF_OBJECT_TYPE>());
    println!("Align of PERF_INSTANCE_DEFINITION = {}", align_of::<PERF_INSTANCE_DEFINITION>());
    println!("Align of PERF_COUNTER_DEFINITION  = {}", align_of::<PERF_COUNTER_DEFINITION>());
    println!("Align of PERF_COUNTER_BLOCK       = {}", align_of::<PERF_COUNTER_BLOCK>());
    println!("Align of DWORD                    = {}", align_of::<DWORD>());

    let meta = get_counters_info(None, UseLocale::English)
        .expect("get_counters_info");

    // make sure we close HKEY afterwards
    let _hkey = RegConnectRegistryW_Safe(null(), HKEY_PERFORMANCE_DATA)
        .expect("connect to registry");

    {
        println!("Querying system data");

        let buf = do_get_values().expect("Get values");
        let (_, perf_data) = perf_data_block(buf.as_slice()).expect("Parse data block");

        // println!("Whole result:");
        // println!("{:?}", perf_data);
        xxd(buf.as_slice()).expect("Print hex value");
        print_perf_data(&perf_data, &meta);
    }

    const SYSTEM_NAME_INDEX: DWORD = 11962;
    let counter_uptime_index: DWORD = meta.map().iter()
        .find(|(_, counter)| counter.name_value.as_str() == "SOS")
        .map(|(&index, _)| index as DWORD)
        .expect("Uptime counter name not found");

    println!();
    println!("Monitoring system uptime in a loop:");
    println!();
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_secs(1));

        let buf = do_get_values().expect("Get values");
        let (_, perf_data) = perf_data_block(buf.as_slice()).expect("Parse data block");

        // Find System object
        let obj_system = perf_data.object_types.iter()
            .find(|obj| obj.ObjectNameTitleIndex == SYSTEM_NAME_INDEX)
            .expect("System object not found");

        unsafe {
            let slice = std::slice::from_raw_parts(obj_system.raw as *const PERF_OBJECT_TYPE as *const u8, obj_system.TotalByteLength as usize);
            xxd( slice).expect("xxd");
        }

        let counter_uptime = obj_system.counters.iter()
            .find(|counter| counter.CounterNameTitleIndex == counter_uptime_index)
            .expect("Uptime counter not found");

        // println!("DataBlock  PerfTime: {:?}; FreqTime: {:?}; PerfTime100nSec: {:?}",
        //          perf_data.PerfTime, perf_data.PerfFreq, perf_data.PerfTime100nSec);
        // println!("ObjectType PerfTime: {:?}; FreqTime: {:?}", obj_system.PerfTime, obj_system.PerfFreq);
        if let PerfObjectData::Singleton(block) = &obj_system.data {
            xxd(block.data()).expect("xxd");
            let value = CounterVal::try_get(counter_uptime, block).expect("get value");
            println!("Value: {:?}", value);

            // {
            //     let raw = get_slice(counter_uptime, block).expect("get slice");
            //     let mut bytes = [0u8; 4];
            //     bytes.copy_from_slice(raw);
                // println!("Uptime: Raw = {:?}; U64 = {:#0x}; F64 = {}", raw, u64::from_ne_bytes(bytes), f64::from_ne_bytes(bytes));
            // }
            // println!("GetTickCount: {}", unsafe { GetTickCount() });

            // let sample = unsafe { get_sample(&*perf_data, &*obj_system, &*counter_uptime, &*block) }
            //     .expect("Get sample");
            // let display = display_calculated_value(&sample, None).expect("Display calculated value");
            // println!("Sample: {:?}", sample);
            // println!("Printed: {}", display);
        }
    }
}

fn print_perf_data(data: &PerfDataBlock, meta: &AllCounters) {
    for obj in data.object_types.iter() {
        let name = &meta.get(obj.ObjectNameTitleIndex).expect("Object name").name_value;
        println!("Object #{}, name: {:?}", obj.ObjectNameTitleIndex, name);
        println!("Has instances? {}", obj.NumInstances != PERF_NO_INSTANCES);
        match &obj.data {
            PerfObjectData::Singleton(block) => print_counters_data("", &*obj.counters, block, &meta),
            PerfObjectData::Instances(pairs) => pairs.iter().for_each(|(instance, block)| {
                println!("  Instance [{}], name: {:?}", instance.UniqueID, instance.name);
                print_counters_data("    ", &*obj.counters, block, &meta);
            })
        }
    }
}

fn print_counters_data(left_pad: &str, counters: &[PerfCounterDefinition], block: &PerfCounterBlock, meta: &AllCounters) {
    println!("{}Data block:", left_pad);
    xxd(block.data()).expect("xxd");
    println!("{}Counters:", left_pad);
    for c in counters {
        let raw = get_slice(c, block).expect("get slice");
        let name = &meta.get(c.CounterNameTitleIndex).expect("Counter name").name_value;
        let typ = CounterTypeDefinition::from_raw(c.CounterType).expect("Counter type");

        println!("{}  Counter #{}, name: {:?}, scale: {}, offset: {}, size: {}, type: {:#010x}",
                 left_pad,
                 c.CounterNameTitleIndex,
                 name,
                 c.DefaultScale,
                 c.CounterOffset,
                 c.CounterSize,
                 c.CounterType,
        );
        println!("{}    Type: {:?}", left_pad, typ);
        xxd(raw).expect("xxd");
    }
}

fn do_get_values() -> WinResult<Vec<u8>> {
    let mut typ: DWORD = 0;

    // Retrieve counter data for the Processor object.
    let value = query_value(
        HKEY_PERFORMANCE_DATA,
        "11962",  // system uptime
        Some(&mut typ),
        Some(2_000_000),
    )?;

    assert_eq!(typ, 3);  // should be 3, which means binary

    Ok(value)
}

fn xxd(buffer: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = Cursor::new(buffer);
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let show_color = true;
    let border_style = BorderStyle::Unicode;
    let squeeze = false;

    let mut printer = Printer::new(&mut stdout_lock, show_color, border_style, squeeze);
    printer.display_offset(0);
    printer.print_all(&mut reader)
}
