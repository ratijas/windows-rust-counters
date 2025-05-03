use log::info;

use win_high::perf::provide::*;
use win_high::perf::types::*;
use win_high::perf::useful::*;
use win_high::perf::values::CounterVal;

use crate::app::*;
use crate::symbols;
use std::sync::{Arc, Mutex};

pub struct MorseCountersProvider {
    timer: ZeroTimeProvider,
    objects: Vec<PerfObjectTypeTemplate>,
    counters: Vec<PerfCounterDefinitionTemplate>,

    app: Arc<Mutex<App>>,
    data: SharedObjectData,
    cache: ObjectData,
    instances: Vec<InstanceId>,
    num_instances: NumInstances,
}

impl MorseCountersProvider {
    pub fn new(app: Arc<Mutex<App>>, data: SharedObjectData) -> Self {
        let typ = CounterTypeDefinition::from_raw(0).unwrap();
        Self {
            timer: ZeroTimeProvider,
            objects: vec![PerfObjectTypeTemplate::new(symbols::MORSE_OBJECT)],
            counters: vec![
                PerfCounterDefinitionTemplate::new(symbols::CHANNEL_SOS, typ),
                PerfCounterDefinitionTemplate::new(symbols::CHANNEL_MOTD, typ),
                PerfCounterDefinitionTemplate::new(symbols::CHANNEL_CUSTOM, typ),
            ],
            app,
            data,
            cache: ObjectData::new(),
            instances: vec![],
            num_instances: NumInstances::NoInstances,
        }
    }
}

impl PerfProvider for MorseCountersProvider {
    fn service_name(&self, for_object: &PerfObjectTypeTemplate) -> &str {
        "Morse"
    }

    fn objects(&self) -> &[PerfObjectTypeTemplate] {
        &*self.objects
    }

    fn time_provider(&self, for_object: &PerfObjectTypeTemplate) -> &dyn PerfTimeProvider {
        &self.timer
    }

    fn counters(&self, for_object: &PerfObjectTypeTemplate) -> &[PerfCounterDefinitionTemplate] {
        &*self.counters
    }

    fn instances<'a>(
        &'a self,
        for_object: &PerfObjectTypeTemplate,
    ) -> Option<Vec<PerfInstanceDefinitionTemplate<'a>>> {
        match self.num_instances {
            NumInstances::NoInstances => None,
            NumInstances::N(_) => Some(self.instances.iter().map(Into::into).collect()),
        }
    }

    fn data<'a>(
        &'a self,
        for_object: &PerfObjectTypeTemplate,
        per_counter: &PerfCounterDefinitionTemplate,
        per_instance: Option<&PerfInstanceDefinitionTemplate<'a>>,
        now: PerfClock,
    ) -> CounterVal<'a> {
        let counter = CounterId::from(per_counter);
        let instance = match per_instance {
            Some(inst) => InstanceId::from(inst),
            None => InstanceId::perf_no_instances(),
        };

        let data = self
            .cache
            .get(counter, instance)
            .unwrap_or(CounterVal::Dword(0));
        info!(
            "get data: object name #{}, counter name #{} => {:?}",
            for_object.name_offset, per_counter.name_offset, data
        );
        data
    }

    fn prepare(&mut self) {
        self.cache = self.data.read();
        let app = self.app.lock().unwrap();
        self.instances = app.instances();
        info!("Instances: {:?}", self.instances);
        self.num_instances = app.num_instances();
    }
}
