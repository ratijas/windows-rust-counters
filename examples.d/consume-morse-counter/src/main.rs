use std::{
    error::Error,
    io::{stdout, Write},
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use argh::FromArgs;
use crossterm::{
    event::{self, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{
    backend::CrosstermBackend,
    Terminal,
};

use morse_stream::*;
use signal_flow::{*, rtsm::*};
use win_high::{
    perf::{
        consume::{CounterMeta, get_counters_info, UseLocale},
        nom::*,
        values::CounterValue,
    },
    prelude::v1::*,
};
use win_high::perf::consume::AllCounters;
use std::sync::{RwLock, RwLockWriteGuard, RwLockReadGuard};
use crossterm::event::KeyEvent;
use figlet_rs::FIGfont;

mod ui;

/// Morse decoder from performance counter
#[derive(Debug, FromArgs)]
struct Cli {
    /// counter object type's name index.
    #[argh(positional)]
    object: u32,
    /// time in ms between two ticks.
    #[argh(option, default = "250")]
    tick: u32,
}

pub struct Decoder {
    counter: CounterMeta,
    tx: SenderTx<DWORD>,
    thread_handle: thread::JoinHandle<()>,
}

#[derive(Clone, Debug)]
pub struct Stats {
    pub counter: CounterMeta,
    pub decoded: String,
    pub signal_bool: Vec<bool>,
    pub signal_raw: Vec<u32>,
}

pub type VecStats = Vec<Stats>;

pub type ArcVecStats = Arc<RwLock<VecStats>>;

impl Stats {
    pub fn new(counter: CounterMeta) -> Self {
        Stats {
            counter,
            decoded: String::with_capacity(100),
            signal_bool: Vec::with_capacity(100),
            signal_raw: Vec::with_capacity(100),
        }
    }

    fn drop_first<T>(vec: &mut Vec<T>) {
        if vec.len() >= 100 {
            vec.remove(0);
        }
    }

    pub fn push_raw(&mut self, raw: u32) {
        Self::drop_first(&mut self.signal_raw);
        self.signal_raw.push(raw);
    }

    pub fn push_bool(&mut self, bool: bool) {
        Self::drop_first(&mut self.signal_bool);
        self.signal_bool.push(bool);
    }

    pub fn push_char(&mut self, char: char) {
        if self.decoded.len() >= 100 {
            self.decoded.remove(0);
        }
        self.decoded.push(char);
    }
}

pub struct App {
    pub title: String,
    inner: AppInner,
    view: ViewState,
}

pub struct AppInner {
    object: CounterMeta,
    all_counters: AllCounters,
    stats: ArcVecStats,
    decoders: Vec<Decoder>,
}

pub struct ViewState {
    font: FIGfont,
    active_counter: DWORD,
}

impl App {
    pub fn new(title: &str, object_index: u32) -> WinResult<Self> {
        let all_counters = get_counters_info(None, UseLocale::UIDefault)?;
        let object = all_counters.get(object_index).ok_or(WinError::new(ERROR_INVALID_DATA))?;
        let stats = Arc::new(RwLock::new(vec![]));

        let inner = AppInner {
            object: object.clone(),
            all_counters,
            stats,
            decoders: vec![],
        };

        Ok(App {
            title: title.to_string(),
            inner,
            view: ViewState {
                font: FIGfont::standand().unwrap(),
                active_counter: 0,
            }
        })
    }

    pub fn on_tick(&mut self) -> Result<(), ()> {
        // on every tick app fetches counters data from registry and sends it into decoders
        let obj_name = self.inner.object.name_index;
        let object_str = obj_name.to_string();

        let buffer = query_value(HKEY_PERFORMANCE_DATA, &*object_str, None, None).unwrap();
        let (_rest, data) = perf_data_block(&*buffer).unwrap();
        let object = data.object_types.iter().find(|o| o.ObjectNameTitleIndex == obj_name).unwrap();
        for counter in object.counters.iter() {
            let meta = self.inner.all_counters.get(counter.CounterNameTitleIndex).unwrap().clone();
            let value = get_as_dword(object, counter);
            AppInner::store_raw(Arc::clone(self.stats()), &meta, value);
            // send for further decoding
            self.inner.decode(&meta, value);
        }

        Ok(())
    }

    pub fn tab_next(&mut self) {
        // find the first one which is bigger than the current
        let lock = self.stats_read();
        if let Some(new) = lock.iter().map(|s| s.counter.name_index).find(|it| *it > self.view.active_counter) {
            drop(lock);
            self.view.active_counter = new;
        }
    }

    pub fn tab_prev(&mut self) {
        // find the first one which is bigger than the current
        let lock = self.stats_read();
        if let Some(new) = lock.iter().map(|s| s.counter.name_index).rev().find(|it| *it < self.view.active_counter) {
            drop(lock);
            self.view.active_counter = new;
        }
    }

    pub fn object(&self) -> CounterMeta {
        self.inner.object().clone()
    }

    pub fn stats(&self) -> &ArcVecStats {
        self.inner.stats()
    }

    pub fn stats_read(&self) -> RwLockReadGuard<VecStats> {
        self.stats().read().unwrap()
    }

    pub fn stats_write(&self) -> RwLockWriteGuard<VecStats> {
        self.stats().write().unwrap()
    }
}

impl AppInner {
    pub fn object(&self) -> &CounterMeta {
        &self.object
    }

    pub fn stats(&self) -> &ArcVecStats {
        &self.stats
    }

    fn stats_mut<T>(stats: ArcVecStats, counter: &CounterMeta, f: impl FnOnce(&mut Stats) -> T) -> T {
        let mut lock = stats.write().unwrap();
        let stat = match lock.iter().position(|s| s.counter.name_index == counter.name_index) {
            Some(i) => &mut lock[i],
            None => {
                lock.push(Stats::new(counter.clone()));
                lock.sort_by(|left, right| left.counter.name_index.cmp(&right.counter.name_index));
                lock.last_mut().unwrap()
            }
        };
        let result = f(stat);
        drop(lock);
        result
    }

    fn store_raw(stats: ArcVecStats, counter: &CounterMeta, raw: DWORD) {
        Self::stats_mut(stats, counter, |stat| stat.push_raw(raw));
    }

    fn store_signal(stats: ArcVecStats, counter: &CounterMeta, signal: bool) {
        Self::stats_mut(stats, counter, |stat| stat.push_bool(signal));
    }

    fn store_char(stats: ArcVecStats, counter: &CounterMeta, char: char) {
        Self::stats_mut(stats, counter, |stat| stat.push_char(char));
    }

    pub fn decode(&mut self, counter: &CounterMeta, value: DWORD) {
        let decoder = self.decoder_for(counter);
        decoder.tx.send(value).unwrap();
    }

    fn decoder_for(&mut self, counter: &CounterMeta) -> &mut Decoder {
        match self.decoders.iter().position(|d| d.counter.name_index == counter.name_index) {
            Some(i) => {
                &mut self.decoders[i]
            },
            None => {
                self.spawn_decoder(counter)
            },
        }
    }

    fn spawn_decoder(&mut self, counter: &CounterMeta) -> &mut Decoder {
        let (tx, rx) = signal_flow::pair::pair();
        let stats = Arc::clone(self.stats());
        let counter_clone = counter.clone();
        let thread_handle = std::thread::spawn(move || {
            let counter = counter_clone;
            let mut decoder = rx.rtsm(RtsmRanges::new(10..40, 60..90).unwrap())
                .map(|signal| {
                    AppInner::store_signal(Arc::clone(&stats), &counter, signal);
                    signal
                })
                .morse_decode::<ITU>()
                .map(|char| {
                    AppInner::store_char(Arc::clone(&stats), &counter, char);
                    char
                });
            loop {
                match decoder.recv() {
                    Ok(None) => break,
                    Ok(Some(_)) => {},
                    Err(_) => {/*try recover*/},
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
            thread_handle.join().unwrap();
        }
    }
}

enum Event {
    Input(KeyEvent),
    Tick,
}

pub fn main() -> Result<(), Box<dyn Error>> {
    let cli: Cli = argh::from_env();
    let mut app = App::new("Morse counters", cli.object)?;

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
            if event::poll(Duration::from_millis(tick as _)).map_err(drop)? {
                if let CEvent::Key(key) = event::read().map_err(drop)? {
                        tx.send(Event::Input(key)).map_err(drop)?
                }
            }
            tx.send(Event::Tick).map_err(drop)?;
        }
    });
    terminal.clear()?;
    loop {
        terminal.draw(|mut f| {
            ui::draw(&mut f, &mut app).unwrap();
        })?;
        let value = rx.recv()?;
        match value {
            Event::Input(event) => match event.code {
                KeyCode::Char('q') => {
                    disable_raw_mode()?;
                    #[allow(deprecated)]
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    terminal.show_cursor()?;
                    break;
                }
                KeyCode::Right => app.tab_next(),
                KeyCode::Left => app.tab_prev(),
                _ => {}
            },
            Event::Tick => {
                app.on_tick().expect("tick error");
            }
        }
    }
    Ok(())
}

fn get_as_dword(object: &PerfObjectType, counter: &PerfCounterDefinition) -> DWORD {
    match &object.data {
        PerfObjectData::Singleton(block) => {
            match CounterValue::try_get(counter, block).unwrap() {
                CounterValue::Large(large) => large as u32,
                CounterValue::Dword(dword) => dword as u32,
                _ => unimplemented!(),
            }
        }
        _ => unimplemented!()
    }
}
