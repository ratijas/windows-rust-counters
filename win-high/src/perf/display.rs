//! Translated from MSDN examples [one] and [two].
//!
//! [one]: https://docs.microsoft.com/en-us/windows/win32/perfctrs/calculating-counter-values
//! [two]: https://docs.microsoft.com/en-us/windows/win32/perfctrs/retrieving-counter-data

use crate::perf::types::*;
use crate::prelude::v2::*;

use win_low::um::winperf::*;

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Sample {
    CounterType: CounterTypeDefinition,
    Data: u64,
    Time: i64,
    MultiCounterData: u32,
    Frequency: i64,
}

pub unsafe fn get_sample(
    perf_data: &PERF_DATA_BLOCK,
    object: &PERF_OBJECT_TYPE,
    counter: &PERF_COUNTER_DEFINITION,
    counter_data_block: &PERF_COUNTER_BLOCK,
) -> Result<Sample, ()>
{ unsafe {
    let mut base_counter_ptr: *const PERF_COUNTER_DEFINITION = counter as *const _;
    let mut sample: Sample = std::mem::zeroed();
    sample.CounterType = CounterTypeDefinition::from_raw(counter.CounterType).ok_or(())?;
    //Point to the raw counter data.
    let buffer = counter_data_block as *const _ as *const u8;
    let data_ptr: *const u8 = buffer.add(counter.CounterOffset as usize);
    let counter_type = CounterTypeDefinition::from_raw_unchecked(counter.CounterType);

    //Now use the PERF_COUNTER_DEFINITION.CounterType value to figure out what
    //other information you need to calculate a displayable value.
    match counter.CounterType {
        PERF_COUNTER_COUNTER
        | PERF_COUNTER_QUEUELEN_TYPE
        | PERF_SAMPLE_COUNTER
        => {
            sample.Data = (data_ptr as *const u32).read() as u64;
            sample.Time = perf_data.PerfTime;
            if PERF_COUNTER_COUNTER == counter.CounterType || PERF_SAMPLE_COUNTER == counter.CounterType
            {
                sample.Frequency = perf_data.PerfFreq;
            }
        }
        PERF_OBJ_TIME_TIMER => {
            sample.Data = (data_ptr as *const u32).read() as u64;
            sample.Time = object.PerfTime;
        }
        PERF_COUNTER_100NS_QUEUELEN_TYPE => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Time = perf_data.PerfTime100nSec;
        }
        PERF_COUNTER_OBJ_TIME_QUEUELEN_TYPE => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Time = object.PerfTime;
        }
        PERF_COUNTER_TIMER
        | PERF_COUNTER_TIMER_INV
        | PERF_COUNTER_BULK_COUNT
        | PERF_COUNTER_LARGE_QUEUELEN_TYPE
        => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Time = perf_data.PerfTime;
            if counter.CounterType == PERF_COUNTER_BULK_COUNT
            {
                sample.Frequency = perf_data.PerfFreq;
            }
        }
        PERF_COUNTER_MULTI_TIMER
        | PERF_COUNTER_MULTI_TIMER_INV
        => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Frequency = perf_data.PerfFreq;
            sample.Time = perf_data.PerfTime;
        }
        //These counters do not use any time reference.
        PERF_COUNTER_RAWCOUNT
        | PERF_COUNTER_RAWCOUNT_HEX
        | PERF_COUNTER_DELTA
        => {
            sample.Data = (data_ptr as *const u32).read() as u64;
            sample.Time = 0;
        }
        PERF_COUNTER_LARGE_RAWCOUNT
        | PERF_COUNTER_LARGE_RAWCOUNT_HEX
        | PERF_COUNTER_LARGE_DELTA
        => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Time = 0;
        }
        //These counters use the 100ns time base in their calculation.
        | PERF_100NSEC_TIMER
        | PERF_100NSEC_TIMER_INV
        | PERF_100NSEC_MULTI_TIMER
        | PERF_100NSEC_MULTI_TIMER_INV
        => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Time = perf_data.PerfTime100nSec;
            // XXX: MultiCounterData after match
        }
        //These counters use two data points, this value and one from this counter's
        //base counter. The base counter should be the next counter in the object's
        //list of counters.
        PERF_SAMPLE_FRACTION
        | PERF_RAW_FRACTION
        => {
            sample.Data = (data_ptr as *const u32).read() as u64;
            //Get base counter
            base_counter_ptr = base_counter_ptr.offset(1);
            if (counter.CounterType & PERF_COUNTER_BASE) == PERF_COUNTER_BASE {
                let data_ptr = buffer.offset(base_counter_ptr.read().CounterOffset as _);
                sample.Time = (data_ptr as *const u32).read() as _;
            } else {
                return Err(());
            }
        }
        PERF_LARGE_RAW_FRACTION => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            // XXX: duplicate fragment from above, except for DWORD vs LONGLONG reads
            base_counter_ptr = base_counter_ptr.offset(1);
            if (counter.CounterType & PERF_COUNTER_BASE) == PERF_COUNTER_BASE {
                let data_ptr = buffer.offset(base_counter_ptr.read().CounterOffset as _);
                sample.Time = (data_ptr as *const i64).read();
            } else {
                return Err(());
            }
        }
        PERF_PRECISION_SYSTEM_TIMER
        | PERF_PRECISION_100NS_TIMER
        | PERF_PRECISION_OBJECT_TIMER
        => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            // XXX: duplicate fragment from above, except for DWORD vs LONGLONG reads
            base_counter_ptr = base_counter_ptr.offset(1);
            if (counter.CounterType & PERF_COUNTER_BASE) == PERF_COUNTER_BASE {
                let data_ptr = buffer.offset(base_counter_ptr.read().CounterOffset as _);
                sample.Time = (data_ptr as *const i64).read();
            } else {
                return Err(());
            }
        }
        PERF_AVERAGE_TIMER
        | PERF_AVERAGE_BULK
        => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            // XXX: duplicate fragment from above, except for DWORD vs LONGLONG reads
            base_counter_ptr = base_counter_ptr.offset(1);
            if (counter.CounterType & PERF_COUNTER_BASE) == PERF_COUNTER_BASE {
                let data_ptr = buffer.offset(base_counter_ptr.read().CounterOffset as _);
                sample.Time = (data_ptr as *const u32).read() as _;
            } else {
                return Err(());
            }

            if counter.CounterType == PERF_AVERAGE_TIMER {
                sample.Frequency = perf_data.PerfFreq;
            }
        }
        //These are base counters and are used in calculations for other counters.
        //This case should never be entered.
        PERF_SAMPLE_BASE
        | PERF_AVERAGE_BASE
        | PERF_COUNTER_MULTI_BASE
        | PERF_RAW_BASE
        | PERF_LARGE_RAW_BASE
        => {
            return Err(());
        }
        PERF_ELAPSED_TIME => {
            sample.Data = (data_ptr as *const u64).read_unaligned();
            sample.Time = object.PerfTime;
            sample.Frequency = object.PerfFreq;
        }
        // These counters are currently not supported.
        PERF_COUNTER_TEXT
        | PERF_COUNTER_NODATA
        | PERF_COUNTER_HISTOGRAM_TYPE
        => {
            return Err(());
        }
        // Encountered an unidentified counter.
        _ => {
            return Err(());
        }
    }
    //These counter types have a second counter value that is adjacent to
    //this counter value in the counter data block. The value is needed for
    //the calculation.
    if counter_type.calculation_modifiers().contains(CalculationModifiers::MULTI) {
        sample.MultiCounterData = ((data_ptr as *const u64).offset(1) as *const u32).read_unaligned();
    }
    Ok(sample)
}}

/// Use the CounterType to determine how to calculate the displayable
/// value. The case statement includes the formula used to calculate
/// the value.
pub fn display_calculated_value(new: &Sample, old_opt: Option<&Sample>) -> Result<String, String> {
    // If the counter type contains the PERF_DELTA_COUNTER flag, you need
    // two samples to calculate the value.
    if new.CounterType.calculation_modifiers().contains(CalculationModifiers::DELTA)
        && old_opt.is_none() {
        return Err("The counter type requires two samples but only one sample was passed.".into());
    }
    // Check for integer overflow or bad data from provider (the data from
    // sample 2 must be greater than the data from sample 1).
    // XXX: it was probably a mistake: Time should be compared instead of Data.
    // XXX: if (pSample2 != NULL && pSample1->Data > pSample2->Data)
    if let Some(old) = old_opt {
        if new.Time > old.Time {
            return Err(format!("Sample1 ({}) is larger than sample2 ({}).", new.Time, old.Time));
        }
    }
    let display = match new.CounterType.into_raw() {
        //(N1 - N0)/((D1 - D0)/F)
        PERF_COUNTER_COUNTER
        | PERF_SAMPLE_COUNTER
        | PERF_COUNTER_BULK_COUNT
        => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = (numerator as f64 / (denominator as f64 / old.Frequency as f64)) as u32;
            let suffix = if new.CounterType.into_raw() == PERF_SAMPLE_COUNTER { "" } else { "/sec" };
            format!("{}{}", value, suffix)
        }
        //(N1 - N0)/(D1 - D0)
        PERF_COUNTER_QUEUELEN_TYPE
        | PERF_COUNTER_100NS_QUEUELEN_TYPE
        | PERF_COUNTER_OBJ_TIME_QUEUELEN_TYPE
        | PERF_COUNTER_LARGE_QUEUELEN_TYPE
        => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = numerator as f64 / denominator as f64;
            format!("{}", value)
        }
        //don't display
        PERF_AVERAGE_BULK => "".to_string(),
        // 100*(N1 - N0)/(D1 - D0)
        PERF_OBJ_TIME_TIMER
        | PERF_COUNTER_TIMER
        | PERF_100NSEC_TIMER
        | PERF_PRECISION_SYSTEM_TIMER
        | PERF_PRECISION_100NS_TIMER
        | PERF_PRECISION_OBJECT_TIMER
        | PERF_SAMPLE_FRACTION
        => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = (100.0 * numerator as f64) / denominator as f64;
            format!("{}%", value)
        }
        // 100*(1- ((N1 - N0)/(D1 - D0)))
        PERF_COUNTER_TIMER_INV => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = 100.0 * (1.0 - (numerator as f64 / denominator as f64));
            format!("{}%", value)
        }
        // 100*(1- (N1 - N0)/(D1 - D0))
        PERF_100NSEC_TIMER_INV => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = 100.0 * (1.0 - numerator as f64 / denominator as f64);
            format!("{}%", value)
        }
        // 100*((N1 - N0)/((D1 - D0)/TB))/B1
        PERF_COUNTER_MULTI_TIMER => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = (old.Time - new.Time) / old.Frequency;
            let value = 100.0 * (numerator as f64 / denominator as f64) / old.MultiCounterData as f64;
            format!("{}%", value)
        }
        // 100*((N1 - N0)/(D1 - D0))/B1
        PERF_100NSEC_MULTI_TIMER => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = 100.0 * (numerator as f64 / denominator as f64) / old.MultiCounterData as f64;
            format!("{}%", value)
        }
        // 100*(B1- ((N1 - N0)/(D1 - D0)))
        PERF_COUNTER_MULTI_TIMER_INV
        | PERF_100NSEC_MULTI_TIMER_INV
        => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = 100.0 * (old.MultiCounterData as f64 - (numerator as f64 / denominator as f64));
            format!("{}%", value)
        }
        // N as decimal
        PERF_COUNTER_RAWCOUNT
        | PERF_COUNTER_LARGE_RAWCOUNT
        => {
            format!("{}", new.Data)
        }
        // N as hexadecimal
        PERF_COUNTER_RAWCOUNT_HEX
        | PERF_COUNTER_LARGE_RAWCOUNT_HEX
        => {
            format!("{:#x}", new.Data)
        }
        // N1 - N0
        PERF_COUNTER_DELTA
        | PERF_COUNTER_LARGE_DELTA
        => {
            let old = old_opt.unwrap();
            let value = old.Data - new.Data;
            format!("{}", value)
        }
        // 100*N/B
        PERF_RAW_FRACTION
        | PERF_LARGE_RAW_FRACTION
        => {
            let value = 100.0 * new.Data as f64 / new.Time as f64;
            format!("{}%", value)
        }
        // ((N1 - N0)/TB)/(B1 - B0)
        PERF_AVERAGE_TIMER => {
            let old = old_opt.unwrap();
            let numerator = old.Data - new.Data;
            let denominator = old.Time - new.Time;
            let value = numerator as f64 / old.Frequency as f64 / denominator as f64;
            format!("{} seconds", value)
        }
        //(D0 - N0)/F
        PERF_ELAPSED_TIME => {
            let value = (new.Time as f64 - new.Data as f64) / new.Frequency as f64;
            format!("{} seconds", value)
        }
        _ => return Err("Counter type not found".into()),
    };
    Ok(display)
}
