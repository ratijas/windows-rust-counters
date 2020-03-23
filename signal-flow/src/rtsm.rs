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

/// RTSM-proto (Ratijas Slow-Mode Protocol) receiver.
pub struct RtsmRx<X: Rx> {
    rx: X,
    off: Range<X::Item>,
    on: Range<X::Item>,
    last: Option<X::Item>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct DecodeError<T>(pub T);

pub trait RtsmTxExt: Tx {
    fn rtsm(self, ranges: RtsmRanges<Self::Item>) -> RtsmTx<Self>
        where Self: Sized,
              Self::Item: SignalValue
    {
        RtsmTx::new(ranges, self)
    }
}

impl<X> RtsmTxExt for X where X: Tx {}

pub trait RtsmRxExt: Rx {
    fn rtsm(self, ranges: RtsmRanges<Self::Item>) -> RtsmRx<Self>
        where Self: Sized,
              Self::Item: SignalValue
    {
        RtsmRx::new(ranges, self)
    }
}

impl<X> RtsmRxExt for X where X: Rx {}

mod imp {
    use super::*;
    use std::error::Error;
    use std::fmt;

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

    impl<X: Rx> RtsmRx<X>
        where X::Item: SignalValue
    {
        pub fn new(ranges: RtsmRanges<X::Item>, rx: X) -> Self {
            RtsmRx {
                rx,
                off: ranges.off,
                on: ranges.on,
                last: None,
            }
        }

        fn signal_for_value(&self, value: X::Item) -> Result<Signal, ()> {
            if self.off.contains(&value) {
                Ok(OFF)
            } else if self.on.contains(&value) {
                Ok(ON)
            } else {
                Err(())
            }
        }

        fn decode(&mut self, value: X::Item) -> Result<Option<Signal>, DecodeError<X::Item>> {
            match &self.last {
                // signal stays still
                Some(last_value) if &value == last_value => {
                    Ok(None)
                }
                _ => {
                    // different value appeared
                    match self.signal_for_value(value.clone()) {
                        Ok(signal) => {
                            self.last = Some(value);
                            Ok(Some(signal))
                        }
                        Err(_) => {
                            self.last = None;
                            Err(DecodeError(value))
                        }
                    }
                }
            }
        }

        // /// Decode the whole signal at once, or fail at first erroneous value.
        // fn decode_all(&mut self, values: &[X::Item]) -> Result<Vec<Signal>, DecodeError<X::Item>> {
        //     values.iter()
        //         .cloned()
        //         .map(|v| self.decode(v))
        //         .filter_map(|res|
        //             // Result<Option<T>, E> -> Option<Result<T, E>>
        //             match res {
        //                 Ok(None) => None,
        //                 Ok(Some(signal)) => Some(Ok(signal)),
        //                 Err(e) => Some(Err(e)),
        //             }
        //         ).collect()
        // }
    }

    impl<X: Rx> Rx for RtsmRx<X>
        where X::Item: SignalValue + 'static
    {
        type Item = Signal;

        fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
            loop {
                match self.rx.recv()? {
                    None => return Ok(None),
                    Some(value) => match self.decode(value)? {
                        None => { /* repeat with next inner value */ }
                        Some(signal) => return Ok(Some(signal)),
                    }
                }
            }
        }
    }

    impl<T> fmt::Debug for DecodeError<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            "DecodeError(..)".fmt(f)
        }
    }

    impl<T> fmt::Display for DecodeError<T> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            "failed to decode a signal".fmt(f)
        }
    }

    impl<T> Error for DecodeError<T> {
        fn description(&self) -> &str {
            "failed to decode a signal"
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_rtsm_tx() {
        let (tx, rx) = pair();
        let ranges = RtsmRanges::new(0..10, 20..30).unwrap();
        let mut rtsm = RtsmTx::new(ranges, tx);

        const SIGNAL: &'static [Signal] = &[ON, ON, ON, OFF];
        rtsm.send_all(SIGNAL.iter().cloned()).unwrap();
        drop(rtsm);

        let vec = rx.collect_vec().unwrap();
        assert_eq!(vec, &[20, 21, 22, 0]);
    }

    #[test]
    fn test_rtsm_rx_error() {
        let (mut tx, rx) = pair();
        let ranges = RtsmRanges::<u32>::new(0..10, 20..30).unwrap();
        let mut rtsm = RtsmRx::new(ranges, rx);

        tx.send(99).unwrap();
        drop(tx);

        let res = rtsm.recv();
        let err = res.err().unwrap();
        let dec = err.downcast::<DecodeError<u32>>().unwrap();
        assert_eq!(dec.0, 99);
    }

    const SIGNAL: &'static [Signal] = &[ON, OFF, ON, OFF, OFF, ON, ON, ON, OFF, OFF, OFF, OFF, OFF];
    const VALUES: &'static [i32] = &[50, 0, 50, 0, 1, 50, 51, 50, 1, 2, 0, 1, 2];

    fn ranges() -> RtsmRanges<i32> {
        RtsmRanges::new(0..3, 50..52).unwrap()
    }

    #[test]
    fn test_encode() {
        // off: 50, 51
        // on: 0, 1, 2
        let (tx, rx) = pair();
        let mut rtsm = RtsmTx::new(ranges(), tx);

        rtsm.send_all(SIGNAL.iter().cloned()).unwrap();
        drop(rtsm);

        let encoded = rx.collect_vec().unwrap();
        assert_eq!(encoded, VALUES);
    }

    #[test]
    fn test_decode() {
        let rx = IteratorRx::new(VALUES.iter().cloned());
        let rtsm = RtsmRx::new(ranges(), rx);
        let decoded = rtsm.collect_vec().unwrap();
        assert_eq!(decoded, SIGNAL);
    }

    #[test]
    fn test_deduplication() {
        const VALUES_DUP: &'static [i32] = &[11, 11, 12, 13, 13, 13, 11];
        const SIGNAL_DUP: &'static [Signal] = &[ON, ON, ON, ON];

        let ranges = RtsmRanges::new(0..10, 10..20).unwrap();
        let rx = IteratorRx::new(VALUES_DUP.iter().cloned());
        let rtsm = RtsmRx::new(ranges, rx);

        let res = rtsm.collect_vec().unwrap();
        assert_eq!(res, SIGNAL_DUP);
    }
}
