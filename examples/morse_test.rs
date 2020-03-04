extern crate windows_rust_test;

use std::thread::sleep;
use std::time::Duration;

use windows_rust_test::morse::*;
use windows_rust_test::signal::rx::*;

fn main() {
    println!("Morse message:");
    let seq = EncoderTx::<ITU>::encode_str("SOS");
    for bit in seq {
        print!("{}", if bit { '-' } else { ' ' });
    }
    println!();

    println!("Back to normal message:");
    let signal: &[Signal] = &[
        ON, OFF, ON, OFF, ON, // S
        OFF, OFF, OFF, // letter pause
        ON, ON, ON, OFF, ON, ON, ON, OFF, ON, ON, ON, // O
        OFF, OFF, OFF, // letter pause
        ON, OFF, ON, OFF, ON // S
    ];
    let text = IteratorRx::from(signal.iter().cloned())
        .morse_decode::<ITU>()
        .collect::<Vec<_>>();
    println!("{:?}", text);


    let mut rx = CounterRx::new().interval(Duration::from_millis(1000));

    loop {
        match rx.recv() {
            Ok(Some(item)) => println!("Got an item: {}", item),
            Ok(None) => println!("Nothing"),
            Err(e) => {
                println!("Error: {:?}", e);
                break;
            }
        }

        // What are the chances that the thread will be waking up in time,
        // one millisecond before the next loop?
        sleep(Duration::from_millis(999));
    }
}
