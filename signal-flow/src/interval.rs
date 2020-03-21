use std::marker::PhantomData;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

pub struct Interval<X, R> {
    pub(crate) inner: X,
    rate: Duration,
    last: Option<SystemTime>,
    role: PhantomData<R>,
}

pub trait IntervalRole: private::Sealed {
    fn role_name() -> &'static str;
}

pub struct IntervalRoleTx;

pub struct IntervalRoleRx;

impl IntervalRole for IntervalRoleTx {
    fn role_name() -> &'static str {
        "Tx"
    }
}

impl IntervalRole for IntervalRoleRx {
    fn role_name() -> &'static str {
        "Rx"
    }
}

mod private {
    use super::*;

    pub trait Sealed {}

    impl Sealed for IntervalRoleTx {}

    impl Sealed for IntervalRoleRx {}
}


impl<X, R: IntervalRole> Interval<X, R> {
    pub fn new(inner: X, rate: Duration) -> Self {
        Interval {
            inner,
            rate,
            last: None,
            role: Default::default(),
        }
    }

    fn duration_until_next_call(&self, now: SystemTime) -> Option<Duration> {
        let last = match self.last {
            Some(time) => time,
            None => return None,
        };

        let elapsed = match now.duration_since(last) {
            Ok(elapsed) => elapsed,
            Err(e) => {
                println!("Interval{}: system time drift error: {:?}", R::role_name(), e);
                return None;
            }
        };

        let until_next_call = match self.rate.checked_sub(elapsed) {
            Some(until_next_call) => until_next_call,
            None => {
                println!("Interval{}: slow receiver, late by {:?}", R::role_name(), (elapsed - self.rate));
                return None;
            }
        };

        Some(until_next_call)
    }

    pub(crate) fn sleep_and_update_last_call_time(&mut self) {
        let mut now = SystemTime::now();

        if let Some(duration) = self.duration_until_next_call(now) {
            sleep(duration);
            // update current timestamp after sleep
            now = SystemTime::now();
        }

        self.last = Some(now);
    }
}
