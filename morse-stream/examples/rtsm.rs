//! Example of using Tx/Rx pair with RTSM and Morse coder.
#[macro_use]
extern crate lazy_static;

use std::error::Error;
use std::iter;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;
use std::time::Duration;

use morse_stream::*;
use signal_flow::*;
use signal_flow::rtsm::*;

pub struct WorkerThread<T> {
    thread: JoinHandle<T>,
    cancellation_token: Arc<AtomicBool>,
}

impl<T> WorkerThread<T>
    where T: Send + 'static
{
    /// Spawn new worker with new cancellation token.
    pub fn spawn(f: impl FnOnce(Arc<AtomicBool>) -> T + Send + 'static) -> Self {
        let cancellation_token = Arc::new(AtomicBool::new(false));
        let token_clone = Arc::clone(&cancellation_token);
        let thread = std::thread::spawn(move || {
            f(token_clone)
        });
        WorkerThread {
            thread,
            cancellation_token,
        }
    }

    /// Cancel the worker by setting cancellation token to true.
    pub fn cancel(&self) {
        self.cancellation_token.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Cancel the worker and blocking wait for it to finish.
    pub fn join(self) -> std::thread::Result<T> {
        self.cancel();
        self.thread.join()
    }
}

lazy_static! {
    static ref CURRENT_SIGNAL: Mutex<u32> = Mutex::new(0);
}
const MESSAGE: &'static str = "Hello, world! ";

fn worker_thread_main(cancellation_token: Arc<AtomicBool>) {
    let mut tx = CustomTx::new(|value: u32| -> Result<(), Box<dyn Error>> {
        println!("tx/rx: {}", value);
        let mut current = CURRENT_SIGNAL.lock().map_err(|_| "Mutex error")?;
        *current = value;
        Ok(())
    })
        .cancel_on(cancellation_token)
        .interval(Duration::from_millis(500))
        .rtsm(RtsmRanges::new(10..40, 60..90).unwrap())
        .morse_encode::<ITU>();

    for char in iter::repeat(MESSAGE).map(str::chars).flatten() {
        println!("Encoding: {}", char);
        match tx.send(char) {
            Err(e) => {
                println!("Error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

fn main() {
    let worker = WorkerThread::spawn(worker_thread_main);
    println!("started");
    std::thread::sleep(Duration::from_millis(100));

    let mut rx = IteratorRx::new(iter::repeat_with(|| *CURRENT_SIGNAL.lock().unwrap()))
        .interval(Duration::from_millis(250))
        .rtsm(RtsmRanges::new(10..40, 60..90).unwrap())
        .morse_decode::<ITU>();

    loop {
        match rx.recv() {
            Ok(Some(char)) => {
                println!("        Decoded: {:?}", char);
            }
            Ok(None) => {
                println!("        None");
                break;
            }
            Err(e) => {
                println!("        Decode Error: {}. Skipping.", e);
                continue;
            }
        }
    }

    println!("stopping");
    worker.join().unwrap();
}
