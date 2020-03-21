use crate::*;

use std::ops::{Range, Sub};

pub type Signal = bool;

pub const ON: Signal = true;
pub const OFF: Signal = false;

pub struct RtsmRanges<T> {
    off: Range<T>,
    on: Range<T>,
}

pub const RANGE_100_HALF: RtsmRanges<u32> = RtsmRanges { off: 0..50, on: 50..100 };
pub const RANGE_100_QUARTER: RtsmRanges<u32> = RtsmRanges { off: 0..25, on: 75..100 };

#[derive(Clone)]
struct RangeValue<T> {
    range: Range<T>,
    value: T,
}

/// Helper trait for type of values on which `RtsmProto` operates.
pub trait SignalValue: Clone + Eq + PartialOrd<Self> + Sub<Output=Self> {
    /// Just a regular one of whatever type it is.
    fn one() -> Self;

    /// `self + 1` wrapped around the bounds of the given range.
    fn wrapping_next(&self, range: &Range<Self>) -> Self;
}

/// RTSM-proto (Ratijas Slow-Mode Protocol) transmitter.
///
/// Give it a signal, and it will encode that signal into value `T` and send
/// `T` down the pipeline.
pub struct RtsmTx<X: Tx> {
    tx: X,
    off: RangeValue<X::Item>,
    on: RangeValue<X::Item>,
    current: Option<Signal>,
}

// pub struct RtsmRx<T> {}

mod imp {
    use super::*;
    use std::error::Error;

    fn ranges_are_valid<T: SignalValue>(r1: &Range<T>, r2: &Range<T>) -> bool {
        // |...r1...|
        //      |...r2...|
        // ^    ^   ^    ^
        // 1s  2s   1e   2e
        // Either both (1s and 1e) must be less than both (2s and 2e) other vice-versa.
        // If ranges are correctly ordered (start < end), than only one check is required:
        // low.end <= high.start. Ranges are exclusive, so <= is OK.

        (true
            // internal ordering check
            && r1.start < r1.end
            && r2.start < r2.end)
            // size check
            && (r1.end.clone() - r1.start.clone()) > T::one()
            && (r2.end.clone() - r2.start.clone()) > T::one()
            // external ordering check
            && ((r1.end <= r2.start)
            /**/ ^ (r2.end <= r1.start))
    }

    impl<T: SignalValue> RtsmRanges<T> {
        pub fn new(off: Range<T>, on: Range<T>) -> Result<Self, ()> {
            if ranges_are_valid(&off, &on) {
                Ok(RtsmRanges { off, on })
            } else {
                Err(())
            }
        }

        /// Suitable for compile-time known constant ranges.
        ///
        /// SAFETY: safe, but make sure not to pass unusable and/or overlaping ranges.
        pub unsafe fn new_unchecked(off: Range<T>, on: Range<T>) -> Self {
            RtsmRanges { off, on }
        }
    }

    impl<T> From<Range<T>> for RangeValue<T>
        where T: Clone
    {
        fn from(range: Range<T>) -> Self {
            RangeValue {
                value: range.start.clone(),
                range,
            }
        }
    }

    macro_rules! imp_signal_value {
        ($($int:ty),+) => {$(
            impl SignalValue for $int {
                fn one() -> Self {
                    1 as $int
                }

                fn wrapping_next(&self, range: &Range<Self>) -> Self {
                    let mut next = self.wrapping_add(Self::one());
                    if !range.contains(&next) {
                        next = range.start.clone();
                    }
                    next
                }
            }
        )+};
    }

    imp_signal_value!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

    impl<X: Tx> RtsmTx<X>
        where X::Item: SignalValue
    {
        pub fn new(ranges: RtsmRanges<X::Item>, tx: X) -> Self {
            RtsmTx {
                tx,
                off: RangeValue::from(ranges.off),
                on: RangeValue::from(ranges.on),
                current: None,
            }
        }

        fn ranges_for_signal(&mut self, signal: Signal) -> &mut RangeValue<X::Item> {
            match signal {
                OFF => &mut self.off,
                ON => &mut self.on,
            }
        }

        // increment current value for range to which the signal belongs to.
        // it does not write the overall current value of self.
        fn increment_range_if_needed(&mut self, signal: Signal) -> X::Item {
            // cache current_signal to satisfy borrow checker.
            let current = self.current;
            let RangeValue { ref range, value } = self.ranges_for_signal(signal);
            // increment only if signal stays at the same value
            if let Some(current_signal) = current {
                if current_signal == signal {
                    *value = value.wrapping_next(range);
                }
            }
            value.clone()
        }

        fn encode(&mut self, signal: Signal) -> X::Item {
            let value = self.increment_range_if_needed(signal);
            self.current = Some(signal);
            value
        }
    }

    impl<X: Tx> Tx for RtsmTx<X>
        where X::Item: SignalValue
    {
        type Item = Signal;

        fn send(&mut self, signal: Signal) -> Result<(), Box<dyn Error>> {
            let value = self.encode(signal);
            self.tx.send(value)
        }
    }

    // impl<T> Rx for RtsmRx<T>
    //     where T: SignalValue
    // {
    //     type Item = bool;
    //
    //     fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
    //         unimplemented!()
    //     }
    // }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_rtsm_tx() {
        let (send, recv) = pair();

        let ranges = RtsmRanges::new(0..10, 20..30).unwrap();
        let mut tx = RtsmTx::new(ranges, send);

        const SIGNAL: &'static [Signal] = &[ON, ON, ON, OFF];
        for signal in SIGNAL.iter().cloned() {
            tx.send(signal).expect("send");
        }
        drop(tx);

        let vec = recv.collect_vec().unwrap();
        assert_eq!(vec, &[20, 21, 22, 0]);
    }
}
