use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use log::error;

use morse_stream::*;
use signal_flow::rtsm::*;
use signal_flow::*;
use win_high::perf::useful::*;
use win_high::perf::values::*;
use win_high::prelude::v2::*;

use crate::reg::*;
use crate::strings_providers::{ConstString, RandomJokeProvider, StringsProvider};
use crate::symbols;
use crate::worker::WorkerThread;

/// App manages counters, instances, synchronization etc.
pub struct App {
    running: bool,
    workers: Vec<WorkerThread<()>>,
    shared_data: SharedObjectData,
    counters: Vec<CounterId>,
    instances: Vec<InstanceId>,
    num_instances: NumInstances,
}

impl App {
    pub fn new(shared_data: SharedObjectData) -> Self {
        App {
            running: false,
            workers: vec![],
            shared_data,
            counters: vec![],
            instances: vec![],
            num_instances: NumInstances::N(0),
        }
    }

    pub fn instances(&self) -> Vec<InstanceId> {
        self.instances.clone()
    }

    pub fn num_instances(&self) -> NumInstances {
        self.num_instances
    }

    fn name_for_instance(index: usize, width: usize) -> U16CString {
        let name = format!("Channel {:width$}", index, width = width);
        U16CString::from_str(name).unwrap()
    }

    pub fn start(&mut self) {
        if !self.running {
            self.running = true;

            self.counters.clear();
            self.counters.push(CounterId::new(symbols::CHANNEL_SOS));
            self.counters.push(CounterId::new(symbols::CHANNEL_MOTD));
            self.counters.push(CounterId::new(symbols::CHANNEL_CUSTOM));

            self.instances.clear();
            self.num_instances = get_number_of_instances();
            match self.num_instances {
                NumInstances::NoInstances => self.instances.push(InstanceId::perf_no_instances()),
                NumInstances::N(n) => {
                    let width = n.to_string().len();
                    self.instances.extend((0..(n as usize)).map(|i| {
                        let unique_id = i as i32;
                        let name = Self::name_for_instance(i, width);
                        InstanceId::new(unique_id, &name)
                    }))
                }
            }

            self.shared_data.update(|mut data| {
                for c in self.counters.iter() {
                    for i in self.instances.iter() {
                        data.set(c.clone(), i.clone(), CounterValue::Dword(0));
                    }
                }
                data
            });

            let d = &self.shared_data;
            self.workers.clear();
            let providers = vec![
                Box::new(ConstString::new("SOS")) as Box<dyn StringsProvider + Send>,
                Box::new(RandomJokeProvider::new()),
                Box::new(get_reg_key_strings_provider()),
            ];
            for (counter, strings_provider) in
                self.counters.clone().into_iter().zip(providers.into_iter())
            {
                let builder = WorkerBuilder::new(
                    self.shared_data.clone(),
                    counter,
                    self.instances.clone(),
                    self.num_instances.clone(),
                    strings_provider,
                );
                self.workers.push(builder.build());
            }
        }
    }

    pub fn stop(&mut self) {
        self.running = false;

        for worker in self.workers.iter() {
            worker.cancel();
        }
        for worker in self.workers.drain(..) {
            if let Err(e) = worker.join() {
                error!("Error while stopping global worker: {:?}", e);
            }
        }
    }
}

pub struct WorkerBuilder {
    shared_data: SharedObjectData,
    counter: CounterId,
    instances: Vec<InstanceId>,
    num_instances: NumInstances,
    strings_provider: Box<dyn StringsProvider + Send>,
}

impl WorkerBuilder {
    pub fn new(
        shared_data: SharedObjectData,
        counter: CounterId,
        instances: Vec<InstanceId>,
        num_instances: NumInstances,
        strings_provider: Box<dyn StringsProvider + Send>,
    ) -> Self {
        WorkerBuilder {
            shared_data,
            counter,
            instances,
            num_instances,
            strings_provider,
        }
    }

    pub fn build(self) -> WorkerThread<()> {
        WorkerThread::spawn(move |cancellation_token: Arc<AtomicBool>| {
            self.main(cancellation_token)
        })
    }

    fn main(self, cancellation_token: Arc<AtomicBool>) {
        let WorkerBuilder {
            shared_data,
            counter,
            instances,
            num_instances,
            mut strings_provider,
        } = self;

        let mut rtsm_coders: Vec<(RtsmTx<_>, signal_flow::pair::ReceiverRx<_>)> = Vec::new();
        for (i, _instance) in instances.iter().cloned().enumerate() {
            let (tx, rx) = signal_flow::pair::pair();
            // let ranges = RtsmRanges::new(10..50, 60..100).unwrap();
            let off = 10 + 10 * (i as u32 % 4);
            let on = 60 + 10 * (i as u32 % 4);
            let ranges = RtsmRanges::new(off..off + 10, on..on + 10).unwrap();
            let rtsm = RtsmTx::new(ranges, tx);
            rtsm_coders.push((rtsm, rx));
        }

        let mut tx = CustomTx::new(|signals: Vec<bool>| -> Result<(), Box<dyn Error>> {
            assert_eq!(signals.len(), rtsm_coders.len());
            assert_ne!(instances.len(), 0);

            shared_data.update(|mut data| {
                for i in 0..signals.len() {
                    let signal = signals[i];
                    let instance = &instances[i];
                    let (coder, rx) = &mut rtsm_coders[i];

                    coder.send(signal).unwrap();
                    let value = rx
                        .recv()
                        .unwrap()
                        .ok_or("Unexpected None from one-to-one rx/tx pair")
                        .unwrap();

                    let counter_value = CounterValue::Dword(value);
                    data.set(counter, instance.clone(), counter_value);
                }
                data
            });

            Ok(())
        })
        .cancel_on(cancellation_token)
        .interval(get_tick_interval())
        .chunks(instances.len())
        .morse_encode::<ITU>();

        'outer: loop {
            let string = strings_provider.provide();
            for char in string.chars().chain(" ".chars()) {
                match tx.send(char) {
                    Err(e) => {
                        error!("Worker error : {}", e);
                        break 'outer;
                    }
                    _ => {}
                }
            }
        }
    }
}
