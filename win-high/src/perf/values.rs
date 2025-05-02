use std::convert::TryFrom;

use crate::perf::nom::*;
use crate::perf::types::*;
use crate::prelude::v2::*;

/// Owned wrapper for counter value.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum CounterValue {
    Dword(u32),
    Large(u64),
    TextUnicode(U16CString),
    TextAscii(String),
    Zero,
}

impl CounterValue {
    pub fn borrow(&self) -> CounterVal {
        match *self {
            Self::Dword(value) => CounterVal::Dword(value),
            Self::Large(value) => CounterVal::Large(value),
            Self::TextUnicode(ref string) => CounterVal::TextUnicode(string.as_ref()),
            Self::TextAscii(ref string) => CounterVal::TextAscii(string.as_ref()),
            Self::Zero => CounterVal::Zero,
        }
    }
}

/// Borrowed wrapper for counter value.
///
/// It is to the `CounterValue` as a `str` is to the String.
/// Well, not actually, because we can't just return `&CounterVal`
/// from the `CounterValue::borrow()` method. Instead, an **owned**
/// `CounterVal` with a **borrowed** data is returned.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum CounterVal<'a> {
    Dword(u32),
    Large(u64),
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

impl<'b> CounterVal<'b> {
    pub fn to_owned(&self) -> CounterValue {
        match *self {
            Self::Dword(value) => CounterValue::Dword(value),
            Self::Large(value) => CounterValue::Large(value),
            Self::TextUnicode(str) => CounterValue::TextUnicode(str.to_owned()),
            Self::TextAscii(str) => CounterValue::TextAscii(str.to_owned()),
            Self::Zero => CounterValue::Zero,
        }
    }

    pub fn try_get<'a>(def: &'a PerfCounterDefinition, block: &'b PerfCounterBlock) -> Result<Self, ValueError> {
        get_value(def, block)
    }

    pub fn write(&self, buffer: &mut [u8]) -> Result<(), ValueError> {
        fn checked_write(src: &[u8], dst: &mut [u8]) -> Result<(), ValueError> {
            if src.len() != dst.len() {
                return Err(ValueError::BadSize);
            }
            dst.copy_from_slice(src);
            Ok(())
        }
        unsafe {
            match *self {
                CounterVal::Dword(dword) => {
                    let slice: &[u32] = &[dword];
                    let source = downcast(slice);
                    checked_write(source, buffer)?;
                }
                CounterVal::Large(qword) => {
                    let slice: &[u64] = &[qword];
                    let source = downcast(slice);
                    checked_write(source, buffer)?;
                }
                CounterVal::TextUnicode(_) => panic!("not supported"),
                CounterVal::TextAscii(_) => panic!("not supported"),
                CounterVal::Zero => checked_write(&[], buffer)?,
            }
        }
        Ok(())
    }
}

pub fn get_slice<'a, 'b>(def: &'a PerfCounterDefinition, block: &'b PerfCounterBlock) -> Option<&'b [u8]> {
    let len = def.raw.CounterSize as usize;
    let offset = def.raw.CounterOffset as usize;
    block.data().get(offset..offset + len)
}

fn get_value<'a, 'b>(def: &'a PerfCounterDefinition, block: &'b PerfCounterBlock) -> Result<CounterVal<'b>, ValueError> {
    let typ = CounterTypeDefinition::try_from(def).expect("counter");
    let mut slice = get_slice(def, block).ok_or(ValueError::BadSize)?;
    let value = unsafe {
        match typ.size() {
            Size::Dword => {
                let number = upcast::<u32>(slice).map_err(|_| ValueError::BadSize)?
                    .get(0).ok_or(ValueError::NoData)?.clone();
                CounterVal::Dword(number)
            }
            Size::Large => {
                let number = upcast::<u64>(slice).map_err(|_| ValueError::BadSize)?
                    .get(0).ok_or(ValueError::NoData)?.clone();
                CounterVal::Large(number)
            }
            Size::Zero => CounterVal::Zero,
            Size::Var => {
                if let CounterType::Text(encoding) = typ.counter_type() {
                    match encoding {
                        Text::Unicode => {
                            let u16slice = upcast::<u16>(slice).map_err(|_| ValueError::BadSize)?;
                            let text = U16CStr::from_slice_truncate(u16slice).map_err(|_| ValueError::StringFormat)?;
                            CounterVal::TextUnicode(text)
                        }
                        Text::Ascii => {
                            // is there slice.trim method?
                            while slice.ends_with(&[0u8]) {
                                slice = &slice[..slice.len() - 1];
                            }
                            let text = std::str::from_utf8(slice).map_err(|_| ValueError::StringFormat)?;
                            CounterVal::TextAscii(text)
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
