use std::io::{self, Write};
use std::mem::align_of;

use win_high::perf::{consume::*, nom::*, types::*, values::*};
use win_high::prelude::v2::*;

fn main() {
    println!(
        "Align of PERF_DATA_BLOCK          = {}",
        align_of::<PERF_DATA_BLOCK>()
    );
    println!(
        "Align of PERF_OBJECT_TYPE         = {}",
        align_of::<PERF_OBJECT_TYPE>()
    );
    println!(
        "Align of PERF_INSTANCE_DEFINITION = {}",
        align_of::<PERF_INSTANCE_DEFINITION>()
    );
    println!(
        "Align of PERF_COUNTER_DEFINITION  = {}",
        align_of::<PERF_COUNTER_DEFINITION>()
    );
    println!(
        "Align of PERF_COUNTER_BLOCK       = {}",
        align_of::<PERF_COUNTER_BLOCK>()
    );
    println!("Align of DWORD                    = {}", align_of::<u32>());

    let meta = get_counters_info(None, UseLocale::English).expect("get_counters_info");

    // make sure we close HKEY afterwards
    let _hkey = RegConnectRegistryW_Safe(PCWSTR::null(), HKEY_PERFORMANCE_DATA)
        .expect("connect to registry");

    {
        println!("Querying system data");

        let buf = do_get_values().expect("Get values");
        let (_, perf_data) = perf_data_block(buf.as_slice()).expect("Parse data block");

        // println!("Whole result:");
        // println!("{:?}", perf_data);
        xxd(buf.as_slice());
        print_perf_data(&perf_data, &meta);
    }

    const SYSTEM_NAME_INDEX: u32 = 11962;
    let counter_uptime_index: u32 = meta
        .map()
        .iter()
        .find(|(_, counter)| counter.name_value.as_str() == "SOS")
        .map(|(&index, _)| index as u32)
        .expect("Uptime counter name not found");

    println!();
    println!("Monitoring system uptime in a loop:");
    println!();
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_secs(1));

        let buf = do_get_values().expect("Get values");
        let (_, perf_data) = perf_data_block(buf.as_slice()).expect("Parse data block");

        // Find System object
        let obj_system = perf_data
            .object_types
            .iter()
            .find(|obj| obj.ObjectNameTitleIndex == SYSTEM_NAME_INDEX)
            .expect("System object not found");

        unsafe {
            let slice = std::slice::from_raw_parts(
                &obj_system.raw as *const PERF_OBJECT_TYPE as *const u8,
                obj_system.TotalByteLength as usize,
            );
            xxd(slice);
        }

        let counter_uptime = obj_system
            .counters
            .iter()
            .find(|counter| counter.CounterNameTitleIndex == counter_uptime_index)
            .expect("Uptime counter not found");

        // println!("DataBlock  PerfTime: {:?}; FreqTime: {:?}; PerfTime100nSec: {:?}",
        //          perf_data.PerfTime, perf_data.PerfFreq, perf_data.PerfTime100nSec);
        // println!("ObjectType PerfTime: {:?}; FreqTime: {:?}", obj_system.PerfTime, obj_system.PerfFreq);
        if let PerfObjectData::Singleton(block) = &obj_system.data {
            xxd(block.data());
            let value = CounterValue::try_get(counter_uptime, block).expect("get value");
            println!("Value: {:?}", value);

            // use win_high::perf::display::*;
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
        let name = &meta
            .get(obj.ObjectNameTitleIndex)
            .expect("Object name")
            .name_value;
        println!("Object #{}, name: {:?}", obj.ObjectNameTitleIndex, name);
        println!("Has instances? {}", obj.NumInstances != PERF_NO_INSTANCES);
        match &obj.data {
            PerfObjectData::Singleton(block) => {
                print_counters_data("", &*obj.counters, block, &meta)
            }
            PerfObjectData::Instances(pairs) => pairs.iter().for_each(|(instance, block)| {
                println!(
                    "  Instance [{}], name: {:?}",
                    instance.UniqueID, instance.name
                );
                print_counters_data("    ", &*obj.counters, block, &meta);
            }),
        }
    }
}

fn print_counters_data(
    left_pad: &str,
    counters: &[PerfCounterDefinition],
    block: &PerfCounterBlock,
    meta: &AllCounters,
) {
    println!("{}Data block:", left_pad);
    xxd(block.data());
    println!("{}Counters:", left_pad);
    for c in counters {
        let raw = get_slice(c, block).expect("get slice");
        let name = &meta
            .get(c.CounterNameTitleIndex)
            .expect("Counter name")
            .name_value;
        let typ = CounterTypeDefinition::from_raw(c.CounterType).expect("Counter type");

        println!(
            "{}  Counter #{}, name: {:?}, scale: {}, offset: {}, size: {}, type: {:#010x}",
            left_pad,
            c.CounterNameTitleIndex,
            name,
            c.DefaultScale,
            c.CounterOffset,
            c.CounterSize,
            c.CounterType,
        );
        println!("{}    Type: {:?}", left_pad, typ);
        xxd(raw);
    }
}

fn do_get_values() -> WinResult<Vec<u8>> {
    let mut typ: REG_VALUE_TYPE = REG_NONE;

    // Retrieve counter data for the Processor object.
    let value = query_value(
        HKEY_PERFORMANCE_DATA,
        "11962", // system uptime
        Some(&mut typ),
        Some(2_000_000),
    )?;

    assert_eq!(typ, REG_BINARY);

    Ok(value)
}

fn xxd(buffer: &[u8]) {
    const BYTES_PER_LINE: usize = 16;

    let stdout = io::stdout();
    let mut f = stdout.lock();

    writeln!(f, "┌────────┬─────────────────────────┬─────────────────────────┬────────┬────────┐").expect("xxd");
    let mut idx: usize = 0;
    while idx < buffer.len() {
        let line_len = BYTES_PER_LINE.min(buffer.len() - idx);
        write!(f, "│{:08x}│ ", idx).expect("xxd");
        for i in 0..BYTES_PER_LINE {
            if i < line_len {
                let byte = buffer[idx + i];
                write!(f, "{:02x} ", byte).expect("xxd");
            } else {
                write!(f, "   ").expect("xxd");
            }
            if i == BYTES_PER_LINE / 2 - 1 {
                write!(f, "┊ ").expect("xxd");
            }
        }
        write!(f, "│").expect("xxd");
        for i in 0..BYTES_PER_LINE {
            if i < line_len {
                let byte = buffer[idx + i];
                let chr = if byte == 0x00 {
                        '.'
                    } else if byte == 0x20 {
                        ' '
                    } else if byte.is_ascii_graphic() {
                        byte as char
                    } else {
                        '.'
                    };
                write!(f, "{}", chr).expect("xxd");
            } else {
                write!(f, " ").expect("xxd");
            }
            if i == BYTES_PER_LINE / 2 - 1 {
                write!(f, "┊").expect("xxd");
            }
        }
        writeln!(f, "│").expect("xxd");
        idx += BYTES_PER_LINE;
    }
    writeln!(f, "└────────┴─────────────────────────┴─────────────────────────┴────────┴────────┘").expect("xxd");
}
