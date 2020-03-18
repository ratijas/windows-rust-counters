use std::convert::TryFrom;

use crate::perf::nom::*;
use crate::perf::types::*;
use crate::prelude::v1::*;

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum CounterValue<'a> {
    Dword(DWORD),
    Large(ULONGLONG),
    TextUnicode(&'a U16CStr),
    TextAscii(&'a str),
    Zero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueError {
    BadSize,
    NoData,
    StringFormat,
    UnknownType,
}

impl<'b> CounterValue<'b> {
    pub fn try_get<'a>(def: &'a PerfCounterDefinition, block: &'b PerfCounterBlock) -> Result<Self, ValueError> {
        get_value(def, block)
    }
}

pub fn get_slice<'a, 'b>(def: &'a PerfCounterDefinition, block: &'b PerfCounterBlock) -> Option<&'b [u8]> {
    let len = def.raw.CounterSize as usize;
    let offset = def.raw.CounterOffset as usize;
    block.data().get(offset..offset + len)
}

fn get_value<'a, 'b>(def: &'a PerfCounterDefinition, block: &'b PerfCounterBlock) -> Result<CounterValue<'b>, ValueError> {
    let typ = CounterTypeDefinition::try_from(def).expect("counter");
    let mut slice = get_slice(def, block).ok_or(ValueError::BadSize)?;
    let value = unsafe {
        match typ.size() {
            Size::Dword => {
                let number = upcast::<DWORD>(slice).map_err(|_| ValueError::BadSize)?
                    .get(0).ok_or(ValueError::NoData)?.clone();
                CounterValue::Dword(number)
            }
            Size::Large => {
                let number = upcast::<ULONGLONG>(slice).map_err(|_| ValueError::BadSize)?
                    .get(0).ok_or(ValueError::NoData)?.clone();
                CounterValue::Large(number)
            }
            Size::Zero => CounterValue::Zero,
            Size::Var => {
                if let CounterType::Text(encoding) = typ.counter_type() {
                    match encoding {
                        Text::Unicode => {
                            let u16slice = upcast::<u16>(slice).map_err(|_| ValueError::BadSize)?;
                            let text = U16CStr::from_slice_with_nul(u16slice).map_err(|_| ValueError::StringFormat)?;
                            CounterValue::TextUnicode(text)
                        }
                        Text::Ascii => {
                            // is there slice.trim method?
                            while slice.ends_with(&[0u8]) {
                                slice = &slice[..slice.len() - 1];
                            }
                            let text = std::str::from_utf8(slice).map_err(|_| ValueError::StringFormat)?;
                            CounterValue::TextAscii(text)
                        }
                    }
                } else {
                    return Err(ValueError::UnknownType);
                }
            }
        }
    };
    Ok(value)
}
