//! Special encoding for morse which allows low sampling rate over
//! short-range integer-valued signal (e.g. byte stream)

use std::ops::Range;

pub type Signal = bool;

pub const ON: Signal = true;
pub const OFF: Signal = false;

/// # Ratijas Slow Mode Protocol
///
/// Specially designed protocol for non-synchronized sender and receiver
/// which may operate at slightly different rates over short-range\*
/// integer-valued signal (e.g. byte-stream).
///
/// The core idea is to transfer individual bits as integer 'signal values',
/// where each new value is different from the last one, such that non-synchronized
/// receiver always knows whether the value changed, or he is reading the old one.
///
/// This method could easily be extended to more than a pair of binary ranges and
/// multiple parallel transmissions.
///
/// \* Short-range: means small integers, like u8 or u16. Each such sample
/// value represents at most one byte of the underlying data stream, so the
/// smalled it is, the better.
pub struct RtsmProto<T> {
    off: Range<T>,
    on: Range<T>,
    off_current: T,
    on_current: T,
    current: Option<(Signal, T)>,
}

/// Helper trait for type of values on which `RtsmProto` operates.
pub trait SignalValue: Clone + Eq + PartialOrd<Self> {
    /// `self + 1` wrapped around the bounds of the given range.
    fn wrapping_next(&self, range: Range<Self>) -> Self;
}

impl<T> RtsmProto<T>
    where T: SignalValue
{
    pub fn new(off: Range<T>, on: Range<T>) -> Self {
        RtsmProto {
            current: None,
            off_current: off.start.clone(),
            on_current: on.start.clone(),
            off,
            on,
        }
    }

    pub fn signal(&self) -> Option<bool> {
        self.current.as_ref().map(|pair| pair.0)
    }

    pub fn value(&self) -> T {
        self.current.as_ref().map(|pair| pair.1.clone())
            .unwrap_or_else(|| self.off.start.clone())
    }

    fn pair_for_signal(&mut self, signal: Signal) -> (Range<T>, &mut T) {
        match signal {
            OFF => ((self.off.clone(), &mut self.off_current)),
            ON => ((self.on.clone(), &mut self.on_current)),
        }
    }

    // increment current value for range to which the signal belongs to.
    // it does not write the overall current value of self.
    fn increment_range_if_needed(&mut self, signal: Signal) -> T {
        // cache current_signal to satisfy borrow checker.
        let current = self.current.clone();
        let (range, value) = self.pair_for_signal(signal);
        // increment only if signal stays at the same value
        if let Some((current_signal, _)) = current {
            if current_signal == signal {
                *value = value.wrapping_next(range);
            }
        }
        value.clone()
    }

    pub fn encode(&mut self, signal: Signal) -> T {
        let value = self.increment_range_if_needed(signal);
        self.current = Some((signal, value.clone()));
        value
    }

    fn signal_for_value(&self, value: T) -> Result<Signal, ()> {
        if self.off.contains(&value) {
            Ok(OFF)
        } else if self.on.contains(&value) {
            Ok(ON)
        } else {
            Err(())
        }
    }

    pub fn decode(&mut self, value: T) -> Result<Option<Signal>, T> {
        match self.current.clone() {
            // signal stays still
            Some((_, current_value)) if value == current_value => {
                Ok(None)
            }
            _ => {
                // different value appeared
                match self.signal_for_value(value.clone()) {
                    Ok(signal) => {
                        self.current = Some((signal, value));
                        Ok(Some(signal))
                    }
                    Err(_) => {
                        self.current = None;
                        Err(value)
                    }
                }
            }
        }
    }

    /// Decode the whole signal at once, or fail at first erroneous value.
    pub fn decode_all(&mut self, values: &[T]) -> Result<Vec<Signal>, T> {
        values.iter()
            .cloned()
            .map(|v| self.decode(v))
            .filter_map(|res|
                // Result<Option<T>, E> -> Option<Result<T, E>>
                match res {
                    Ok(None) => None,
                    Ok(Some(signal)) => Some(Ok(signal)),
                    Err(e) => Some(Err(e)),
                }
            ).collect()
    }
}

macro_rules! imp_signal_value {
    ($($int:ty),+) => {$(
        impl SignalValue for $int {
            fn wrapping_next(&self, range: Range<Self>) -> Self {
                let mut next = self.wrapping_add(1);
                if !range.contains(&next) {
                    next = range.start.clone();
                }
                next
            }
        }
    )+};
}

imp_signal_value!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

impl Default for RtsmProto<i32> {
    fn default() -> Self {
        RtsmProto::new(10..40, 60..90)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::iter;

    const SIGNAL: &'static [Signal] = &[ON, OFF, ON, OFF, OFF, ON, ON, ON, OFF, OFF, OFF, OFF, OFF];
    const VALUES: &'static [i32] = &[50, 0, 50, 0, 1, 50, 51, 50, 1, 2, 0, 1, 2];

    #[test]
    fn test_encode() {
        // off: 50, 51
        // on: 0, 1, 2
        let mut p = RtsmProto::new(0..3, 50..52);
        let encoded = iter::once(p.value()).chain(
            SIGNAL.iter().cloned().map(|s| p.encode(s))
        ).collect::<Vec<_>>();
        assert_eq!(encoded, iter::once(0).chain(VALUES.iter().cloned()).collect::<Vec<_>>());
    }

    #[test]
    fn test_decode() {
        let mut p = RtsmProto::new(0..3, 50..52);
        let res = p.decode_all(VALUES);
        assert_eq!(res.as_ref().map(Vec::as_slice), Ok(SIGNAL));
    }

    #[test]
    fn test_deduplication() {
        const VALUES: &'static [i32] = &[11, 11, 12, 13, 13, 13, 11];
        const SIGNAL: &'static [Signal] = &[ON, ON, ON, ON];

        let mut p = RtsmProto::new(0..10, 10..20);
        let res = p.decode_all(VALUES);
        assert_eq!(res.as_ref().map(Vec::as_slice), Ok(SIGNAL));
    }
}
