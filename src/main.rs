#![allow(non_snake_case)]
#![allow(unused)]
#![allow(unused_imports)]

extern crate libc;
extern crate widestring;
extern crate winapi;
extern crate windows_rust_counters;
#[macro_use]
extern crate windows_service;

use itertools::Itertools;

use std::io::{Read, Write};
use std::time::Duration;
use windows_rust_counters::morse::*;
use windows_rust_counters::signal::rx::*;
use windows_rust_counters::signal::tx::*;
use windows_rust_counters::win::uses::*;

use self::ascii::*;

enum Role {
    Encoder,
    Decoder,
}

mod ascii {
    use std::error::Error;
    use windows_rust_counters::morse::*;
    use windows_rust_counters::signal::tx::Tx;
    use windows_rust_counters::signal::rx::Rx;

    pub const ASCII_ON: char = '-';
    pub const ASCII_OFF: char = ' ';


    pub struct SignalToAsciiTx<X> {
        inner: X
    }

    impl<X> SignalToAsciiTx<X> {
        pub fn new(inner: X) -> Self {
            SignalToAsciiTx { inner }
        }

        pub fn encode(&self, state: Signal) -> char {
            if state {
                ASCII_ON
            } else {
                ASCII_OFF
            }
        }
    }

    impl<X: Tx<Item=char>> Tx for SignalToAsciiTx<X> {
        type Item = Signal;

        fn send(&mut self, value: Self::Item) -> Result<(), Box<dyn Error>> {
            self.inner.send(self.encode(value))
        }
    }


    pub struct SignalFromAsciiRx<X> {
        inner: X,
    }

    impl<X> SignalFromAsciiRx<X> {
        pub fn new(inner: X) -> Self {
            SignalFromAsciiRx { inner }
        }

        pub fn decode(&self, char: char) -> Option<Signal> {
            if char == ASCII_ON {
                Some(ON)
            } else if char == ASCII_OFF {
                Some(OFF)
            } else {
                None
            }
        }
    }

    impl<X: Rx<Item=char>> Rx for SignalFromAsciiRx<X> {
        type Item = Signal;

        fn recv(&mut self) -> Result<Option<Self::Item>, Box<dyn Error>> {
            loop {
                match self.inner.recv()? {
                    None => return Ok(None),
                    Some(char) => match self.decode(char) {
                        None => { /* loop */ }
                        Some(state) => return Ok(Some(state))
                    }
                }
            }
        }
    }


    pub trait AsciiRxExt where Self: Sized {
        fn signal_from_ascii(self) -> SignalFromAsciiRx<Self> {
            SignalFromAsciiRx::new(self)
        }
    }

    impl<X> AsciiRxExt for X where X: Rx<Item=char> {}
}

fn main() {
    let prg = std::env::args().next().unwrap();
    let role = match std::env::args().skip(1).next() {
        Some(arg) if &*arg == "--encode" => Role::Encoder,
        Some(arg) if &*arg == "--decode" => Role::Decoder,
        _ => panic!("Syntax: {} ( --encode | --decode )", prg),
    };

    match role {
        Role::Encoder => {
            let mut encoder = EncoderTx::<ITU>::new(
                SignalToAsciiTx::new(
                    CustomTx::new(|char| {
                        let stdout = std::io::stdout();
                        let mut handle = stdout.lock();
                        handle.write_all(&[char as u8])?;
                        handle.flush()?;
                        Ok(())
                    })
                )
            );
            // Text in, Morse as ASCII out
            for byte in std::io::stdin().bytes() {
                // Assume single-byte ASCII input
                let char = byte.expect("read byte from stdin") as char;
                encoder.send(char).expect("encode character");
            }
        }
        Role::Decoder => {
            let stdin = std::io::stdin();
            let mut h_stdin = stdin.lock();
            println!("locked stdin");
            let mut decoder = IteratorRx::from(
                h_stdin.bytes().map(|r| r.expect("read byte from stdin") as char)
            )
                .signal_from_ascii()
                .morse_decode::<ITU>();
            loop {
                match decoder.recv() {
                    Ok(Some(char)) => {
                        let stdout = std::io::stdout();
                        let mut handle = stdout.lock();
                        handle.write_all(&[char as u8]).expect("write decoded character");
                        handle.flush().expect("flush");
                    },
                    Ok(None) => {
                        println!("Ok(None)");
                        break
                    },
                    Err(e) => println!("Decode error: {}", e),
                }
            }
        }
    }
}
