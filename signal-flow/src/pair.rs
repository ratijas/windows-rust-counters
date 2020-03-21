//! Tx to Rx adapter built on top of `std::mpsc`.
use crate::tx::*;
use crate::rx::*;

use std::sync::mpsc::{channel, Sender, Receiver};
use std::error::Error;

pub struct SenderTx<T> {
    sender: Sender<T>,
}

/// Never returns an error. If corresponding sender hung up,
/// any further call to `recv()` returns `Ok(None)`.
pub struct ReceiverRx<T> {
    receiver: Receiver<T>,
}

pub fn pair<T>() -> (SenderTx<T>, ReceiverRx<T>) {
    let (sender, receiver) = channel();

    (SenderTx { sender }, ReceiverRx { receiver })
}

impl<T: Send + 'static> Tx for SenderTx<T> {
    type Item = T;

    fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
        self.sender.send(value).map_err(|e| e.into())
    }
}

impl<T> Rx for ReceiverRx<T> {
    type Item = T;

    fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
        match self.receiver.recv() {
            Ok(value) => Ok(Some(value)),
            Err(_) => Ok(None)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_mpsc() {
        let (mut sender, mut receiver) = pair();

        sender.send(12).unwrap();
        sender.send(42).unwrap();
        sender.send(37).unwrap();
        // it is important to drop sender to prevent infinite waiting on `.collect()`
        drop(sender);

        assert!(matches!(receiver.recv(), Ok(Some(12))));
        let vec = receiver.collect_vec();
        assert!(matches!(vec, Ok(_)));
        assert_eq!(vec.unwrap(), &[42, 37]);
    }
}
