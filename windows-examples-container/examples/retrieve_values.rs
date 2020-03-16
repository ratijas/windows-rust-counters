use std::io::{self, Cursor};
use std::mem::{align_of, size_of};

use hexyl::*;
use winapi::um::sysinfoapi::GetTickCount;

use win_high::perf::consume::*;
use win_high::perf::display::*;
use win_high::perf::types::*;
use win_high::prelude::v1::*;
use win_low::winperf::*;

// #[derive(Debug)]
pub struct PerfDataBlock<'a> {
    pub inner: &'a PERF_DATA_BLOCK,
    pub system_name_u16: &'a U16CStr,
    pub system_name: String,
    pub object_types: Vec<PerfObjectType<'a>>,
}

// #[derive(Debug)]
pub struct PerfObjectType<'a> {
    pub inner: &'a PERF_OBJECT_TYPE,
    pub counters: Vec<PerfCounterDefinition<'a>>,
    pub data: PerfObjectData<'a>,
}

// #[derive(Debug)]
pub enum PerfObjectData<'a> {
    Single(PerfCounterBlock<'a>),
    Multi(Vec<(PerfInstanceDefinition<'a>, PerfCounterBlock<'a>)>),
}

// #[derive(Debug)]
pub struct PerfCounterDefinition<'a> {
    pub inner: &'a PERF_COUNTER_DEFINITION,
}

// #[derive(Debug)]
pub struct PerfCounterBlock<'a> {
    pub inner: &'a PERF_COUNTER_BLOCK,
    pub data: &'a [u8],
}

// #[derive(Debug)]
pub struct PerfInstanceDefinition<'a> {
    pub inner: &'a PERF_INSTANCE_DEFINITION,
    pub name_u16: &'a U16CStr,
    pub name: String,
}

impl<'a> PerfCounterDefinition<'a> {
    pub fn get_raw<'b>(&self, block: &'b PerfCounterBlock) -> &'b [u8] {
        let offset = self.inner.CounterOffset as usize;
        let length = self.inner.CounterSize as usize;
        &block.data[offset..offset + length]
    }
}

fn parse_perf_data_block(buf: &[u8]) -> PerfDataBlock {
    if buf.len() < size_of::<PERF_DATA_BLOCK>() { panic!("size_of"); }
    let inner = unsafe { (buf.as_ptr() as *const PERF_DATA_BLOCK).as_ref() }.unwrap();

    let name_slice = unsafe {
        let offset = inner.SystemNameOffset as usize;
        let length = inner.SystemNameLength as usize;
        let ptr = buf.as_ptr().add(offset) as *const u16;
        std::slice::from_raw_parts(ptr, length / 2)
    };
    let name_u16 = U16CStr::from_slice_with_nul(name_slice)
        .expect("system name unicode");
    let name = name_u16.to_string_lossy();

    let mut object_types = vec![];
    let mut next_object_type = &buf[inner.HeaderLength as usize..];
    for _ in 0..inner.NumObjectTypes {
        let perf_object_type = parse_perf_object_type(next_object_type);
        let length = perf_object_type.inner.TotalByteLength as usize;
        object_types.push(perf_object_type);
        next_object_type = &next_object_type[length..];
    }

    PerfDataBlock {
        inner,
        system_name_u16: name_u16,
        system_name: name,
        object_types,
    }
}

fn parse_perf_object_type(buf: &[u8]) -> PerfObjectType {
    if buf.len() < size_of::<PERF_OBJECT_TYPE>() { panic!("size_of"); }
    let inner = unsafe { (buf.as_ptr() as *const PERF_OBJECT_TYPE).as_ref().unwrap() };

    let mut counters = vec![];
    let mut next_counter_def = &buf[inner.HeaderLength as usize..];
    for _ in 0..inner.NumCounters {
        let counter_def = parse_perf_counter_definition(next_counter_def);
        let length = counter_def.inner.ByteLength as usize;
        counters.push(counter_def);
        next_counter_def = &next_counter_def[length..];
    }

    // either PERF_INSTANCE_DEFINITION or PERF_COUNTER_BLOCK
    let mut next_section = &buf[inner.DefinitionLength as usize..];
    assert_eq!(next_counter_def.as_ptr(), next_section.as_ptr());

    let data = if inner.NumInstances == PERF_NO_INSTANCES {
        PerfObjectData::Single(make_perf_counter_block(next_section))
    } else {
        let mut vec = Vec::new();

        for _ in 0..inner.NumInstances {
            let instance = parse_perf_instance_definition(next_section);
            next_section = &next_section[instance.inner.ByteLength as usize..];
            let counter_block = make_perf_counter_block(next_section);
            next_section = &next_section[counter_block.inner.ByteLength as usize..];

            vec.push((instance, counter_block));
        }
        PerfObjectData::Multi(vec)
    };

    PerfObjectType {
        inner,
        counters,
        data,
    }
}

fn parse_perf_instance_definition(buf: &[u8]) -> PerfInstanceDefinition {
    if buf.len() < size_of::<PERF_INSTANCE_DEFINITION>() { panic!("size_of"); }
    let inner = unsafe { (buf.as_ptr() as *const PERF_INSTANCE_DEFINITION).as_ref() }.unwrap();

    let name_slice = unsafe {
        let offset = inner.NameOffset as usize;
        let length = inner.NameLength as usize;
        let ptr = buf.as_ptr().add(offset) as *const u16;
        std::slice::from_raw_parts(ptr, length / 2)
    };
    let name_u16 = U16CStr::from_slice_with_nul(name_slice)
        .expect("instance name unicode");
    let name = name_u16.to_string_lossy();

    PerfInstanceDefinition {
        inner,
        name_u16,
        name,
    }
}

fn parse_perf_counter_definition(buf: &[u8]) -> PerfCounterDefinition {
    if buf.len() < size_of::<PERF_COUNTER_DEFINITION>() { panic!("size_of"); }
    let inner = unsafe { (buf.as_ptr() as *const PERF_COUNTER_DEFINITION).as_ref() }.unwrap();
    PerfCounterDefinition { inner }
}

fn make_perf_counter_block(buf: &[u8]) -> PerfCounterBlock {
    if buf.len() < size_of::<PERF_COUNTER_BLOCK>() { panic!("size_of"); }
    let inner = unsafe { (buf.as_ptr() as *const PERF_COUNTER_BLOCK).as_ref() }.unwrap();
    PerfCounterBlock {
        inner,
        data: buf,
    }
}

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
        let perf_data = parse_perf_data_block(buf.as_slice());

        // println!("Whole result:");
        // println!("{:?}", perf_data);
        xxd(buf.as_slice()).expect("Print hex value");
        print_perf_data(&perf_data, &meta);
    }

    const SYSTEM_NAME_INDEX: DWORD = 2;
    let counter_uptime_index: DWORD = meta.map().iter()
        .find(|(_, counter)| counter.name_value.as_str() == "System Up Time")
        .map(|(&index, _)| index as DWORD)
        .expect("Uptime counter name not found");

    println!();
    println!("Monitoring system uptime in a loop:");
    println!();
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_secs(1));

        let buf = do_get_values().expect("Get values");
        let perf_data = parse_perf_data_block(buf.as_slice());

        // Find System object
        let obj_system = perf_data.object_types.iter()
            .find(|obj| obj.inner.ObjectNameTitleIndex == SYSTEM_NAME_INDEX)
            .expect("System object not found");

        let counter_uptime = obj_system.counters.iter()
            .find(|counter| counter.inner.CounterNameTitleIndex == counter_uptime_index)
            .expect("Uptime counter not found");

        // println!("DataBlock  PerfTime: {:?}; FreqTime: {:?}; PerfTime100nSec: {:?}",
        //          perf_data.inner.PerfTime, perf_data.inner.PerfFreq, perf_data.inner.PerfTime100nSec);
        // println!("ObjectType PerfTime: {:?}; FreqTime: {:?}", obj_system.inner.PerfTime, obj_system.inner.PerfFreq);
        if let PerfObjectData::Single(block) = &obj_system.data {
            xxd(block.data).expect("xxd");
            {
                let raw = counter_uptime.get_raw(block);
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(raw);
                // println!("Uptime: Raw = {:?}; U64 = {:#0x}; F64 = {}", raw, u64::from_ne_bytes(bytes), f64::from_ne_bytes(bytes));
            }
            println!("GetTickCount: {}", unsafe { GetTickCount() });
            let sample = unsafe { get_sample(perf_data.inner, obj_system.inner, counter_uptime.inner, block.inner) }
                .expect("Get sample");
            let display = display_calculated_value(&sample, None).expect("Display calculated value");
            println!("Sample: {:?}", sample);
            println!("Printed: {}", display);
        }
    }
}

fn print_perf_data(data: &PerfDataBlock, meta: &AllCounters) {
    for obj in data.object_types.iter() {
        let name = &meta.get(obj.inner.ObjectNameTitleIndex as usize).expect("Object name").name_value;
        println!("Object #{}, name: {:?}", obj.inner.ObjectNameTitleIndex, name);
        println!("Has instances? {}", obj.inner.NumInstances != PERF_NO_INSTANCES);
        match &obj.data {
            PerfObjectData::Single(block) => print_counters_data("", &*obj.counters, block, &meta),
            PerfObjectData::Multi(vec) => vec.iter().for_each(|(instance, block)| {
                println!("  Instance [{}], name: {}", instance.inner.UniqueID, instance.name);
                print_counters_data("    ", &*obj.counters, block, &meta);
            })
        }
    }
}

fn print_counters_data(left_pad: &str, counters: &[PerfCounterDefinition], block: &PerfCounterBlock, meta: &AllCounters) {
    println!("{}Data block:", left_pad);
    xxd(block.data).expect("xxd");
    println!("{}Counters:", left_pad);
    for c in counters {
        let raw = c.get_raw(block);
        let name = &meta.get(c.inner.CounterNameTitleIndex as usize).expect("Counter name").name_value;
        let typ = CounterTypeDefinition::from_raw(c.inner.CounterType).expect("Counter type");

        println!("{}  Counter #{}, name: {:?}, scale: {}, offset: {}, size: {}, type: {:#010x}",
                 left_pad,
                 c.inner.CounterNameTitleIndex,
                 name,
                 c.inner.DefaultScale,
                 c.inner.CounterOffset,
                 c.inner.CounterSize,
                 c.inner.CounterType,
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
        "2",  // system uptime
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
