use std::error::Error;
use std::marker::PhantomData;
use std::time::Duration;

use super::*;

pub trait Tx {
    type Item;

    /// Blocking send value.
    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>>;

    // fn deduplicate(self) -> DeDuplicator<Self> where Self: Sized {
    //     DeDuplicator::new(self)
    // }
    //
    // fn fuse(self) -> FuseRx<Self> where Self: Sized {
    //     FuseRx::new(self)
    // }

    fn interval(self, rate: Duration) -> Interval<Self, IntervalRoleTx> where Self: Sized {
        Interval::new(self, rate)
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