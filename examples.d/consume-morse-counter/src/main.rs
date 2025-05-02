use std::{
    collections::VecDeque,
    error::Error,
    io::{stdout, Stdout},
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};
use std::sync::{RwLock, RwLockReadGuard};

use argh::FromArgs;
use crossterm::{
    event::{self, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::KeyEvent;
use figlet_rs::FIGfont;
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use morse_stream::*;
use signal_flow::{*, rtsm::*};
use win_high::{
    perf::{
        consume::{CounterMeta, get_counters_info, UseLocale},
        nom::*,
        values::CounterVal,
    },
    prelude::v1::*,
};
use win_high::perf::consume::AllCounters;
use win_high::perf::useful::InstanceId;

mod reg;
mod ui;

/// Morse decoder from performance counter
#[derive(Debug, FromArgs)]
struct Cli {
    /// counter object type's name index.
    #[argh(positional, default = "crate::reg::get_object_name_index()")]
    object: u32,
    /// time in ms between two ticks.
    #[argh(option, from_str_fn(parse_tick_duration), default = "crate::reg::get_tick_interval()")]
    tick: Duration,
}

fn parse_tick_duration(value: &str) -> Result<Duration, String> {
    let millis = value.parse::<u64>().map_err(|_| "Unable to parse tick duration")?;
    if millis < 10 {
        return Err("Tick duration is too short".into());
    }
    Ok(Duration::from_millis(millis))
}

pub struct Decoder {
    counter: CounterMeta,
    tx: SenderTx<Vec<DataPair>>,
    thread_handle: thread::JoinHandle<()>,
}

#[derive(Clone, Debug)]
pub struct ObjectStats {
    pub meta: CounterMeta,
    pub counters: Vec<CounterStats>,
}

#[derive(Clone, Debug)]
pub struct CounterStats {
    pub meta: CounterMeta,
    pub decoded: String,
    pub signal: VecDeque<bool>,
    pub instances: Vec<InstanceStats>,
}

const HIST_SIZE: usize = 200;

#[derive(Clone, Debug)]
pub struct InstanceStats {
    pub instance_id: InstanceId,
    pub name: String,
    pub signal: VecDeque<DWORD>,
}

impl ObjectStats {
    pub fn new(meta: CounterMeta) -> Self {
        ObjectStats {
            meta,
            counters: vec![],
        }
    }

    pub fn counter_mut(&mut self, meta: &CounterMeta) -> &mut CounterStats {
        let index = self.counters.iter().position(|c| c.meta.name_index == meta.name_index);
        match index {
            Some(index) => &mut self.counters[index],
            None => {
                self.counters.push(CounterStats::new(meta.clone()));
                self.counters.sort_by(|left, right| left.meta.name_index.cmp(&right.meta.name_index));
                self.counters.last_mut().unwrap()
            }
        }
    }
}

impl CounterStats {
    pub fn new(meta: CounterMeta) -> Self {
        CounterStats {
            meta,
            decoded: String::with_capacity(HIST_SIZE),
            signal: VecDeque::with_capacity(HIST_SIZE),
            instances: vec![],
        }
    }

    pub fn instance_mut(&mut self, instance_id: &InstanceId) -> &mut InstanceStats {
        let index = self.instances.iter().position(|i| &i.instance_id == instance_id);
        match index {
            Some(index) => &mut self.instances[index],
            None => {
                self.instances.push(InstanceStats::new(instance_id.clone()));
                self.instances.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
                self.instances.last_mut().unwrap()
            }
        }
    }

    pub fn push_char(&mut self, char: char) {
        if self.decoded.len() >= HIST_SIZE {
            self.decoded.remove(0);
        }
        self.decoded.push(char);
    }

    pub fn push_signal(&mut self, signal: bool) {
        while self.signal.len() >= HIST_SIZE {
            self.signal.pop_front();
        }
        self.signal.push_back(signal);
    }
}

impl InstanceStats {
    pub fn new(instance_id: InstanceId) -> Self {
        InstanceStats {
            name: instance_id.name().to_string_lossy(),
            instance_id,
            signal: VecDeque::with_capacity(HIST_SIZE),
        }
    }

    fn trim<T>(vec: &mut VecDeque<T>) {
        while vec.len() >= HIST_SIZE {
            vec.pop_front();
        }
    }

    pub fn push_signal(&mut self, signal: u32) {
        Self::trim(&mut self.signal);
        self.signal.push_back(signal);
    }
}

pub struct App {
    inner: AppInner,
    view: ViewState,
}

pub type ArcVecStats = Arc<RwLock<ObjectStats>>;

pub struct AppInner {
    all_counters: AllCounters,
    stats: ArcVecStats,
    decoders: Vec<Decoder>,
}

pub struct ViewState {
    font: FIGfont,
    active_counter: DWORD,
}

impl App {
    pub fn new(object_index: u32) -> WinResult<Self> {
        let all_counters = get_counters_info(None, UseLocale::UIDefault)?;
        let object = all_counters.get(object_index).ok_or(WinError::new(ERROR_INVALID_DATA))?;
        let stats = Arc::new(RwLock::new(ObjectStats::new(object.clone())));

        let inner = AppInner {
            all_counters,
            stats,
            decoders: vec![],
        };

        Ok(App {
            inner,
            view: ViewState {
                font: FIGfont::standard().unwrap(),
                active_counter: 0,
            },
        })
    }

    pub fn on_tick(&mut self) -> Result<(), Box<dyn Error>> {
        // on every tick app fetches counters data from registry and sends it into decoders
        let obj_name = self.stats_read().meta.name_index;
        let object_str = obj_name.to_string();

        let buffer = query_value(HKEY_PERFORMANCE_DATA, &*object_str, None, None).unwrap();
        let (_rest, data) = perf_data_block(&*buffer).unwrap();
        let object = data.object_types.iter().find(|o| o.ObjectNameTitleIndex == obj_name).unwrap();
        for counter in object.counters.iter() {
            let counter_meta = self.inner.all_counters.get(counter.CounterNameTitleIndex).unwrap().clone();
            let values = get_as_dword(object, counter);
            // send for further decoding
            self.inner.decode(&counter_meta, values)?;
        }

        Ok(())
    }

    pub fn tab_next(&mut self) {
        // find the first one which is bigger than the current
        let lock = self.stats_read();
        if let Some(new) = lock.counters.iter().map(|s| s.meta.name_index).find(|it| *it > self.view.active_counter) {
            drop(lock);
            self.view.active_counter = new;
        }
    }

    pub fn tab_prev(&mut self) {
        // find the first one which is bigger than the current
        let lock = self.stats_read();
        if let Some(new) = lock.counters.iter().map(|s| s.meta.name_index).rev().find(|it| *it < self.view.active_counter) {
            drop(lock);
            self.view.active_counter = new;
        }
    }

    pub fn stats(&self) -> &ArcVecStats {
        self.inner.stats()
    }

    pub fn stats_read(&self) -> RwLockReadGuard<ObjectStats> {
        self.stats().read().unwrap()
    }

    fn fix_active_counter(&mut self) {
        let found = self.active_counter_index().is_some();
        let empty = self.stats_read().counters.first().is_none();
        if !found && !empty {
            let it = self.stats_read().counters.first().unwrap().meta.name_index;
            self.view.active_counter = it;
        }
    }

    fn active_counter_index(&self) -> Option<usize> {
        self.stats_read().counters.iter().position(|s| s.meta.name_index == self.view.active_counter)
    }
}

impl AppInner {
    pub fn stats(&self) -> &ArcVecStats {
        &self.stats
    }

    pub fn decode(&mut self, counter: &CounterMeta, values: Vec<DataPair>) -> Result<(), Box<dyn Error>> {
        let decoder = self.decoder_for(counter);
        decoder.tx.send(values)
    }

    fn decoder_for(&mut self, counter: &CounterMeta) -> &mut Decoder {
        match self.decoders.iter().position(|d| d.counter.name_index == counter.name_index) {
            Some(i) => &mut self.decoders[i],
            None => {
                self.spawn_decoder(counter)
            }
        }
    }

    fn spawn_decoder(&mut self, counter: &CounterMeta) -> &mut Decoder {
        let (tx, rx) = signal_flow::pair::pair();
        let stats = Arc::clone(self.stats());
        let counter_clone = counter.clone();

        let thread_handle = std::thread::spawn(move || {
            let counter = counter_clone;
            let ranges = RtsmRanges::new(10..50, 60..100).unwrap();

            let mut decoder = rx
                .map(|mut vec: Vec<DataPair>| {
                    let mut lock = stats.write().unwrap();
                    let counter = lock.counter_mut(&counter);
                    for DataPair(instance_id, value) in vec.iter() {
                        let instance = counter.instance_mut(instance_id);
                        instance.push_signal(*value);
                    }
                    // make sure instances are ordered
                    vec.sort_by(|left, right| left.0.cmp(&right.0));
                    vec.into_iter().map(|pair| pair.1)
                })
                .rtsm_multi(|_| ranges.clone())
                .flatten()
                .map(|signal| {
                    let mut lock = stats.write().unwrap();
                    let counter = lock.counter_mut(&counter);
                    counter.push_signal(signal);
                    signal
                })
                .morse_decode::<ITU>()
                .map(|char| {
                    let mut lock = stats.write().unwrap();
                    let counter = lock.counter_mut(&counter);
                    counter.push_char(char);
                    char
                });
            loop {
                match decoder.recv() {
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                    Err(_) => { /*try recover*/ }
                }
            }
        });
        let decoder = Decoder {
            counter: counter.clone(),
            tx,
            thread_handle,
        };
        self.decoders.push(decoder);
        self.decoders.last_mut().unwrap()
    }
}

impl Drop for AppInner {
    fn drop(&mut self) {
        for d in self.decoders.drain(..) {
            let Decoder { tx, thread_handle, .. } = d;
            drop(tx);
            if let Err(e) = thread_handle.join() {
                println!("Thread panic: {:?}", e);
            }
        }
    }
}

enum Event {
    Input(KeyEvent),
    Tick,
}

pub fn main() -> Result<(), Box<dyn Error>> {
    let cli: Cli = argh::from_env();
    let mut app = App::new(cli.object)?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    #[allow(deprecated)]
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    terminal.hide_cursor()?;

    // Setup input handling
    let (tx, rx) = mpsc::channel();

    let tick = cli.tick;
    thread::spawn(move || -> Result<(), ()> {
        loop {
            // poll for tick rate duration, if no events, sent tick event.
            if event::poll(tick).map_err(drop)? {
                if let CEvent::Key(key) = event::read().map_err(drop)? {
                    if key.is_press() {
                        tx.send(Event::Input(key)).map_err(drop)?
                    }
                }
            }
            tx.send(Event::Tick).map_err(drop)?;
        }
    });
    terminal.clear()?;
    loop {
        app.fix_active_counter();

        terminal.draw(|mut f| {
            ui::draw(&mut f, &mut app).unwrap();
        })?;

        match rx.recv()? {
            Event::Input(event) => match event.code {
                KeyCode::Char('q') => {
                    clean_on_exit(&mut terminal)?;
                    break;
                }
                KeyCode::Right => app.tab_next(),
                KeyCode::Left => app.tab_prev(),
                _ => {}
            },
            Event::Tick => {
                if let Err(e) = app.on_tick() {
                    clean_on_exit(&mut terminal)?;
                    println!("Error: {:?}", e);
                    break;
                }
            }
        }
    }
    Ok(())
}

fn clean_on_exit(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    #[allow(deprecated)]
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[derive(Debug)]
pub struct DataPair(InstanceId, DWORD);

fn get_as_dword(object: &PerfObjectType, counter: &PerfCounterDefinition) -> Vec<DataPair> {
    match &object.data {
        PerfObjectData::Singleton(block) => {
            vec![get_as_dword_inner(InstanceId::perf_no_instances(), counter, block)]
        }
        PerfObjectData::Instances(vec) => {
            vec.iter()
                .map(|(instance, block)| get_as_dword_inner(instance.into(), counter, block))
                .collect()
        }
    }
}

fn get_as_dword_inner(instance: InstanceId, counter: &PerfCounterDefinition, block: &PerfCounterBlock) -> DataPair {
    match CounterVal::try_get(counter, block).unwrap() {
        CounterVal::Dword(dword) => DataPair(instance, dword as DWORD),
        _ => unimplemented!(),
    }
}
