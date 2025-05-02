//! From the sources of `<WinPerf.h>`:
//!
//! ```txt
//!  PERF_COUNTER_DEFINITION.CounterType field values
//!
//!
//!        Counter ID Field Definition:
//!
//!   3      2        2    2    2        1        1    1
//!   1      8        4    2    0        6        2    0    8                0
//!  +--------+--------+----+----+--------+--------+----+----+----------------+
//!  |Display |Calculation  |Time|Counter |        |Ctr |Size|                |
//!  |Flags   |Modifiers    |Base|SubType |Reserved|Type|Fld |   Reserved     |
//!  +--------+--------+----+----+--------+--------+----+----+----------------+
//! ```
use std::fmt::{self, Debug};
use std::mem::transmute;

use crate::prelude::v2::*;

/// A safe, high-level wrapper for `PERF_COUNTER_DEFINITION.CounterType` value.
#[derive(Copy, Clone)]
pub struct CounterTypeDefinition(u32);

/// Container for bit-masks of `CounterTypeDefinition` components.
#[repr(u32)]
pub enum CounterTypeMask {
    Reserved /*      */ = 0b_00000000_00000000_11110000_11111111,
    Size /*          */ = 0b_00000000_00000000_00000011_00000000,
    CounterType /*   */ = 0b_00000000_00000000_00001100_00000000,
    CounterSubType /**/ = 0b_00000000_00001111_00000000_00000000,
    TimeBase /*      */ = 0b_00000000_00110000_00000000_00000000,
    CalcModifier /*  */ = 0b_00001111_11000000_00000000_00000000,
    DisplayFlags /*  */ = 0b_11110000_00000000_00000000_00000000,
}

/// select one of the following to indicate the counter's data size
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Size {
    /// 32 bit field
    Dword = PERF_SIZE_DWORD,
    /// 64 bit field
    Large = PERF_SIZE_LARGE,
    /// for Zero Length fields
    Zero = PERF_SIZE_ZERO,
    /// length is in CounterLength field
    /// of Counter Definition struct
    Var = PERF_SIZE_VARIABLE_LEN,
}

/// select one of the following values to indicate the counter field usage
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum RawType {
    /// a number (not a counter)
    Number = PERF_TYPE_NUMBER,
    /// an increasing numeric value
    Counter = PERF_TYPE_COUNTER,
    /// a text field
    Text = PERF_TYPE_TEXT,
    /// displays a zero
    Zero = PERF_TYPE_ZERO,
}

/// Safe wrapper for type &amp; subtype.
#[derive(Copy, Clone, Debug)]
pub enum CounterType {
    Number(Number),
    Counter(Counter),
    Text(Text),
    Zero,
}

/// If the PERF_TYPE_NUMBER field was selected, then select one of the
/// following to describe the Number
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Number {
    /// display as HEX value
    Hex = PERF_NUMBER_HEX,
    /// display as a decimal integer
    Decimal = PERF_NUMBER_DECIMAL,
    /// display as a decimal/1000
    Dec1000 = PERF_NUMBER_DEC_1000,
}

/// If the PERF_TYPE_COUNTER value was selected then select one of the
/// following to indicate the type of counter
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Counter {
    /// display counter value
    Value = PERF_COUNTER_VALUE,
    /// divide ctr / delta time
    Rate = PERF_COUNTER_RATE,
    /// divide ctr / base
    Fraction = PERF_COUNTER_FRACTION,
    /// base value used in fractions
    Base = PERF_COUNTER_BASE,
    /// subtract counter from current time
    Elapsed = PERF_COUNTER_ELAPSED,
    /// Use Queuelen processing func.
    Queuelen = PERF_COUNTER_QUEUELEN,
    /// Counter begins or ends a histogram
    Histogram = PERF_COUNTER_HISTOGRAM,
    /// divide ctr / private clock
    Precision = PERF_COUNTER_PRECISION,
}

/// If the PERF_TYPE_TEXT value was selected, then select one of the
/// following to indicate the type of TEXT data.
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Text {
    /// type of text in text field
    Unicode = PERF_TEXT_UNICODE,
    /// ASCII using the CodePage field
    Ascii = PERF_TEXT_ASCII,
}

/// Timer SubTypes
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Timer {
    /// use system perf. freq for base
    TimerTick = PERF_TIMER_TICK,
    /// use 100 NS timer time base units
    Timer100NS = PERF_TIMER_100NS,
    /// use the object timer freq
    ObjectTimer = PERF_OBJECT_TIMER,
}

// Any types that have calculations performed can use one or more of
// the following calculation modification flags listed here
bitflags! {
    #[derive(Copy, Clone, Debug)]
    pub struct CalculationModifiers: u32 {
        /// compute difference first
        const DELTA = PERF_DELTA_COUNTER;
        /// compute base diff as well
        const DELTA_BASE = PERF_DELTA_BASE;
        /// show as 1.00-value (assumes:
        const INVERSE = PERF_INVERSE_COUNTER;
        /// sum of multiple instances
        const MULTI = PERF_MULTI_COUNTER;
    }
}

/// Select one of the following values to indicate the display suffix (if any)
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum DisplayFlags {
    /// no suffix
    NoSuffix = PERF_DISPLAY_NO_SUFFIX,
    /// "/sec"
    PerSec = PERF_DISPLAY_PER_SEC,
    /// "%"
    Percent = PERF_DISPLAY_PERCENT,
    /// "secs"
    Seconds = PERF_DISPLAY_SECONDS,
    /// value is not displayed
    NoShow = PERF_DISPLAY_NOSHOW,
}

///  The following are used to determine the level of detail associated
///  with the counter.  The user will be setting the level of detail
///  that should be displayed at any given time.
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum DetailLevel {
    /// The uninformed can understand it
    Novice = PERF_DETAIL_NOVICE.0,
    /// For the advanced user
    Advanced = PERF_DETAIL_ADVANCED.0,
    /// For the expert user
    Expert = PERF_DETAIL_EXPERT.0,
    /// For the system designer
    Wizard = PERF_DETAIL_WIZARD.0,
}

impl Default for DetailLevel {
    fn default() -> Self {
        DetailLevel::Novice
    }
}

impl CounterTypeDefinition {
    pub fn new(
        size: Size,
        counter_type: CounterType,
        timer: Timer,
        calculation_modifiers: CalculationModifiers,
        display_flags: DisplayFlags,
    ) -> Self {
        let inner = size.into_raw()
            | RawType::from(counter_type).into_raw()
            | counter_type.sub_type()
            | timer.into_raw()
            | calculation_modifiers.into_raw()
            | display_flags.into_raw();
        CounterTypeDefinition(inner)
    }

    pub fn from_raw(value: u32) -> Option<Self> {
        Some(Self::new(
            Size::from_raw(value),
            CounterType::from_raw(value)?,
            Timer::from_raw(value)?,
            CalculationModifiers::from_raw(value)?,
            DisplayFlags::from_raw(value)?,
        ))
    }

    pub unsafe fn from_raw_unchecked(value: u32) -> Self {
        Self::new(
            Size::from_raw(value),
            CounterType::from_raw_unchecked(value),
            Timer::from_raw_unchecked(value),
            CalculationModifiers::from_raw_truncate(value),
            DisplayFlags::from_raw_unchecked(value),
        )
    }
    #[inline(always)]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
    #[inline(always)]
    pub fn size(&self) -> Size {
        Size::from_raw(self.into_raw())
    }
    #[inline(always)]
    pub fn raw_type(&self) -> RawType {
        RawType::from_raw(self.into_raw())
    }
    #[inline(always)]
    pub fn sub_type(&self) -> u32 {
        imp::sub_type(self.into_raw())
    }
    #[inline(always)]
    pub fn counter_type(&self) -> CounterType {
        CounterType::from_raw(self.into_raw()).expect("Invalid counter type")
    }
    #[inline(always)]
    pub fn time_base(&self) -> Timer {
        Timer::from_raw(self.into_raw()).expect("Invalid time base")
    }
    #[inline(always)]
    pub fn calculation_modifiers(&self) -> CalculationModifiers {
        CalculationModifiers::from_raw(self.into_raw()).expect("Invalid calculation modifiers")
    }
    #[inline(always)]
    pub fn display_flags(self) -> DisplayFlags {
        DisplayFlags::from_raw(self.into_raw()).expect("Invalid display flags")
    }
}

// from_raw/from_raw_unchecked/into_raw implementations for CounterTypeDefinition components
mod imp {
    use std::convert::TryFrom;

    use crate::perf::nom::PerfCounterDefinition;

    use super::*;

    impl CounterTypeMask {
        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl Size {
        pub fn from_raw(value: u32) -> Self {
            let value = value & CounterTypeMask::Size.into_raw();
            // SAFETY: enum variants cover all possible values
            unsafe { transmute(value) }
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }

        pub fn size_of(self) -> Option<usize> {
            use std::mem::size_of;
            match self {
                Size::Dword => Some(size_of::<u32>()),
                Size::Large => Some(size_of::<u32>() * 2),
                Size::Zero => Some(0),
                Size::Var => None,
            }
        }
    }

    impl RawType {
        pub fn from_raw(value: u32) -> Self {
            let value = value & CounterTypeMask::CounterType.into_raw();
            // SAFETY: enum variants cover all possible values
            unsafe { transmute(value) }
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl From<CounterType> for RawType {
        /// Convert between `CounterType` and `RawType` counterparts.
        fn from(value: CounterType) -> Self {
            match value {
                CounterType::Counter(..) => RawType::Counter,
                CounterType::Number(..) => RawType::Number,
                CounterType::Text(..) => RawType::Text,
                CounterType::Zero => RawType::Zero,
            }
        }
    }

    impl CounterType {
        pub fn from_raw(value: u32) -> Option<Self> {
            Some(match RawType::from_raw(value) {
                RawType::Number => CounterType::Number(Number::from_raw(value)?),
                RawType::Counter => CounterType::Counter(Counter::from_raw(value)?),
                RawType::Text => CounterType::Text(Text::from_raw(value)?),
                RawType::Zero => CounterType::Zero,
            })
        }

        pub unsafe fn from_raw_unchecked(value: u32) -> Self {
            match RawType::from_raw(value) {
                RawType::Number => CounterType::Number(Number::from_raw_unchecked(value)),
                RawType::Counter => CounterType::Counter(Counter::from_raw_unchecked(value)),
                RawType::Text => CounterType::Text(Text::from_raw_unchecked(value)),
                RawType::Zero => CounterType::Zero,
            }
        }

        pub fn sub_type(&self) -> u32 {
            match *self {
                CounterType::Counter(it) => it.into_raw(),
                CounterType::Number(it) => it.into_raw(),
                CounterType::Text(it) => it.into_raw(),
                // Note: in Microsoft docs and sources nothing is said about subtype values of
                // Zero type. By observing certain patterns, it is safe to assume zero subtype.
                CounterType::Zero => 0,
            }
        }
    }

    #[inline(always)]
    pub const fn sub_type(value: u32) -> u32 {
        value & CounterTypeMask::CounterSubType.into_raw()
    }

    impl Number {
        pub fn from_raw(value: u32) -> Option<Self> {
            let value = sub_type(value);
            Some(match value {
                PERF_NUMBER_HEX => Self::Hex,
                PERF_NUMBER_DECIMAL => Self::Decimal,
                PERF_NUMBER_DEC_1000 => Self::Dec1000,
                _ => return None,
            })
        }

        pub unsafe fn from_raw_unchecked(value: u32) -> Self {
            // SAFETY: unsafe
            transmute(sub_type(value))
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl Counter {
        pub fn from_raw(value: u32) -> Option<Self> {
            let value = sub_type(value);
            Some(match value {
                PERF_COUNTER_VALUE => Self::Value,
                PERF_COUNTER_RATE => Self::Rate,
                PERF_COUNTER_FRACTION => Self::Fraction,
                PERF_COUNTER_BASE => Self::Base,
                PERF_COUNTER_ELAPSED => Self::Elapsed,
                PERF_COUNTER_QUEUELEN => Self::Queuelen,
                PERF_COUNTER_HISTOGRAM => Self::Histogram,
                PERF_COUNTER_PRECISION => Self::Precision,
                _ => return None,
            })
        }

        pub unsafe fn from_raw_unchecked(value: u32) -> Self {
            // SAFETY: unsafe
            transmute(sub_type(value))
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl Text {
        pub fn from_raw(value: u32) -> Option<Self> {
            let value = sub_type(value);
            Some(match value {
                PERF_TEXT_UNICODE => Self::Unicode,
                PERF_TEXT_ASCII => Self::Ascii,
                _ => return None,
            })
        }

        pub unsafe fn from_raw_unchecked(value: u32) -> Self {
            // SAFETY: unsafe
            transmute(sub_type(value))
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl Timer {
        pub fn from_raw(value: u32) -> Option<Self> {
            let value = value & CounterTypeMask::TimeBase.into_raw();
            Some(match value {
                PERF_TIMER_TICK => Self::TimerTick,
                PERF_TIMER_100NS => Self::Timer100NS,
                PERF_OBJECT_TIMER => Self::ObjectTimer,
                _ => return None,
            })
        }

        pub unsafe fn from_raw_unchecked(value: u32) -> Self {
            let value = sub_type(value);
            // SAFETY: unsafe
            transmute(value)
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl CalculationModifiers {
        pub fn from_raw(value: u32) -> Option<Self> {
            let value = value & CounterTypeMask::CalcModifier.into_raw();
            CalculationModifiers::from_bits(value)
        }

        pub unsafe fn from_raw_truncate(value: u32) -> Self {
            let value = value & CounterTypeMask::CalcModifier.into_raw();
            CalculationModifiers::from_bits_truncate(value)
        }

        pub fn into_raw(self) -> u32 {
            self.bits()
        }
    }

    impl DisplayFlags {
        pub fn from_raw(value: u32) -> Option<Self> {
            let value = value & CounterTypeMask::DisplayFlags.into_raw();
            Some(match value {
                PERF_DISPLAY_NO_SUFFIX => DisplayFlags::NoSuffix,
                PERF_DISPLAY_PER_SEC => DisplayFlags::PerSec,
                PERF_DISPLAY_PERCENT => DisplayFlags::Percent,
                PERF_DISPLAY_SECONDS => DisplayFlags::Seconds,
                PERF_DISPLAY_NOSHOW => DisplayFlags::NoShow,
                _ => return None,
            })
        }

        pub unsafe fn from_raw_unchecked(value: u32) -> Self {
            let value = value & CounterTypeMask::DisplayFlags.into_raw();
            transmute(value)
        }

        #[inline(always)]
        pub const fn into_raw(self) -> u32 {
            self as _
        }
    }

    impl<'a> TryFrom<&PerfCounterDefinition<'a>> for CounterTypeDefinition {
        type Error = ();

        fn try_from(counter: &PerfCounterDefinition<'a>) -> Result<Self, Self::Error> {
            Self::from_raw(counter.raw.CounterType).ok_or(())
        }
    }

    impl Debug for CounterTypeDefinition {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "CounterTypeDefinition {{ ")?;
            write!(f, "Size = {:?}, ", self.size())?;
            write!(f, "Type = {:?}, ", self.counter_type())?;
            write!(f, "Timer = {:?}, ", self.time_base())?;
            write!(f, "Modifiers = {:?}, ", self.calculation_modifiers())?;
            write!(f, "Display = {:?}", self.display_flags())?;
            write!(f, " }}")?;
            Ok(())
        }
    }
}
