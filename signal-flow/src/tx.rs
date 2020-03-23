use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::*;

pub trait Tx {
    type Item;

    /// Blocking send value.
    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>>;

    fn interval(self, rate: Duration) -> Interval<Self, IntervalRoleTx> where Self: Sized {
        Interval::new(self, rate)
    }

    fn cancel_on(self, cancellation_token: Arc<AtomicBool>) -> CancellableTx<Self>
        where Self: Sized
    {
        CancellableTx::new(cancellation_token, self)
    }
}


////////////////////////////////////////////////
//////////////////// Tx Ext ////////////////////
////////////////////////////////////////////////


pub trait TxExt<Iter>: Tx
    where Iter: IntoIterator<Item=Self::Item>,
{
    fn send_all(&mut self, values: Iter) -> Result<(), Box<dyn Error>> {
        for value in values.into_iter() {
            self.send(value)?;
        }
        Ok(())
    }
}

impl<Iter, Item, Any: ?Sized> TxExt<Iter> for Any
    where
        Any: Tx<Item=Item>,
        Iter: IntoIterator<Item=Item>,
{}


//////////////////////////////////////////////////
//////////////////// Interval ////////////////////
//////////////////////////////////////////////////


impl<T: Tx> Tx for Interval<T, IntervalRoleTx> {
    type Item = T::Item;

    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
        self.sleep_and_update_last_call_time();
        self.inner.send(value)
    }
}


//////////////////////////////////////////////
//////////////////// Null ////////////////////
//////////////////////////////////////////////


pub struct NullTx<T> {
    _marker: PhantomData<T>
}

impl<T> NullTx<T> {
    pub fn new() -> Self {
        NullTx {
            _marker: Default::default()
        }
    }
}

impl<T> Tx for NullTx<T> {
    type Item = T;

    fn send(&mut self, _value: Self::Item) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}


/////////////////////////////////////////////
//////////////////// Vec ////////////////////
/////////////////////////////////////////////


pub struct VecCollectorTx<'a, T> {
    buffer: &'a mut Vec<T>,
}

impl<'a, T> VecCollectorTx<'a, T> {
    pub fn new(buffer: &'a mut Vec<T>) -> Self {
        VecCollectorTx {
            buffer
        }
    }
}

impl<'a, T> Tx for VecCollectorTx<'a, T> {
    type Item = T;

    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
        self.buffer.push(value);
        Ok(())
    }
}

////////////////////////////////////////////////
//////////////////// Custom ////////////////////
////////////////////////////////////////////////

/// Passes values through unless `cancellation_token` (AtomicBool) is set to true,
/// in which case it returns an error.
pub struct CancellableTx<X> {
    tx: X,
    cancellation_token: Arc<AtomicBool>,
}

/// Specific error to indicate that Rx chain was cancelled by cancellation token.
#[derive(Debug)]
pub struct CancelledError;

impl<X> CancellableTx<X> {
    pub fn new(cancellation_token: Arc<AtomicBool>, tx: X) -> Self {
        CancellableTx {
            tx,
            cancellation_token,
        }
    }
}

impl<X: Tx> Tx for CancellableTx<X> {
    type Item = X::Item;

    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
        if self.cancellation_token.load(std::sync::atomic::Ordering::Relaxed) {
            Err(Box::new(CancelledError))
        } else {
            self.tx.send(value)
        }
    }
}

impl fmt::Display for CancelledError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "Tx was cancelled (via shared AtomicBool)".fmt(f)
    }
}

impl std::error::Error for CancelledError {
    fn description(&self) -> &str {
        "Tx was cancelled (via shared AtomicBool)"
    }
}

////////////////////////////////////////////////
//////////////////// Custom ////////////////////
////////////////////////////////////////////////


pub struct CustomTx<F, T> {
    handler: F,
    _marker: PhantomData<T>,
}

impl<F, T> CustomTx<F, T>
    where F: FnMut(T) -> Result<(), Box<dyn Error>>
{
    pub fn new(handler: F) -> Self {
        CustomTx {
            handler,
            _marker: Default::default(),
        }
    }
}

impl<F, T> Tx for CustomTx<F, T>
    where F: FnMut(T) -> Result<(), Box<dyn Error>>
{
    type Item = T;

    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
        (self.handler)(value)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_custom() {
        let mut side_effect: i32 = 0;
        let mut tx = CustomTx::new(|value| {
            side_effect = value;
            Ok(())
        });
        tx.send(42).unwrap();
        tx.send(37).unwrap();
        drop(tx);

        assert_eq!(side_effect, 37);
    }
}