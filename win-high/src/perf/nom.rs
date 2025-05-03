//! Nom parsers for performance data structures
use core::num::NonZeroUsize;
use std::mem;

use nom::{Err, IResult, Needed, Parser};

use crate::prelude::v2::*;

#[derive(Clone)]
pub struct PerfDataBlock {
    pub raw: PERF_DATA_BLOCK,
    pub system_name: U16CString,
    pub object_types: Vec<PerfObjectType>,
}

#[derive(Clone)]
pub struct PerfObjectType {
    pub raw: PERF_OBJECT_TYPE,
    pub counters: Vec<PerfCounterDefinition>,
    pub data: PerfObjectData,
}

#[derive(Clone)]
pub struct PerfCounterDefinition {
    pub raw: PERF_COUNTER_DEFINITION,
}

#[derive(Clone)]
pub struct PerfInstanceDefinition {
    pub raw: PERF_INSTANCE_DEFINITION,
    pub name: U16CString,
}

#[derive(Clone)]
pub struct PerfCounterBlock {
    // pub raw: PERF_COUNTER_BLOCK,
    pub payload: Vec<u8>,
}

/// This is an extension to support both global and multi-instance counters.
#[derive(Clone)]
pub enum PerfObjectData {
    Singleton(PerfCounterBlock),
    Instances(Vec<(PerfInstanceDefinition, PerfCounterBlock)>),
}

impl PerfObjectData {
    pub fn singleton(&self) -> Option<&PerfCounterBlock> {
        match self {
            Self::Singleton(block) => Some(block),
            Self::Instances(..) => None,
        }
    }

    pub fn instances(&self) -> Option<&[(PerfInstanceDefinition, PerfCounterBlock)]> {
        match self {
            Self::Singleton(..) => None,
            Self::Instances(vec) => Some(vec),
        }
    }
}

impl PerfCounterBlock {
    pub fn len(&self) -> usize {
        self.payload.len()
    }

    /// Get data of this counter block as a byte slice.
    pub fn data(&self) -> &[u8] {
        self.payload.as_slice()
    }
}

pub fn perf_data_block(input: &[u8]) -> IResult<&[u8], PerfDataBlock> {
    let (_, raw) = take_struct::<PERF_DATA_BLOCK>(input)?;
    // it is important to use whole input slice, because offsets are calculated relative to the
    // beginning of the PERF_DATA_BLOCK.
    let (_, system_name) = u16cstr(input, raw.SystemNameOffset, raw.SystemNameLength)?;
    // after HeaderLength bytes starts an array of NumObjectTypes x PERF_OBJECT_TYPE blocks.
    // we could just skip the size of struct, but trusting HeaderLength is more future-proof.
    // again, counting from the beginning of the whole input slice.
    let n = raw.NumObjectTypes as usize;
    let (i1, _) = nom::bytes::complete::take(raw.HeaderLength)(input)?;
    let (_, object_types) = nom::multi::many_m_n(n, n, perf_object_type).parse(i1)?;
    // yet again, skipping TotalByteLength from the beginning the whole input slice.
    let (rest, _) = nom::bytes::complete::take(raw.TotalByteLength)(input)?;
    Ok((
        rest,
        PerfDataBlock {
            raw,
            system_name,
            object_types,
        },
    ))
}

pub fn perf_object_type(input: &[u8]) -> IResult<&[u8], PerfObjectType> {
    let (_, raw) = take_struct::<PERF_OBJECT_TYPE>(input)?;
    // counter definitions block starts right at HeaderLength offset.
    let (_, counters) = {
        let n = raw.NumCounters as usize;
        let (i1, _) = nom::bytes::complete::take(raw.HeaderLength)(input)?;
        nom::multi::many_m_n(n, n, perf_counter_definition).parse(i1)?
    };
    // after DefinitionLength bytes, comes counters' data.
    // depending of NumInstances, it is either:
    //  - single PERF_COUNTER_BLOCK; or
    //  - (PERF_INSTANCE_DEFINITION, PERF_COUNTER_BLOCK) adjacent pairs of blocks.
    let (i2, _) = nom::bytes::complete::take(raw.DefinitionLength)(input)?;
    let data = if raw.NumInstances == PERF_NO_INSTANCES {
        let (_, block) = perf_counter_block(i2)?;
        PerfObjectData::Singleton(block)
    } else {
        let n = raw.NumInstances as usize;
        let (_, pairs) = nom::multi::many_m_n(
            n,
            n,
            nom::sequence::pair(perf_instance_definition, perf_counter_block),
        )
        .parse(i2)?;
        PerfObjectData::Instances(pairs)
    };
    let (rest, _) = nom::bytes::complete::take(raw.TotalByteLength)(input)?;
    Ok((
        rest,
        PerfObjectType {
            raw,
            counters,
            data,
        },
    ))
}

pub fn perf_counter_definition(input: &[u8]) -> IResult<&[u8], PerfCounterDefinition> {
    nom::combinator::map(take_struct::<PERF_COUNTER_DEFINITION>, |raw| {
        PerfCounterDefinition { raw }
    })
    .parse(input)
}

pub fn perf_instance_definition(input: &[u8]) -> IResult<&[u8], PerfInstanceDefinition> {
    let (_, raw) = take_struct::<PERF_INSTANCE_DEFINITION>(input)?;
    // same as perf_data_block: offset is from the beginning of the input.
    let (_, name) = u16cstr(input, raw.NameOffset, raw.NameLength)?;
    let (rest, _) = nom::bytes::complete::take(raw.ByteLength)(input)?;
    Ok((rest, PerfInstanceDefinition { raw, name }))
}

pub fn perf_counter_block(input: &[u8]) -> IResult<&[u8], PerfCounterBlock> {
    let (_, raw) = take_struct::<PERF_COUNTER_BLOCK>(input)?;
    // ensure that length of input is large enough
    let (rest, payload) = nom::bytes::complete::take(raw.ByteLength)(input)?;
    Ok((
        rest,
        PerfCounterBlock {
            payload: payload.to_vec(),
        },
    ))
}

pub fn take_struct<T>(input: &[u8]) -> nom::IResult<&[u8], T>
where
    T: Sized,
{
    // SAFETY: `take` ensures there is enough bytes in `T` slice to read it as an `T`.
    nom::combinator::map(
        nom::bytes::complete::take(mem::size_of::<T>()),
        |data: &[u8]| unsafe { (data.as_ptr() as *const T).read_unaligned() },
    )
    .parse(input)
}

pub fn u16cstr(input: &[u8], offset: u32, len: u32) -> IResult<&[u8], U16CString> {
    debug_assert!(len % 2 == 0);
    let (i1, _) = ::nom::bytes::complete::take(offset as usize)(input)?;
    let (i2, u8slice) = ::nom::bytes::complete::take(len as usize)(i1)?;

    let mut buf = Vec::<u16>::with_capacity(len as usize / 2);
    for _ in 0..(len as usize / 2) {
        buf.push(0u16);
    }
    // u8-aligned read
    unsafe { downcast_mut(buf.as_mut_slice()) }.copy_from_slice(u8slice);
    let u16cstr = U16CString::from_vec_truncate(buf);
    Ok((i2, u16cstr))
}

fn no_zst<T>() {
    if mem::size_of::<T>() == 0 {
        panic!("ZST are not allowed here");
    }
}

pub unsafe fn downcast<T>(input: &[T]) -> &[u8] {
    unsafe {
        no_zst::<T>();
        let len = input.len() * mem::size_of::<T>();
        std::slice::from_raw_parts(input.as_ptr().cast(), len)
    }
}

pub unsafe fn downcast_mut<T>(input: &mut [T]) -> &mut [u8] {
    unsafe {
        no_zst::<T>();
        let len = input.len() * mem::size_of::<T>();
        std::slice::from_raw_parts_mut(input.as_mut_ptr().cast(), len)
    }
}

/// Error value is the remainder of a division of length by size of `T`.
/// Returned raw slice may be unaligned for reading.
pub unsafe fn upcast<T>(input: &[u8]) -> Result<*const [T], NonZeroUsize> {
    no_zst::<T>();
    let len = input.len() / mem::size_of::<T>();
    let rem = input.len() % mem::size_of::<T>();
    match NonZeroUsize::new(rem) {
        Some(rem) => Err(rem),
        None => Ok(std::ptr::slice_from_raw_parts(input.as_ptr().cast(), len)),
    }
}

/// Error value is the remainder of a division of length by size of `T`.
/// Returned raw slice may be unaligned for reading.
pub unsafe fn upcast_mut<T>(input: &mut [u8]) -> Result<*mut [T], NonZeroUsize> {
    no_zst::<T>();
    let len = input.len() / mem::size_of::<T>();
    let rem = input.len() % mem::size_of::<T>();
    match NonZeroUsize::new(rem) {
        Some(rem) => Err(rem),
        None => Ok(std::ptr::slice_from_raw_parts_mut(
            input.as_mut_ptr().cast(),
            len,
        )),
    }
}

/// Consumes all the input, transmutes input as a raw slice of `T`.
/// On success, output of parser will be empty.
///
/// SAFETY: this function ensures that input length is divisible by size of `T`,
/// but otherwise the semantics of achieved result depends on the actual `T` type.
/// Returned raw slice may be unaligned for reading.
pub unsafe fn view<T>(input: &[u8]) -> IResult<&[u8], *const [T]> {
    unsafe {
        let (empty, i1) = nom::bytes::complete::take(input.len())(input)?;
        debug_assert!(empty.is_empty());
        let slice_t = upcast::<T>(i1)
            .map_err(|rem| Err::Incomplete(Needed::new(mem::size_of::<T>() - rem.get())))?;
        Ok((empty, slice_t))
    }
}

mod imp_deref {
    use std::ops::Deref;

    use super::*;

    impl Deref for PerfDataBlock {
        type Target = PERF_DATA_BLOCK;

        fn deref(&self) -> &Self::Target {
            &self.raw
        }
    }

    impl Deref for PerfObjectType {
        type Target = PERF_OBJECT_TYPE;

        fn deref(&self) -> &Self::Target {
            &self.raw
        }
    }

    impl Deref for PerfCounterDefinition {
        type Target = PERF_COUNTER_DEFINITION;

        fn deref(&self) -> &Self::Target {
            &self.raw
        }
    }

    impl Deref for PerfInstanceDefinition {
        type Target = PERF_INSTANCE_DEFINITION;

        fn deref(&self) -> &Self::Target {
            &self.raw
        }
    }
}

#[cfg(test)]
mod test {
    use super::super::values::*;
    use super::*;

    const SAMPLE_SYSTEM: &'static [u8] = include_bytes!("test/sample_system_perf_data_block.bin");
    const SAMPLE_PROCESSOR: &'static [u8] =
        include_bytes!("test/sample_processor_perf_data_block.bin");

    // singleton object test
    #[test]
    fn test_system_object() {
        let (rest, data_block) = perf_data_block(SAMPLE_SYSTEM).expect("parse data block");
        assert_eq!(rest, &b""[..]); // basically is_empty(), but shows the diff
        assert_eq!(data_block.system_name.to_string_lossy(), "GETAWAY");
        assert_eq!(data_block.object_types.len(), 1);
        let obj = &data_block.object_types[0];
        assert_eq!(obj.raw.NumCounters, 18);
        match &obj.data {
            PerfObjectData::Singleton(block) => {
                let processes_counter = obj
                    .counters
                    .iter()
                    .find(|c| c.raw.CounterNameTitleIndex == 248)
                    .expect("Processes counter");
                let res = CounterValue::try_get(processes_counter, block);
                assert_eq!(res, Ok(CounterValue::Dword(201)));
            }
            _ => panic!("should be an object without instances"),
        }
    }

    // multi-instance object test
    #[test]
    fn test_processor_object() {
        let (rest, data_block) = perf_data_block(SAMPLE_PROCESSOR).expect("parse data block");
        assert_eq!(rest, &b""[..]); // basically is_empty(), but shows the diff
        assert_eq!(data_block.system_name.to_string_lossy(), "GETAWAY");
        assert_eq!(data_block.object_types.len(), 1);
        let obj = &data_block.object_types[0];

        match &obj.data {
            PerfObjectData::Instances(pairs) => {
                // 8 processor cores plus a special '_Total' instance == 9
                assert_eq!(pairs.len(), 9);
                let instance_total = &pairs.last().unwrap().0;
                assert_eq!(instance_total.name.to_string_lossy(), "_Total");
            }
            _ => panic!("should have instances"),
        }
    }
}
