//! Types and functions for counter data providers
#![allow(non_snake_case)]

use std::error::Error;
use std::mem::size_of;
use std::str::FromStr;
use std::cell::RefCell;

use win_low::winperf::*;

use crate::perf::nom::*;
use crate::perf::types::*;
use crate::perf::values::CounterValue;
use crate::prelude::v1::*;

use winapi::um::winnt::KEY_READ;

pub trait PerfProvider {
    fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str;

    fn first_counter(&self, for_object: &PerfObjectTypeTemplate) -> WinResult<DWORD> {
        let sub_key = format!(r"SYSTEM\CurrentControlSet\Services\{}\Performance", self.service_name(for_object));
        let sub_key_wstr = U16CString::from_str(sub_key).map_err(|_| WinError::new(ERROR_INVALID_DATA))?;
        let hkey = RegOpenKeyEx_Safe(
            HKEY_LOCAL_MACHINE,
            sub_key_wstr.as_ptr(),
            0,
            KEY_READ,
        )?;
        let buffer = query_value(
            *hkey,
            "First Counter",
            None,
            None,
        )?;
        let first_counter = nom::number::complete::le_u32::<()>(&*buffer)
            .map_err(|_| WinError::new(ERROR_INVALID_DATA))?
            .1;

        Ok(first_counter)
    }

    fn objects(&self) -> &[PerfObjectTypeTemplate];

    fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider;

    fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate];

    fn instances(&self, for_object: &PerfObjectTypeTemplate) -> Option<&[PerfInstanceDefinitionTemplate]>;

    fn data(&self,
            for_object: &PerfObjectTypeTemplate,
            per_counter: &PerfCounterDefinitionTemplate,
            per_instance: Option<&PerfInstanceDefinitionTemplate>,
            now: PerfClock,
    ) -> CounterValue;

    fn should_respond(&self, to_query: &QueryType, with_object: &PerfObjectTypeTemplate) -> WinResult<bool> {
        let answer = match to_query {
            QueryType::Global => true,
            QueryType::Items(items) => {
                let base = self.first_counter(&with_object)?;
                let name_index = base + with_object.name_offset;
                items.contains(&name_index)
            }
            _ => false,
        };
        Ok(answer)
    }

    fn collect_object<'a>(
        &self,
        object_template: &PerfObjectTypeTemplate,
        now: PerfClock,
        mut buffer: &'a mut [u8],
    ) -> WinResult<(usize, &'a mut [u8])>
    {
        // build all headers and definitions before writing anything.
        // counters data will be requested at the very last step, when it is really needed.
        let first_counter = self.first_counter(object_template)?;
        let counter_templates = self.counters(object_template);
        let counters_block_template = layout_of_counters(first_counter, counter_templates);
        let counters = counters_block_template.counters();
        let instance_templates = self.instances(object_template);
        let instances = instance_templates.map(layout_of_instances);
        let object = object_template.build_layout(
            first_counter,
            counters_block_template.counters(),
            instances.as_ref().map(Vec::as_ref),
            counters_block_template.block(),
            now,
        );

        // write header and counters definitions
        let _ = write_object_struct_header(&object, buffer).map_err(error_small_buffer)?;
        buffer = &mut buffer[object.HeaderLength as usize..];

        for counter in counters {
            let _ = write_counter_definition(counter, buffer).map_err(error_small_buffer)?;
            buffer = &mut buffer[counter.ByteLength as usize..];
        }

        if instances.is_none() {
            // no instances, single block
            let _ = self.write_block(
                object_template,
                None,
                counter_templates,
                &counters_block_template,
                now,
                buffer,
            ).map_err(error_small_buffer)?;
            buffer = &mut buffer[counters_block_template.ByteLength as usize..];
        } else {
            let instance_templates = instance_templates.unwrap();
            let instances = instances.unwrap();
            for (instance, instance_template) in instances.iter().zip(instance_templates) {
                // write instance
                let _ = instance_template.write_with_layout(instance, buffer).map_err(error_small_buffer)?;
                buffer = &mut buffer[instance.ByteLength as usize..];
                // write block
                let _ = self.write_block(
                    object_template,
                    Some(instance_template),
                    counter_templates,
                    &counters_block_template,
                    now,
                    buffer,
                ).map_err(error_small_buffer)?;
                buffer = &mut buffer[instance.ByteLength as usize..];
            }
        }
        Ok((object.TotalByteLength as _, buffer))
    }

    fn write_block<'a>(
        &self,
        object_template: &PerfObjectTypeTemplate,
        instance_template: Option<&PerfInstanceDefinitionTemplate>,
        counter_templates: &[PerfCounterDefinitionTemplate],
        counters_block_template: &CountersBlockTemplate,
        now: PerfClock,
        buffer: &'a mut [u8],
    ) -> Result<(usize, &'a mut [u8]), ()>
    {
        let counters = counters_block_template.counters();
        let mut block = counters_block_template.buffer(buffer)?;
        for (counter, counter_template) in counters.iter().zip(counter_templates) {
            let value = self.data(object_template, counter_template, instance_template, now);
            block.write(counter, value)?;
        }
        let i = block.buffer.len();
        drop(block);
        Ok((i, &mut buffer[i..]))
    }

    fn collect(&self, query: QueryType, mut buffer: &mut [u8]) -> WinResult<Collected> {
        let mut total_bytes: usize = 0;
        let mut num_object_types = 0;
        for object in self.objects() {
            if self.should_respond(&query, object)? {
                let now = self.time_provider(object).get_time();
                let (length, rest) = self.collect_object(object, now, buffer)?;
                total_bytes += length;
                buffer = rest;
                num_object_types += 1;
            }
        }
        Ok(Collected {
            total_bytes,
            num_object_types,
        })
    }
}

pub struct CachingPerfProvider<X> {
    inner: X,
    first_counters: RefCell<Vec<(DWORD, DWORD)>>,
}

impl<X> CachingPerfProvider<X> {
    pub fn new(inner: X) -> Self {
        Self {
            inner,
            first_counters: RefCell::new(vec![]),
        }
    }

    fn lookup_first_counter(&self, name_offset: DWORD) -> Option<DWORD> {
        self.first_counters.borrow().iter()
            .find_map(|&(offset, base)| if offset == name_offset { Some(base) } else { None })
    }

    fn cache_first_counter(&self, name_offset: DWORD, first_counter: DWORD) {
        self.first_counters.borrow_mut().push((name_offset, first_counter));
    }
}

impl<X: PerfProvider> PerfProvider for CachingPerfProvider<X> {
    // pass through implementation
    fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str {
        self.inner.service_name(for_object)
    }

    // custom method
    fn first_counter(&self, for_object: &PerfObjectTypeTemplate) -> WinResult<DWORD> {
        if let Some(cached) = self.lookup_first_counter(for_object.name_offset) {
            return Ok(cached);
        } else {
            let first_counter = self.inner.first_counter(for_object)?;
            self.cache_first_counter(for_object.name_offset, first_counter);
            Ok(first_counter)
        }
    }

    fn objects(&self) -> &[PerfObjectTypeTemplate] {
        self.inner.objects()
    }

    fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider {
        self.inner.time_provider(for_object)
    }

    fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate] {
        self.inner.counters(for_object)
    }

    fn instances(&self, for_object: &PerfObjectTypeTemplate) -> Option<&[PerfInstanceDefinitionTemplate]> {
        self.inner.instances(for_object)
    }

    fn data(&self, for_object: &PerfObjectTypeTemplate, per_counter: &PerfCounterDefinitionTemplate, per_instance: Option<&PerfInstanceDefinitionTemplate>, now: PerfClock) -> CounterValue {
        self.inner.data(for_object, per_counter, per_instance, now)
    }

    fn should_respond(&self, to_query: &QueryType, with_object: &PerfObjectTypeTemplate) -> WinResult<bool> {
        self.inner.should_respond(to_query, with_object)
    }
}

pub struct Collected {
    pub total_bytes: usize,
    pub num_object_types: usize,
}

#[derive(Copy, Clone)]
pub struct PerfClock {
    PerfTime: LARGE_INTEGER,
    PerfFreq: LARGE_INTEGER,
}

pub trait PerfTimeProvider {
    fn get_time(&self) -> PerfClock;
}

pub struct ZeroTimeProvider;

impl PerfTimeProvider for ZeroTimeProvider {
    fn get_time(&self) -> PerfClock {
        unsafe {
            PerfClock {
                PerfTime: std::mem::transmute(0u64),
                PerfFreq: std::mem::transmute(0u64),
            }
        }
    }
}

#[derive(Debug)]
pub struct TickCountTimeProvider;

impl PerfTimeProvider for TickCountTimeProvider {
    fn get_time(&self) -> PerfClock {
        use winapi::um::sysinfoapi::GetTickCount;

        unsafe fn make_large_integer(value: i64) -> LARGE_INTEGER {
            let mut it = std::mem::zeroed::<LARGE_INTEGER>();
            *it.QuadPart_mut() = value;
            it
        }

        unsafe {
            // number of milliseconds that have elapsed since the system was started
            let millis = GetTickCount();
            PerfClock {
                PerfTime: make_large_integer(millis as _),
                PerfFreq: make_large_integer(1_000),
            }
        }
    }
}

#[derive(Debug)]
pub struct PerfObjectTypeTemplate {
    pub name_offset: DWORD,
    pub help_offset: DWORD,
    pub detail_level: DetailLevel,
    pub DefaultCounter: LONG,
}

impl PerfObjectTypeTemplate {
    pub fn new(
        name_offset: DWORD,
    ) -> Self {
        PerfObjectTypeTemplate {
            name_offset,
            help_offset: name_offset + 1,
            detail_level: DetailLevel::Novice,
            DefaultCounter: -1,
        }
    }

    pub fn with_detail_level(mut self, detail_level: DetailLevel) -> Self {
        self.detail_level = detail_level;
        self
    }

    pub fn with_default_counter(mut self, default_counter: LONG) -> Self {
        self.DefaultCounter = default_counter;
        self
    }

    pub fn build_layout(
        &self,
        first_counter: DWORD,
        counters: &[PERF_COUNTER_DEFINITION],
        instances: Option<&[PERF_INSTANCE_DEFINITION]>,
        block: PERF_COUNTER_BLOCK,
        now: PerfClock,
    ) -> PERF_OBJECT_TYPE
    {
        let counters_length: usize = counters.iter().map(|c| c.ByteLength as usize).sum();
        let instances_length: usize = instances.map(|it| it.iter().map(|i| i.ByteLength as usize).sum()).unwrap_or(0);
        // no instances == 1 block
        let blocks_length: usize = instances.map(|it| it.len()).unwrap_or(1) * (block.ByteLength as usize);

        let header_length = size_of::<PERF_OBJECT_TYPE>();
        let definition_length = header_length + counters_length;
        let total = dword_multiple(definition_length + instances_length + blocks_length);

        PERF_OBJECT_TYPE {
            TotalByteLength: total as _,
            DefinitionLength: definition_length as _,
            HeaderLength: header_length as _,
            ObjectNameTitleIndex: first_counter + self.name_offset,
            ObjectNameTitle: 0,
            ObjectHelpTitleIndex: first_counter + self.help_offset,
            ObjectHelpTitle: 0,
            DetailLevel: self.detail_level as _,
            NumCounters: counters.len() as _,
            DefaultCounter: self.DefaultCounter,
            NumInstances: instances.map(|it| it.len() as _).unwrap_or(-1),
            CodePage: 0,
            PerfTime: now.PerfTime,
            PerfFreq: now.PerfFreq,
        }
    }

    pub fn write_with_layout<'a>(
        &self,
        raw: &PERF_OBJECT_TYPE,
        buffer: &'a mut [u8],
    ) -> Result<(usize, &'a mut [u8]), Box<dyn Error>>
    {
        unsafe {
            copy_struct_into_buffer(raw, buffer).map_err(error_small_buffer)?;
        }
        Ok((raw.HeaderLength as usize, &mut buffer[raw.TotalByteLength as usize..]))
    }

    pub fn write_header<'a>(
        &self,
        first_counter: DWORD,
        counters: &[PERF_COUNTER_DEFINITION],
        instances: Option<&[PERF_INSTANCE_DEFINITION]>,
        block: PERF_COUNTER_BLOCK,
        now: PerfClock,
        buffer: &'a mut [u8],
    ) -> Result<(usize, &'a mut [u8]), Box<dyn Error>>
    {
        let layout = self.build_layout(
            first_counter,
            counters,
            instances,
            block,
            now,
        );
        self.write_with_layout(&layout, buffer)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PerfCounterDefinitionTemplate {
    pub name_offset: DWORD,
    pub help_offset: DWORD,
    pub DefaultScale: LONG,
    pub DetailLevel: DetailLevel,
    pub counter_type: CounterTypeDefinition,
    pub CounterSize: Option<DWORD>,
}

impl PerfCounterDefinitionTemplate {
    pub fn new(
        name_offset: DWORD,
        CounterType: CounterTypeDefinition,
    ) -> Self {
        let CounterSize = CounterType.size().size_of().map(|it| it as _);
        Self {
            name_offset,
            help_offset: name_offset + 1,
            DefaultScale: 0,
            DetailLevel: DetailLevel::default(),
            counter_type: CounterType,
            CounterSize,
        }
    }

    pub fn with_default_scale(mut self, scale: LONG) -> Self {
        self.DefaultScale = scale;
        self
    }

    pub fn with_detail_level(mut self, detail_level: DetailLevel) -> Self {
        self.DetailLevel = detail_level as _;
        self
    }

    pub fn with_size(&mut self, size: DWORD) -> &mut Self {
        self.CounterSize = Some(size);
        self
    }

    pub fn build_layout(&self, first_counter: DWORD, offset: DWORD) -> PERF_COUNTER_DEFINITION {
        PERF_COUNTER_DEFINITION {
            ByteLength: size_of::<PERF_COUNTER_DEFINITION>() as _,
            CounterNameTitleIndex: self.name_offset + first_counter,
            CounterNameTitle: 0,
            CounterHelpTitleIndex: self.help_offset + first_counter,
            CounterHelpTitle: 0,
            DefaultScale: self.DefaultScale,
            DetailLevel: self.DetailLevel as _,
            CounterType: self.counter_type.into_raw(),
            CounterSize: dword_multiple(self.CounterSize.expect("Cannot infer size. Please, set it manually") as _) as _,
            CounterOffset: offset as _,
        }
    }
}

#[derive(Debug)]
pub struct PerfInstanceDefinitionTemplate<'a> {
    ParentObjectTitleIndex: DWORD,
    ParentObjectInstance: DWORD,
    UniqueID: LONG,
    Name: &'a U16CStr,
}

impl<'a> PerfInstanceDefinitionTemplate<'a> {
    pub fn new(Name: &'a U16CStr) -> Self {
        PerfInstanceDefinitionTemplate {
            ParentObjectTitleIndex: 0,
            ParentObjectInstance: 0,
            UniqueID: -1,
            Name,
        }
    }

    pub fn with_unique_id(mut self, unique_id: LONG) -> Self {
        self.UniqueID = unique_id;
        self
    }

    pub fn with_parent(
        mut self,
        ParentObjectTitleIndex: DWORD,
        ParentObjectInstance: DWORD,
    ) -> Self {
        self.ParentObjectTitleIndex = ParentObjectTitleIndex;
        self.ParentObjectInstance = ParentObjectInstance;
        self
    }

    pub fn build_layout(&self) -> PERF_INSTANCE_DEFINITION {
        let struct_size = size_of::<PERF_INSTANCE_DEFINITION>();
        // +1 for nul terminator
        let name_size = (self.Name.len() + 1) * size_of::<DWORD>();

        PERF_INSTANCE_DEFINITION {
            ByteLength: dword_multiple(struct_size + name_size) as _,
            ParentObjectTitleIndex: self.ParentObjectTitleIndex,
            ParentObjectInstance: self.ParentObjectInstance,
            UniqueID: self.UniqueID,
            NameOffset: struct_size as _,
            NameLength: name_size as _,
        }
    }

    /// Returns the rest of the buffer after this object
    pub fn write_with_layout<'b>(&self, def: &PERF_INSTANCE_DEFINITION, buffer: &'b mut [u8]) -> Result<(usize, &'b mut [u8]), ()> {
        unsafe {
            copy_struct_into_buffer(def, buffer)?;
            copy_cstr_into_buffer(self.Name, buffer, def.NameOffset, def.NameLength)?;
        }
        let len = def.ByteLength as usize;
        Ok((len as usize, &mut buffer[len as usize..]))
    }

    pub fn write<'b>(&self, buffer: &'b mut [u8]) -> Result<(usize, &'b mut [u8]), ()> {
        let layout = self.build_layout();
        self.write_with_layout(&layout, buffer)
    }
}

pub struct CountersBlockTemplate {
    ByteLength: DWORD,
    counters: Vec<PERF_COUNTER_DEFINITION>,
}

impl CountersBlockTemplate {
    pub fn new() -> Self {
        CountersBlockTemplate {
            ByteLength: size_of::<PERF_COUNTER_BLOCK>() as _,
            counters: vec![],
        }
    }

    pub fn add_counter(&mut self, counter: PERF_COUNTER_DEFINITION) {
        self.ByteLength += dword_multiple(counter.CounterSize as _) as DWORD;
        self.counters.push(counter);
    }

    pub fn offset(&self) -> DWORD {
        self.ByteLength
    }

    pub fn counters(&self) -> &[PERF_COUNTER_DEFINITION] {
        &*self.counters
    }

    pub fn block(&self) -> PERF_COUNTER_BLOCK {
        PERF_COUNTER_BLOCK {
            ByteLength: self.ByteLength
        }
    }

    pub fn buffer<'a>(&self, buffer: &'a mut [u8]) -> Result<CountersBlockBuffer<'a>, ()> {
        unsafe { copy_struct_into_buffer(&self.block(), buffer)? };
        Ok(CountersBlockBuffer {
            buffer: buffer.get_mut(..self.ByteLength as usize).ok_or(())?
        })
    }
}

pub struct CountersBlockBuffer<'a> {
    buffer: &'a mut [u8],
}

impl<'a> CountersBlockBuffer<'a> {
    pub fn write<'b>(&mut self, def: &PERF_COUNTER_DEFINITION, value: CounterValue<'b>) -> Result<(), ()> {
        let offset = def.CounterOffset as usize;
        let length = def.CounterSize as usize;
        let slice = self.buffer.get_mut(offset..offset + length).ok_or(())?;

        value.write(slice).map_err(drop)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum QueryType {
    Global,
    Costly,
    Foreign,
    Items(Vec<DWORD>),
}

impl FromStr for QueryType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Global" | "" => QueryType::Global,
            "Costly" => QueryType::Costly,
            "Foreign" => QueryType::Foreign,
            _ => QueryType::Items(
                s.split_ascii_whitespace()
                    .map(|item| item.parse::<DWORD>().map_err(drop))
                    .collect::<Result<Vec<_>, _>>()?
            )
        })
    }
}

fn dword_multiple(of: usize) -> usize {
    let size = size_of::<DWORD>();
    ((of + size - 1) / size) * size
}

fn layout_of_counters(first_counter: DWORD, templates: &[PerfCounterDefinitionTemplate]) -> CountersBlockTemplate {
    let mut block = CountersBlockTemplate::new();
    for template in templates {
        let counter = template.build_layout(first_counter, block.offset());
        block.add_counter(counter);
    }
    block
}

fn layout_of_instances(templates: &[PerfInstanceDefinitionTemplate]) -> Vec<PERF_INSTANCE_DEFINITION> {
    let mut instances = Vec::new();
    for template in templates {
        let instance = template.build_layout();
        instances.push(instance);
    };
    instances
}

fn error_small_buffer(_: ()) -> WinError {
    WinError::new(ERROR_MORE_DATA)
}

unsafe fn copy_struct_into_buffer<'a, T>(source: &T, buffer: &'a mut [u8]) -> Result<(usize, &'a mut [u8]), ()> {
    let size = size_of::<T>();
    let slice_u8 = buffer.get_mut(..size).ok_or(())?;
    slice_u8.as_mut_ptr().cast::<T>().copy_from_nonoverlapping(source as *const _, 1);
    Ok((size, &mut buffer[size..]))
}

unsafe fn copy_cstr_into_buffer(str: &U16CStr, buffer: &mut [u8], offset: DWORD, length: DWORD) -> Result<(), ()> {
    let offset = offset as usize;
    let length = length as usize;
    let slice_u8 = buffer.get_mut(offset..offset + length).ok_or(())?;
    let str_u8 = downcast(str.as_slice_with_nul());
    if slice_u8.len() != str_u8.len() {
        return Err(());
    }
    slice_u8.copy_from_slice(str_u8);
    Ok(())
}

fn write_object_struct_header<'a>(object: &PERF_OBJECT_TYPE, buffer: &'a mut [u8]) -> Result<(usize, &'a mut [u8]), ()> {
    unsafe { copy_struct_into_buffer(object, buffer) }
}

fn write_counter_definition<'a>(counter: &PERF_COUNTER_DEFINITION, buffer: &'a mut [u8]) -> Result<(usize, &'a mut [u8]), ()> {
    unsafe { copy_struct_into_buffer(counter, buffer) }
}

#[cfg(test)]
mod test {
    use super::*;

    struct BasicPerfProvider {
        timer: ZeroTimeProvider,
        objects: Vec<PerfObjectTypeTemplate>,
        counters: Vec<PerfCounterDefinitionTemplate>,
        // memory_percent: u32,
        // cpu_percent: u32,
    }

    impl BasicPerfProvider {
        pub fn new(objects: Vec<PerfObjectTypeTemplate>, counters: Vec<PerfCounterDefinitionTemplate>) -> Self {
            Self {
                timer: ZeroTimeProvider,
                objects,
                counters,
            }
        }
    }

    impl PerfProvider for BasicPerfProvider {
        fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str {
            unimplemented!()
        }

        fn first_counter(&self, for_object: &PerfObjectTypeTemplate) -> WinResult<u32> {
            Ok(0)
        }

        fn objects(&self) -> &[PerfObjectTypeTemplate] {
            self.objects.as_ref()
        }

        fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider {
            &self.timer
        }

        fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate] {
            self.counters.as_ref()
        }

        fn instances(&self, for_object: &PerfObjectTypeTemplate) -> Option<&[PerfInstanceDefinitionTemplate]> {
            None
        }

        fn data(&self, for_object: &PerfObjectTypeTemplate, per_counter: &PerfCounterDefinitionTemplate, per_instance: Option<&PerfInstanceDefinitionTemplate>, now: PerfClock) -> CounterValue {
            match per_counter.counter_type.size() {
                Size::Dword => CounterValue::Dword(42),
                Size::Large => CounterValue::Large(37),
                _ => unimplemented!()
            }
        }
    }

    #[test]
    fn test_no_instances() {
        let provider = BasicPerfProvider::new(
            vec![PerfObjectTypeTemplate::new(0)],
            vec![PerfCounterDefinitionTemplate::new(2, unsafe { CounterTypeDefinition::from_raw_unchecked(PERF_COUNTER_RAWCOUNT) })],
        );
        let mut buffer = vec![0u8; 10 * 1024];
        let collected = provider.collect(QueryType::Global, buffer.as_mut_slice()).unwrap();

        assert_eq!(collected.num_object_types, 1);
        let buffer_slice = &buffer[..collected.total_bytes];

        let (rest, obj) = crate::perf::nom::perf_object_type(buffer_slice).unwrap();
        assert_eq!(rest, &b""[..]);
        assert_eq!(obj.counters[0].CounterNameTitleIndex, 2);
    }
}
