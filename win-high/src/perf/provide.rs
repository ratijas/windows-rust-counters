//! Types and functions for counter data providers
#![allow(non_snake_case)]

use std::mem::{align_of, size_of};
use std::str::FromStr;
use std::cell::RefCell;
use std::borrow::{Borrow, Cow};

use crate::perf::nom::*;
use crate::perf::types::*;
use crate::perf::values::CounterVal;
use crate::prelude::v2::*;

pub trait PerfProvider {
    fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str;

    fn first_counter(&self, for_object: &PerfObjectTypeTemplate) -> WinResult<u32> {
        let sub_key = format!(r"SYSTEM\CurrentControlSet\Services\{}\Performance", self.service_name(for_object));
        let sub_key_wstr = U16CString::from_str(sub_key).map_err(|_| WinError::new(ERROR_INVALID_DATA))?;
        let hkey = RegOpenKeyEx_Safe(
            HKEY_LOCAL_MACHINE,
            PCWSTR(sub_key_wstr.as_ptr()),
            None,
            KEY_READ,
        )?;
        let buffer = query_value(
            *hkey,
            "First Counter",
            None,
            None,
        )?;
        let first_counter = nom::number::complete::le_u32::<_, ()>(&*buffer)
            .map_err(|_| WinError::new(ERROR_INVALID_DATA))?
            .1;

        Ok(first_counter)
    }

    fn objects(&self) -> &[PerfObjectTypeTemplate];

    fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider;

    fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate];

    fn instances<'a>(&'a self, for_object: &PerfObjectTypeTemplate) -> Option<Vec<PerfInstanceDefinitionTemplate<'a>>>;

    fn data<'a>(
        &'a self,
        for_object: &PerfObjectTypeTemplate,
        per_counter: &PerfCounterDefinitionTemplate,
        per_instance: Option<&PerfInstanceDefinitionTemplate<'a>>,
        now: PerfClock,
    ) -> CounterVal<'a>;

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

    /// Callback before `collect()`.
    fn prepare(&mut self) {}

    /// Callback after `collect()`.
    fn finish(&mut self) {}

    fn collect_object<'a>(
        &self,
        object_template: &PerfObjectTypeTemplate,
        now: PerfClock,
        buffer: &'a mut [u8],
    ) -> WinResult<(usize, &'a mut [u8])> {
        // build all headers and definitions before writing anything.
        // counters data will be requested at the very last step, when it is really needed.
        let first_counter = self.first_counter(object_template)?;
        let counter_templates = self.counters(object_template);
        let counters_block_template = layout_of_counters(first_counter, counter_templates);
        let counters = counters_block_template.counters();
        let counters_layout = counters_block_template.build_layout();
        let instance_templates = self.instances(object_template);
        let instances = instance_templates.as_ref().map(Vec::as_ref).map(layout_of_instances);
        let object = object_template.build_layout(
            first_counter,
            counters,
            instances.as_ref().map(Vec::as_ref),
            &counters_layout,
            now,
        );

        // make sure we won't go past the limit.
        let mut buf = buffer.get_mut(..object.TotalByteLength as usize)
            .ok_or(()).map_err(error_small_buffer)?;

        // write header
        buf = write_object_type_header(&object, buf).map_err(error_internal)?;

        // write counters definitions
        for counter in counters {
            buf = write_counter_definition(counter, buf).map_err(error_internal)?;
        }

        // write data
        if instances.is_none() {
            // no instances, single block
            buf = self.write_block(
                object_template,
                None,
                counter_templates,
                &counters_block_template,
                now,
                buf,
            ).map_err(error_internal)?;
            // suppress 'unused' warning
            let _ = buf;
        } else {
            let instance_templates = instance_templates.unwrap();
            let instances = instances.unwrap();
            for (instance, instance_template) in instances.iter().zip(instance_templates.iter()) {
                // write instance
                buf = instance_template.write_with_layout(instance, buf).map_err(error_internal)?;
                // write block
                buf = self.write_block(
                    object_template,
                    Some(instance_template),
                    counter_templates,
                    &counters_block_template,
                    now,
                    buf,
                ).map_err(error_internal)?;
            }
        }
        // if we got past the line above, then it is save to slice from TotalByteLength onward.
        let rest = &mut buffer[object.TotalByteLength as usize..];
        Ok((object.TotalByteLength as _, rest))
    }

    fn write_block<'a>(
        &self,
        object_template: &PerfObjectTypeTemplate,
        instance_template: Option<&PerfInstanceDefinitionTemplate<'a>>,
        counter_templates: &[PerfCounterDefinitionTemplate],
        counters_block_template: &CountersBlockTemplate,
        now: PerfClock,
        buffer: &'a mut [u8],
    ) -> Result<&'a mut [u8], ()>
    {
        let counters = counters_block_template.counters();
        let mut block = counters_block_template.block(buffer)?;
        for (counter, counter_template) in counters.iter().zip(counter_templates) {
            let value = self.data(object_template, counter_template, instance_template, now);
            block.write(counter, value)?;
        }
        let len = block.len();
        buffer.get_mut(len..).ok_or(())
    }

    fn collect(&mut self, query: QueryType, mut buffer: &mut [u8]) -> WinResult<Collected> {
        self.prepare();

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

        self.finish();

        Ok(Collected {
            total_bytes,
            num_object_types,
        })
    }
}

pub struct CachingPerfProvider<X> {
    inner: X,
    first_counters: RefCell<Vec<(u32, u32)>>,
}

impl<X> CachingPerfProvider<X> {
    pub fn new(inner: X) -> Self {
        Self {
            inner,
            first_counters: RefCell::new(vec![]),
        }
    }

    fn lookup_first_counter(&self, name_offset: u32) -> Option<u32> {
        self.first_counters.borrow().iter()
            .find_map(|&(offset, base)| if offset == name_offset { Some(base) } else { None })
    }

    fn cache_first_counter(&self, name_offset: u32, first_counter: u32) {
        self.first_counters.borrow_mut().push((name_offset, first_counter));
    }
}

impl<X: PerfProvider> PerfProvider for CachingPerfProvider<X> {
    // pass through implementation
    fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str {
        self.inner.service_name(for_object)
    }

    // custom method
    fn first_counter(&self, for_object: &PerfObjectTypeTemplate) -> WinResult<u32> {
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

    fn instances<'a>(&'a self, for_object: &PerfObjectTypeTemplate) -> Option<Vec<PerfInstanceDefinitionTemplate<'a>>> {
        self.inner.instances(for_object)
    }

    fn data<'a>(
        &'a self,
        for_object: &PerfObjectTypeTemplate,
        per_counter: &PerfCounterDefinitionTemplate,
        per_instance: Option<&PerfInstanceDefinitionTemplate<'a>>,
        now: PerfClock,
    ) -> CounterVal<'a> {
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
    PerfTime: i64,
    PerfFreq: i64,
}

pub trait PerfTimeProvider {
    fn get_time(&self) -> PerfClock;
}

pub struct ZeroTimeProvider;

impl PerfTimeProvider for ZeroTimeProvider {
    fn get_time(&self) -> PerfClock {
        PerfClock {
            PerfTime: 0,
            PerfFreq: 0,
        }
    }
}

#[derive(Debug)]
pub struct TickCountTimeProvider;

impl PerfTimeProvider for TickCountTimeProvider {
    fn get_time(&self) -> PerfClock {
        // number of milliseconds that have elapsed since the system was started
        let millis = unsafe { GetTickCount() };
        PerfClock {
            PerfTime: millis as _,
            PerfFreq: 1_000,
        }
    }
}

#[derive(Debug)]
pub struct PerfObjectTypeTemplate {
    pub name_offset: u32,
    pub help_offset: u32,
    pub detail_level: DetailLevel,
    pub DefaultCounter: i32,
}

impl PerfObjectTypeTemplate {
    pub fn new(
        name_offset: u32,
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

    pub fn with_default_counter(mut self, default_counter: i32) -> Self {
        self.DefaultCounter = default_counter;
        self
    }

    pub fn build_layout(
        &self,
        first_counter: u32,
        counters: &[PERF_COUNTER_DEFINITION],
        instances: Option<&[PERF_INSTANCE_DEFINITION]>,
        block: &PERF_COUNTER_BLOCK,
        now: PerfClock,
    ) -> PERF_OBJECT_TYPE
    {
        let counters_length: usize = counters.iter().map(|c| c.ByteLength as usize).sum();
        let instances_length: usize = instances.map(|it| it.iter().map(|i| i.ByteLength as usize).sum()).unwrap_or(0);
        // no instances == 1 block
        let blocks_length: usize = instances.map(|it| it.len()).unwrap_or(1) * (block.ByteLength as usize);

        let header_length = size_of::<PERF_OBJECT_TYPE>();
        let definition_length = header_length + counters_length;
        let total = align_to::<PERF_OBJECT_TYPE>(definition_length + instances_length + blocks_length);

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
}

fn write_object_type_header<'a>(object: &PERF_OBJECT_TYPE, buffer: &'a mut [u8]) -> Result<&'a mut [u8], ()> {
    unsafe { copy_struct_into_buffer(object, buffer)? };
    buffer.get_mut(object.HeaderLength as usize..).ok_or(())
}

#[derive(Copy, Clone, Debug)]
pub struct PerfCounterDefinitionTemplate {
    pub name_offset: u32,
    pub help_offset: u32,
    pub DefaultScale: i32,
    pub DetailLevel: DetailLevel,
    pub counter_type: CounterTypeDefinition,
    pub CounterSize: Option<u32>,
}

impl PerfCounterDefinitionTemplate {
    pub fn new(
        name_offset: u32,
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

    pub fn with_default_scale(mut self, scale: i32) -> Self {
        self.DefaultScale = scale;
        self
    }

    pub fn with_detail_level(mut self, detail_level: DetailLevel) -> Self {
        self.DetailLevel = detail_level as _;
        self
    }

    pub fn with_size(&mut self, size: u32) -> &mut Self {
        self.CounterSize = Some(size);
        self
    }

    pub fn build_layout(&self, first_counter: u32, offset: u32) -> PERF_COUNTER_DEFINITION {
        let counter_size = align_to_dword(
            self.CounterSize
                .expect("Cannot infer counter size. Set it manually via PerfCounterDefinitionTemplate::with_size() method.")
                as _
        );
        PERF_COUNTER_DEFINITION {
            ByteLength: size_of::<PERF_COUNTER_DEFINITION>() as _,
            CounterNameTitleIndex: self.name_offset + first_counter,
            CounterNameTitle: 0,
            CounterHelpTitleIndex: self.help_offset + first_counter,
            CounterHelpTitle: 0,
            DefaultScale: self.DefaultScale,
            DetailLevel: self.DetailLevel as _,
            CounterType: self.counter_type.into_raw(),
            CounterSize: counter_size as _,
            CounterOffset: offset as _,
        }
    }
}

fn write_counter_definition<'a>(counter: &PERF_COUNTER_DEFINITION, buffer: &'a mut [u8]) -> Result<&'a mut [u8], ()> {
    unsafe { copy_struct_into_buffer(counter, buffer)? };
    buffer.get_mut(counter.ByteLength as usize..).ok_or(())
}

#[derive(Debug)]
pub struct PerfInstanceDefinitionTemplate<'a> {
    pub ParentObjectTitleIndex: u32,
    pub ParentObjectInstance: u32,
    pub UniqueID: i32,
    pub Name: Cow<'a, U16CStr>,
}

impl<'a> PerfInstanceDefinitionTemplate<'a> {
    pub fn new(Name: Cow<'a, U16CStr>) -> Self {
        PerfInstanceDefinitionTemplate {
            ParentObjectTitleIndex: 0,
            ParentObjectInstance: 0,
            UniqueID: PERF_NO_UNIQUE_ID,
            Name,
        }
    }

    pub fn into_owned(self) -> PerfInstanceDefinitionTemplate<'static> {
        PerfInstanceDefinitionTemplate {
            Name: Cow::from(self.Name.into_owned()),
            ..self
        }
    }

    pub fn with_unique_id(mut self, unique_id: i32) -> Self {
        self.UniqueID = unique_id;
        self
    }

    pub fn with_parent(
        mut self,
        ParentObjectTitleIndex: u32,
        ParentObjectInstance: u32,
    ) -> Self {
        self.ParentObjectTitleIndex = ParentObjectTitleIndex;
        self.ParentObjectInstance = ParentObjectInstance;
        self
    }

    pub fn build_layout(&self) -> PERF_INSTANCE_DEFINITION {
        let struct_size = size_of::<PERF_INSTANCE_DEFINITION>();
        // including nul terminator
        let name_size = self.Name.as_slice_with_nul().len() * size_of::<u16>();

        PERF_INSTANCE_DEFINITION {
            ByteLength: align_to::<PERF_INSTANCE_DEFINITION>(struct_size + name_size) as _,
            ParentObjectTitleIndex: self.ParentObjectTitleIndex,
            ParentObjectInstance: self.ParentObjectInstance,
            UniqueID: self.UniqueID,
            NameOffset: struct_size as _,
            NameLength: name_size as _,
        }
    }

    /// Returns the rest of the buffer after this object
    pub fn write_with_layout<'b>(&self, def: &PERF_INSTANCE_DEFINITION, buffer: &'b mut [u8]) -> Result<&'b mut [u8], ()> {
        unsafe {
            copy_struct_into_buffer(def, buffer)?;
            copy_cstr_into_buffer(self.Name.borrow(), buffer, def.NameOffset, def.NameLength)?;
        }
        buffer.get_mut(def.ByteLength as usize..).ok_or(())
    }
}

pub struct CountersBlockTemplate {
    ByteLength: u32,
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
        self.ByteLength += align_to_dword(counter.CounterSize as _) as u32;
        self.counters.push(counter);
    }

    pub fn offset(&self) -> u32 {
        self.ByteLength
    }

    pub fn counters(&self) -> &[PERF_COUNTER_DEFINITION] {
        &*self.counters
    }

    pub fn build_layout(&self) -> PERF_COUNTER_BLOCK {
        PERF_COUNTER_BLOCK {
            ByteLength: self.ByteLength
        }
    }

    pub fn block<'a>(&self, buffer: &'a mut [u8]) -> Result<CountersBlockBuffer<'a>, ()> {
        let slice = buffer.get_mut(..self.ByteLength as usize).ok_or(())?;
        unsafe { copy_struct_into_buffer(&self.build_layout(), slice)? };
        Ok(CountersBlockBuffer { buffer: slice })
    }
}

pub struct CountersBlockBuffer<'a> {
    buffer: &'a mut [u8],
}

impl<'a> CountersBlockBuffer<'a> {
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn write<'b>(&mut self, def: &PERF_COUNTER_DEFINITION, value: CounterVal<'b>) -> Result<(), ()> {
        let offset = def.CounterOffset as usize;
        let length = def.CounterSize as usize;
        let slice = self.buffer.get_mut(offset..offset + length).ok_or(())?;

        value.write(slice).map_err(drop)
    }
}

#[derive(Clone, Debug)]
pub enum QueryType {
    Global,
    Costly,
    Foreign,
    Items(Vec<u32>),
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
                    .map(|item| item.parse::<u32>().map_err(drop))
                    .collect::<Result<Vec<_>, _>>()?
            )
        })
    }
}

/// Bump the `size` up to be a closest multiple of given type's alignment.
/// No-op if the `size` is already divisible by alignment of type.
fn align_to<T>(size: usize) -> usize {
    let align = align_of::<T>();
    assert_ne!(align, 0);
    ((size + align - 1) / align) * align
}

/// See [`align_to`](fn.align_to.html).
fn align_to_dword(size: usize) -> usize {
    align_to::<u32>(size)
}

fn layout_of_counters(first_counter: u32, templates: &[PerfCounterDefinitionTemplate]) -> CountersBlockTemplate {
    let mut block = CountersBlockTemplate::new();
    for template in templates {
        let counter = template.build_layout(first_counter, block.offset());
        block.add_counter(counter);
    }
    block
}

fn layout_of_instances(templates: &[PerfInstanceDefinitionTemplate<'_>]) -> Vec<PERF_INSTANCE_DEFINITION> {
    templates
        .iter()
        .map(|t| t.build_layout())
        .collect()
}

fn error_small_buffer(_: ()) -> WinError {
    WinError::new_with_message(ERROR_MORE_DATA)
}

fn error_internal(_: ()) -> WinError {
    // any other ideas for an error code?
    WinError::new_with_message(ERROR_ACCESS_DENIED)
}

unsafe fn copy_struct_into_buffer<'a, T>(source: &T, buffer: &'a mut [u8]) -> Result<&'a mut [u8], ()> { unsafe {
    let size = size_of::<T>();
    let slice_u8 = buffer.get_mut(..size).ok_or(())?;
    slice_u8.as_mut_ptr().cast::<T>().copy_from_nonoverlapping(source as *const _, 1);
    buffer.get_mut(size..).ok_or(())
}}

unsafe fn copy_cstr_into_buffer(str: &U16CStr, buffer: &mut [u8], offset: u32, length: u32) -> Result<(), ()> { unsafe {
    let offset = offset as usize;
    let length = length as usize;
    let slice_u8 = buffer.get_mut(offset..offset + length).ok_or(())?;
    let str_u8 = downcast(str.as_slice_with_nul());
    if slice_u8.len() != str_u8.len() {
        return Err(());
    }
    slice_u8.copy_from_slice(str_u8);
    Ok(())
}}

#[cfg(test)]
mod test {
    use super::*;

    use win_low::um::winperf::*;

    struct BasicPerfProvider {
        timer: ZeroTimeProvider,
        objects: Vec<PerfObjectTypeTemplate>,
        counters: Vec<PerfCounterDefinitionTemplate>,
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
        fn service_name(&self, _for_object: &PerfObjectTypeTemplate) -> &str {
            unimplemented!()
        }

        fn first_counter(&self, _for_object: &PerfObjectTypeTemplate) -> WinResult<u32> {
            Ok(0)
        }

        fn objects(&self) -> &[PerfObjectTypeTemplate] {
            self.objects.as_ref()
        }

        fn time_provider(&self, _for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider {
            &self.timer
        }

        fn counters(&self, _for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate] {
            self.counters.as_ref()
        }

        fn instances<'a>(&'a self, _for_object: &PerfObjectTypeTemplate) -> Option<Vec<PerfInstanceDefinitionTemplate<'a>>> {
            None
        }

        fn data<'a>(
            &'a self,
            _for_object: &PerfObjectTypeTemplate,
            per_counter: &PerfCounterDefinitionTemplate,
            _per_instance: Option<&PerfInstanceDefinitionTemplate<'a>>,
            _now: PerfClock,
        ) -> CounterVal<'a> {
            match per_counter.counter_type.size() {
                Size::Dword => CounterVal::Dword(42),
                Size::Large => CounterVal::Large(37),
                _ => unimplemented!()
            }
        }
    }

    #[test]
    fn test_align() {
        assert_eq!(align_of::<u32>(), size_of::<u32>());
        assert_eq!(align_to_dword(0), 0);
        // smallest
        assert_eq!(align_to_dword(1), size_of::<u32>());
        // identity
        assert_eq!(align_to_dword(size_of::<u32>()), size_of::<u32>());
        // smallest on a bigger type
        assert_eq!(align_to::<PERF_OBJECT_TYPE>(1), 8);
    }

    #[test]
    fn test_no_instances() {
        let mut provider = BasicPerfProvider::new(
            vec![PerfObjectTypeTemplate::new(0)],
            vec![PerfCounterDefinitionTemplate::new(2, unsafe { CounterTypeDefinition::from_raw_unchecked(PERF_COUNTER_RAWCOUNT) })],
        );
        let mut buffer = vec![0u8; 10 * 1024];
        let collected = provider.collect(QueryType::Global, buffer.as_mut_slice()).unwrap();
        assert_eq!(collected.num_object_types, 1);
        let buffer_slice = &buffer[..collected.total_bytes];

        let (rest, obj) = crate::perf::nom::perf_object_type(buffer_slice).unwrap();
        assert_eq!(rest, &b""[..]);
        assert_eq!(obj.counters.len(), 1);
        let counter = &obj.counters[0];
        assert_eq!(counter.CounterNameTitleIndex, 2);
    }

    struct InstancesPerfProvider {
        timer: ZeroTimeProvider,
        objects: Vec<PerfObjectTypeTemplate>,
        counters: Vec<PerfCounterDefinitionTemplate>,
        instances: Vec<String>,
    }

    #[allow(unused_variables)]
    impl PerfProvider for InstancesPerfProvider {
        fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str {
            unimplemented!()
        }

        fn first_counter(&self, for_object: &PerfObjectTypeTemplate) -> WinResult<u32> {
            Ok(0)
        }

        fn objects(&self) -> &[PerfObjectTypeTemplate] {
            &self.objects
        }

        fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider {
            &self.timer
        }

        fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate] {
            &self.counters
        }

        fn instances<'a>(&'a self, for_object: &PerfObjectTypeTemplate) -> Option<Vec<PerfInstanceDefinitionTemplate<'a>>> {
            Some(self.instances.iter().enumerate().map(|(id, name)| {
                PerfInstanceDefinitionTemplate::new(
                    Cow::from(
                        U16CString::from_str(name).unwrap()
                    )
                ).with_unique_id(id as _)
            }).collect())
        }

        fn data<'a>(
            &'a self,
            for_object: &PerfObjectTypeTemplate,
            per_counter: &PerfCounterDefinitionTemplate,
            per_instance: Option<&PerfInstanceDefinitionTemplate<'a>>,
            now: PerfClock,
        ) -> CounterVal<'a> {
            match per_instance {
                Some(instance) => CounterVal::Dword((2 * instance.UniqueID) as _),
                None => CounterVal::Dword(42),
            }
        }
    }

    #[test]
    fn test_instances() {
        let mut provider = InstancesPerfProvider {
            timer: ZeroTimeProvider,
            objects: vec![PerfObjectTypeTemplate::new(0)],
            counters: vec![PerfCounterDefinitionTemplate::new(2, unsafe { CounterTypeDefinition::from_raw_unchecked(PERF_COUNTER_RAWCOUNT) })],
            instances: vec!["first".to_string(), "second".to_string()],
        };

        let mut buffer = vec![0u8; 10 * 1024];
        let collected = provider.collect(QueryType::Global, buffer.as_mut_slice()).unwrap();

        assert_eq!(collected.num_object_types, 1);
        let buffer_slice = &buffer[..collected.total_bytes];

        let (rest, obj) = crate::perf::nom::perf_object_type(buffer_slice).unwrap();
        assert_eq!(rest, &b""[..]);
        assert_eq!(obj.counters.len(), 1);
        let counter = &obj.counters[0];
        assert_eq!(counter.CounterNameTitleIndex, 2);

        assert!(matches!(&obj.data, PerfObjectData::Instances(..)));
        let instances = obj.data.instances().unwrap();
        assert_eq!(instances.len(), 2);
        for (instance, block) in instances {
            match instance.UniqueID {
                0 => {
                    assert_eq!(instance.name.to_string_lossy(), "first");
                    assert_eq!(CounterVal::try_get(counter, block).unwrap(), CounterVal::Dword(0));
                }
                1 => {
                    assert_eq!(instance.name.to_string_lossy(), "second");
                    assert_eq!(CounterVal::try_get(counter, block).unwrap(), CounterVal::Dword(2));
                }
                _ => unreachable!(),
            }
        }
    }
}
