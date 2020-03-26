use std::error::Error;
use std::iter::FromIterator;
use std::time::Duration;

use super::*;

pub trait Rx {
    type Item;

    /// Blocking receive value.
    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>>;

    fn deduplicate(self) -> DeduplicateRx<Self> where Self: Sized {
        DeduplicateRx::new(self)
    }

    fn fuse(self) -> FuseRx<Self> where Self: Sized {
        FuseRx::new(self)
    }

    fn interval(self, rate: Duration) -> Interval<Self, IntervalRoleRx> where Self: Sized {
        Interval::new(self, rate)
    }

    fn map<F>(self, f: F) -> MapRx<Self, F> where Self: Sized {
        MapRx::new(self, f)
    }

    fn collect<B: FromIterator<Self::Item>>(self) -> Result<B, Box<dyn Error>> where Self: Sized {
        RxIteratorAdapter::new(self).collect()
    }

    fn collect_vec(self) -> Result<Vec<Self::Item>, Box<dyn Error>> where Self: Sized {
        self.collect()
    }
}


pub struct ConstStringRx {
    string: String,
}

impl ConstStringRx {
    pub fn new<S: Into<String>>(s: S) -> Self {
        ConstStringRx {
            string: s.into()
        }
    }
}

impl Rx for ConstStringRx {
    type Item = String;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        Ok(Some(self.string.clone()))
    }
}


pub struct CounterRx {
    i: isize,
}

impl CounterRx {
    pub fn new() -> Self {
        CounterRx {
            i: 0
        }
    }
}

impl Rx for CounterRx {
    type Item = isize;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        let i = self.i;
        self.i += 1;
        Ok(Some(i))
    }
}


pub struct DeduplicateRx<R: Rx> {
    inner: R,
    last: Option<Option<R::Item>>,
}

impl<R: Rx> DeduplicateRx<R> {
    pub fn new(inner: R) -> Self {
        DeduplicateRx {
            inner,
            last: None,
        }
    }
}

impl<R> Rx for DeduplicateRx<R>
    where R: Rx,
          R::Item: Clone + Eq,
{
    type Item = R::Item;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        let new = match self.last.clone() {
            None => {
                // first time here
                self.inner.recv()?
            }
            Some(last) => {
                let mut new = last.clone();
                while new == last {
                    new = self.inner.recv()?;
                }
                // at this point new != last
                new
            }
        };

        self.last = Some(new.clone());
        Ok(new)
    }
}


/// Stops polling inner `Rx` after first error, always returning `Ok(None)` afterwards.
pub struct FuseRx<R> {
    inner: R,
    error: bool,
}

impl<R> FuseRx<R> {
    pub fn new(inner: R) -> Self {
        FuseRx {
            inner,
            error: false,
        }
    }
}

impl<R: Rx> Rx for FuseRx<R> {
    type Item = R::Item;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        if self.error {
            Ok(None)
        } else {
            match self.inner.recv() {
                Ok(item) => Ok(item),
                Err(_) => {
                    self.error = true;
                    Ok(None)
                }
            }
        }
    }
}



impl<R: Rx> Rx for Interval<R, IntervalRoleRx> {
    type Item = R::Item;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        self.sleep_and_update_last_call_time();
        self.inner.recv()
    }
}


pub struct MapRx<R, F> {
    inner: R,
    f: F,
}

impl<R, F> MapRx<R, F> {
    pub fn new(inner: R, f: F) -> Self {
        MapRx { inner, f }
    }
}

impl<R: Rx, F, U> Rx for MapRx<R, F>
    where F: FnMut(R::Item) -> U
{
    type Item = U;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        Ok(self.inner.recv()?.map(&mut self.f))
    }
}


pub struct RxIteratorAdapter<R> {
    inner: R
}

impl<R> RxIteratorAdapter<R> {
    pub fn new(inner: R) -> Self {
        RxIteratorAdapter {
            inner
        }
    }
}

impl<R: Rx> Iterator for RxIteratorAdapter<R> {
    type Item = Result<R::Item, Box<dyn Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.recv() {
            Err(e) => Some(Err(e)),
            Ok(Some(item)) => Some(Ok(item)),
            Ok(None) => None,
        }
    }
}



pub struct IteratorRx<I> {
    iter: I
}

impl<I> IteratorRx<I> {
    pub fn new(iter: I) -> Self {
        IteratorRx {
            iter
        }
    }
}

impl<I> Rx for IteratorRx<I>
    where I: Iterator
{
    type Item = I::Item;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        Ok(self.iter.next())
    }
}

impl<I> From<I> for IteratorRx<I::IntoIter>
where I: IntoIterator
{
    fn from(from: I) -> Self {
        Self::new(from.into_iter())
    }
}