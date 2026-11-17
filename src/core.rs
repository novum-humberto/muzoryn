//! Núcleo de reproducción: cola, estado, comandos, modos de repetición/aleatorio,
//! crossfade, espectro y servicio en hilo propio.
use std::{
    process::Stdio,
    sync::{mpsc, Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use serde::{Deserialize, Serialize};
use crate::{
    audio::{AudioEngine, NullEngine, RodioEngine},
    config::{Hook, Settings},
    dsp::{self, EQ_BANDS, SPECTRUM_BANDS},
    media::{MediaSource, QueueItem},
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat { Off, All, One }

impl Repeat {
    pub fn cycle(self) -> Repeat {
        match self { Repeat::Off => Repeat::All, Repeat::All => Repeat::One, Repeat::One => Repeat::Off }
    }
    pub fn as_str(self) -> &'static str {
        match self { Repeat::Off => "off", Repeat::All => "all", Repeat::One => "one" }
    }
    pub fn from_str(text: &str) -> Repeat {
        match text { "all" => Repeat::All, "one" => Repeat::One, _ => Repeat::Off }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Command {
    Play(QueueItem),
    PlayIndex(usize),
    Enqueue(QueueItem),
    EnqueueMany(Vec<QueueItem>),
    Replace { items: Vec<QueueItem>, start: usize },
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    Clear,
    Toggle,
    Stop,
    Next,
    Previous,
    VolumeDelta(f32),
    SeekBy(i64),
    ToggleShuffle,
    CycleRepeat,
    SetEq([f32; EQ_BANDS]),
    SetEqEnabled(bool),
    SetReplayGain(bool),
    SetCrossfade(u32),
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback { Stopped, Playing, Paused }

/// Estado de la conexión de una emisora en directo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamState {
    /// No hay emisora en directo en marcha.
    #[default]
    Idle,
    Connecting,
    Live,
    Reconnecting,
    Unavailable,
}

/// Esperas antes de cada intento de reconexión (el primero es inmediato).
const RETRY_DELAYS_SECS: [u64; 6] = [0, 2, 5, 10, 20, 30];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub queue: Vec<QueueItem>,
    pub current: Option<usize>,
    pub playback: Playback,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub volume: f32,
    pub error: Option<String>,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub eq: [f32; EQ_BANDS],
    pub eq_enabled: bool,
    pub replay_gain: bool,
    pub crossfade_secs: u32,
    pub spectrum: Vec<f32>,
    pub sample_rate: u32,
    /// Canción en vivo que anuncia la emisora (ICY), si la envía.
    #[serde(default)]
    pub now_playing: Option<String>,
    #[serde(default)]
    pub stream: StreamState,
}

impl Snapshot {
    pub fn new(queue: Vec<QueueItem>) -> Self {
        Self {
            queue, current: None, playback: Playback::Stopped,
            position: Duration::ZERO, duration: None, volume: 0.7, error: None,
            shuffle: false, repeat: Repeat::Off, eq: [0.0; EQ_BANDS], eq_enabled: true,
            replay_gain: true, crossfade_secs: 0, spectrum: vec![0.0; SPECTRUM_BANDS],
            sample_rate: 0, now_playing: None, stream: StreamState::Idle,
        }
    }

    pub fn current_item(&self) -> Option<&QueueItem> {
        self.current.and_then(|i| self.queue.get(i))
    }
}

/// Apertura de una fuente de red en curso (se hace en otro hilo para no bloquear el núcleo).
struct Pending<P> {
    index: usize,
    rx: mpsc::Receiver<Result<P, String>>,
    crossfade: Duration,
    /// Reconexión de la emisora que ya sonaba (no cambia de pista ni avisa a los hooks).
    reconnect: bool,
    /// La pista se añadió a la cola solo para esta apertura: se quita si falla.
    pushed: bool,
    prev_current: Option<usize>,
    prev_duration: Option<Duration>,
    prev_stream: StreamState,
}

pub struct Core<E: AudioEngine> {
    pub state: Snapshot,
    engine: E,
    hooks: Vec<Hook>,
    fade_started: bool,
    rng: u64,
    /// Reconexión en curso: (intentos hechos, cuándo toca el siguiente).
    retry: Option<(usize, Instant)>,
    retry_delays: Vec<Duration>,
    pending: Option<Pending<E::Prepared>>,
}

impl<E: AudioEngine> Core<E> {
    pub fn new(engine: E, queue: Vec<QueueItem>) -> Self {
        engine.volume(0.7);
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(88172645463325252);
        Self {
            state: Snapshot::new(queue), engine, hooks: Vec::new(), fade_started: false, rng: seed | 1,
            retry: None, retry_delays: RETRY_DELAYS_SECS.iter().map(|s| Duration::from_secs(*s)).collect(),
            pending: None,
        }
    }

    pub fn apply_settings(&mut self, settings: &Settings) {
        self.state.volume = settings.volume.clamp(0.0, 1.0);
        self.engine.volume(self.state.volume);
        self.state.eq = settings.eq_array();
        self.engine.set_eq(self.state.eq);
        self.state.eq_enabled = settings.eq_enabled;
        self.engine.set_eq_enabled(settings.eq_enabled);
        self.state.replay_gain = settings.replay_gain;
        self.engine.set_replay_gain(settings.replay_gain);
        self.state.crossfade_secs = settings.crossfade_secs.min(12);
        self.state.shuffle = settings.shuffle;
        self.state.repeat = Repeat::from_str(&settings.repeat);
        self.hooks = settings.hooks.clone();
    }

    fn random(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    fn random_other(&mut self, current: Option<usize>) -> usize {
        let len = self.state.queue.len();
        if len <= 1 { return 0; }
        loop {
            let pick = (self.random() % len as u64) as usize;
            if Some(pick) != current { return pick; }
        }
    }

    fn fire_hooks(&self, event: &str) {
        let item = self.state.current_item();
        for hook in self.hooks.iter().filter(|h| h.event == event) {
            let mut command = std::process::Command::new("sh");
            command.arg("-c").arg(&hook.command)
                .env("MUZORYN_EVENT", event)
                .env("MUZORYN_TITLE", item.map(|i| i.title.as_str()).unwrap_or(""))
                .env("MUZORYN_ARTIST", item.map(|i| i.artist.as_str()).unwrap_or(""))
                .env("MUZORYN_ALBUM", item.map(|i| i.album.as_str()).unwrap_or(""))
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            if let Ok(mut child) = command.spawn() {
                thread::spawn(move || { let _ = child.wait(); });
            }
        }
    }

    fn start(&mut self, index: usize) -> Result<(), String> {
        let item = self.state.queue.get(index).ok_or("Índice de cola no válido")?.clone();
        if matches!(item.media, MediaSource::Url(_)) {
            self.begin_open(index, item, false);
            return Ok(());
        }
        let crossfade = if self.state.playback == Playback::Playing {
            Duration::from_secs(self.state.crossfade_secs as u64)
        } else { Duration::ZERO };
        let duration = self.engine.load(&item, crossfade)?;
        self.pending = None;
        self.finish_start(index, &item, duration);
        Ok(())
    }

    /// Estado tras empezar a sonar una pista.
    fn finish_start(&mut self, index: usize, item: &QueueItem, duration: Option<Duration>) {
        self.retry = None;
        self.state.stream = if item.live { StreamState::Live } else { StreamState::Idle };
        self.state.now_playing = None;
        self.state.current = Some(index);
        self.state.duration = duration.or(item.duration);
        self.state.position = Duration::ZERO;
        self.state.playback = Playback::Playing;
        self.fade_started = false;
        self.fire_hooks("track_changed");
    }

    /// Abre una fuente de red en otro hilo; el resultado se recoge en `tick`.
    /// Mientras tanto el núcleo sigue atendiendo órdenes y se muestra «Conectando…».
    fn begin_open(&mut self, index: usize, item: QueueItem, reconnect: bool) {
        let crossfade = if !reconnect && self.state.playback == Playback::Playing {
            Duration::from_secs(self.state.crossfade_secs as u64)
        } else { Duration::ZERO };
        let (prev_current, prev_duration, prev_stream) = match self.pending.take() {
            Some(old) => (old.prev_current, old.prev_duration, old.prev_stream),
            None => (self.state.current, self.state.duration, self.state.stream),
        };
        let open = self.engine.opener();
        let (tx, rx) = mpsc::channel();
        let target = item.clone();
        thread::spawn(move || { let _ = tx.send(open(&target)); });
        if reconnect {
            self.state.stream = StreamState::Reconnecting;
        } else {
            self.state.stream = StreamState::Connecting;
            self.state.current = Some(index);
            self.state.duration = None;
        }
        self.pending = Some(Pending {
            index, rx, crossfade, reconnect, pushed: false, prev_current, prev_duration, prev_stream,
        });
    }

    /// Recoge el resultado de una apertura en segundo plano, si ya terminó.
    fn poll_pending(&mut self) {
        let Some(pending) = self.pending.as_ref() else { return };
        let result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("la conexión se interrumpió".to_string()),
        };
        let Some(p) = self.pending.take() else { return };
        let Some(item) = self.state.queue.get(p.index).cloned() else { return };
        let outcome = result.and_then(|prepared| self.engine.attach(prepared, &item, p.crossfade));
        match (outcome, p.reconnect) {
            (Ok(_), true) => {
                self.retry = None;
                self.state.stream = StreamState::Live;
                self.state.error = None;
                self.state.position = Duration::ZERO;
            }
            (Ok(duration), false) => self.finish_start(p.index, &item, duration),
            (Err(error), true) => self.reconnect_failed(error),
            (Err(error), false) => {
                // Se conserva lo que sonaba antes de intentar abrir la nueva fuente.
                self.state.current = p.prev_current;
                self.state.duration = p.prev_duration;
                self.state.stream = p.prev_stream;
                if p.pushed && self.state.queue.len() == p.index + 1 { self.state.queue.pop(); }
                self.state.error = Some(if item.live { format!("Emisora no disponible: {error}") } else { error });
            }
        }
    }

    /// Descarta una apertura en curso (la pista a la que apuntaba ya no es válida).
    fn cancel_pending(&mut self) {
        if self.pending.take().is_some()
            && matches!(self.state.stream, StreamState::Connecting | StreamState::Reconnecting) {
            self.state.stream = StreamState::Idle;
        }
    }

    fn stop_playback(&mut self) {
        self.pending = None;
        self.engine.stop();
        self.state.playback = Playback::Stopped;
        self.state.position = Duration::ZERO;
        self.state.stream = StreamState::Idle;
        self.state.now_playing = None;
        self.retry = None;
        self.fire_hooks("playback_stopped");
    }

    fn current_is_live(&self) -> bool {
        self.state.current_item().map(|i| i.live).unwrap_or(false)
    }

    /// La emisora en directo se cortó: reintenta con esperas crecientes y, si no hay
    /// manera, se detiene avisando de que no está disponible.
    fn reconnect(&mut self) {
        if self.pending.is_some() { return; }
        let Some(index) = self.state.current else { return };
        let Some(item) = self.state.queue.get(index).cloned() else { return };
        let now = Instant::now();
        let first_wait = self.retry_delays.first().copied().unwrap_or_default();
        let (_, next_at) = *self.retry.get_or_insert((0, now + first_wait));
        self.state.stream = StreamState::Reconnecting;
        if now >= next_at { self.begin_open(index, item, true); }
    }

    fn reconnect_failed(&mut self, error: String) {
        let attempt = self.retry.map(|(n, _)| n).unwrap_or(0) + 1;
        if attempt >= self.retry_delays.len() {
            self.engine.stop();
            self.state.playback = Playback::Stopped;
            self.state.position = Duration::ZERO;
            self.state.stream = StreamState::Unavailable;
            self.state.now_playing = None;
            self.retry = None;
            self.state.error = Some(format!("Emisora no disponible tras {attempt} intentos: {error}"));
            self.fire_hooks("playback_stopped");
        } else {
            self.state.stream = StreamState::Reconnecting;
            self.retry = Some((attempt, Instant::now() + self.retry_delays[attempt]));
        }
    }

    pub fn handle(&mut self, command: Command) -> Result<(), String> {
        if matches!(command, Command::Remove(_) | Command::MoveUp(_) | Command::MoveDown(_) | Command::Clear | Command::Replace { .. }) {
            self.cancel_pending();
        }
        match command {
            Command::Play(item) => {
                if let Some(i) = self.state.queue.iter().position(|q| q.media == item.media) {
                    self.start(i)?;
                } else {
                    self.state.queue.push(item);
                    let index = self.state.queue.len() - 1;
                    if let Err(error) = self.start(index) {
                        self.state.queue.pop();
                        return Err(error);
                    }
                    if let Some(pending) = self.pending.as_mut() { pending.pushed = true; }
                }
            }
            Command::PlayIndex(i) => self.start(i)?,
            Command::Enqueue(item) => self.state.queue.push(item),
            Command::EnqueueMany(items) => self.state.queue.extend(items),
            Command::Replace { items, start } => {
                self.state.queue = items;
                self.state.current = None;
                if self.state.queue.is_empty() {
                    self.stop_playback();
                } else {
                    let first = start.min(self.state.queue.len() - 1);
                    self.start(first)?;
                }
            }
            Command::Remove(i) => {
                if i < self.state.queue.len() {
                    self.state.queue.remove(i);
                    match self.state.current {
                        Some(c) if c == i => {
                            self.state.current = None;
                            self.stop_playback();
                        }
                        Some(c) if c > i => self.state.current = Some(c - 1),
                        _ => {}
                    }
                }
            }
            Command::MoveUp(i) => {
                if i > 0 && i < self.state.queue.len() {
                    self.state.queue.swap(i, i - 1);
                    self.state.current = self.state.current.map(|c| {
                        if c == i { i - 1 } else if c == i - 1 { i } else { c }
                    });
                }
            }
            Command::MoveDown(i) => {
                if i + 1 < self.state.queue.len() {
                    self.state.queue.swap(i, i + 1);
                    self.state.current = self.state.current.map(|c| {
                        if c == i { i + 1 } else if c == i + 1 { i } else { c }
                    });
                }
            }
            Command::Clear => {
                self.state.queue.clear();
                self.state.current = None;
                self.stop_playback();
            }
            Command::Toggle => match self.state.playback {
                Playback::Playing => { self.engine.pause(); self.state.playback = Playback::Paused; }
                Playback::Paused => { self.engine.resume(); self.state.playback = Playback::Playing; }
                Playback::Stopped if !self.state.queue.is_empty() => {
                    self.start(self.state.current.unwrap_or(0))?
                }
                _ => {}
            },
            Command::Stop => self.stop_playback(),
            Command::Next => self.advance(false)?,
            Command::Previous => {
                if self.state.queue.is_empty() { return Ok(()); }
                let current = self.state.current.unwrap_or(0);
                if self.state.position > Duration::from_secs(3) {
                    self.start(current)?;
                } else if current > 0 {
                    self.start(current - 1)?;
                } else if self.state.repeat == Repeat::All {
                    self.start(self.state.queue.len() - 1)?;
                } else {
                    self.start(0)?;
                }
            }
            Command::VolumeDelta(delta) => {
                if !delta.is_finite() { return Err("Volumen no finito".into()); }
                self.state.volume = (self.state.volume + delta).clamp(0.0, 1.0);
                self.engine.volume(self.state.volume);
            }
            Command::SeekBy(delta) => {
                if self.state.playback != Playback::Stopped {
                    let position = self.engine.position();
                    let shift = Duration::from_secs(delta.unsigned_abs());
                    let mut target = if delta < 0 { position.saturating_sub(shift) }
                        else { position.saturating_add(shift) };
                    if let Some(duration) = self.state.duration { target = target.min(duration); }
                    self.engine.seek(target)?;
                    self.state.position = self.engine.position();
                }
            }
            Command::ToggleShuffle => self.state.shuffle = !self.state.shuffle,
            Command::CycleRepeat => self.state.repeat = self.state.repeat.cycle(),
            Command::SetEq(gains) => {
                let mut clean = gains;
                for g in clean.iter_mut() {
                    *g = if g.is_finite() { g.clamp(-dsp::EQ_RANGE_DB, dsp::EQ_RANGE_DB) } else { 0.0 };
                }
                self.state.eq = clean;
                self.engine.set_eq(clean);
            }
            Command::SetEqEnabled(on) => {
                self.state.eq_enabled = on;
                self.engine.set_eq_enabled(on);
            }
            Command::SetReplayGain(on) => {
                self.state.replay_gain = on;
                self.engine.set_replay_gain(on);
            }
            Command::SetCrossfade(secs) => self.state.crossfade_secs = secs.min(12),
            Command::Shutdown => self.engine.stop(),
        }
        Ok(())
    }

    /// Siguiente índice según repetición y aleatorio. `auto` = fin natural de pista.
    fn next_index(&mut self, auto: bool) -> Option<usize> {
        let len = self.state.queue.len();
        if len == 0 { return None; }
        let current = self.state.current;
        if auto && self.state.repeat == Repeat::One { return current; }
        if self.state.shuffle && len > 1 { return Some(self.random_other(current)); }
        match current {
            None => Some(0),
            Some(i) if i + 1 < len => Some(i + 1),
            Some(_) if self.state.repeat == Repeat::All => Some(0),
            Some(_) => None,
        }
    }

    fn advance(&mut self, auto: bool) -> Result<(), String> {
        match self.next_index(auto) {
            Some(next) => self.start(next),
            None => {
                self.stop_playback();
                self.state.position = self.state.duration.unwrap_or_default();
                Ok(())
            }
        }
    }

    fn update_spectrum(&mut self) {
        let playing = self.state.playback == Playback::Playing;
        let (samples, rate) = self.engine.spectrum_samples();
        self.state.sample_rate = rate;
        let target = if playing && !samples.is_empty() {
            dsp::spectrum(&samples, rate, SPECTRUM_BANDS)
        } else {
            vec![0.0; SPECTRUM_BANDS]
        };
        if self.state.spectrum.len() != SPECTRUM_BANDS {
            self.state.spectrum = vec![0.0; SPECTRUM_BANDS];
        }
        for (old, new) in self.state.spectrum.iter_mut().zip(target.iter()) {
            *old = if *new > *old { *new } else { *old * 0.82 + *new * 0.18 };
        }
    }

    pub fn tick(&mut self) {
        self.engine.tick();
        self.update_spectrum();
        self.poll_pending();
        if self.state.playback != Playback::Playing || self.pending.is_some() { return; }
        if self.engine.ended() {
            if self.current_is_live() { self.reconnect(); return; }
            if let Err(error) = self.advance(true) {
                self.state.error = Some(error);
                self.state.playback = Playback::Stopped;
            }
            return;
        }
        self.state.position = self.engine.position();
        self.state.now_playing = if self.current_is_live() { self.engine.stream_title() } else { None };
        let fade = Duration::from_secs(self.state.crossfade_secs as u64);
        if !fade.is_zero() && !self.fade_started {
            if let Some(duration) = self.state.duration {
                if duration > fade * 2 && duration.saturating_sub(self.state.position) <= fade {
                    self.fade_started = true;
                    if let Some(next) = self.next_index(true) {
                        if let Err(error) = self.start(next) { self.state.error = Some(error); }
                    }
                }
            }
        }
    }
}

// ---- servicio ----

pub trait AudioHandle: Send {
    fn send(&self, command: Command) -> Result<(), String>;
    fn snapshot(&self) -> Snapshot;
    /// `true` si el audio vive en un daemon externo.
    fn is_remote(&self) -> bool { false }
}

fn run_loop<E: AudioEngine>(mut core: Core<E>, rx: mpsc::Receiver<Command>, output: Arc<Mutex<Snapshot>>) {
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = core.handle(Command::Shutdown);
                break;
            }
            Ok(command) => { core.state.error = core.handle(command).err(); }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        core.tick();
        *lock(&output) = core.state.clone();
    }
}

pub struct LocalService {
    tx: Mutex<Option<mpsc::SyncSender<Command>>>,
    latest: Arc<Mutex<Snapshot>>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl LocalService {
    pub fn spawn(settings: Settings, queue: Vec<QueueItem>) -> Self {
        let (tx, rx) = mpsc::sync_channel(128);
        let latest = Arc::new(Mutex::new(Snapshot::new(queue.clone())));
        let output = latest.clone();
        let worker = thread::spawn(move || match RodioEngine::new() {
            Ok(engine) => {
                let mut core = Core::new(engine, queue);
                core.apply_settings(&settings);
                run_loop(core, rx, output);
            }
            Err(error) => {
                let mut core = Core::new(NullEngine::new(error.clone()), queue);
                core.apply_settings(&settings);
                core.state.error = Some(format!("Audio no disponible: {error}"));
                run_loop(core, rx, output);
            }
        });
        Self { tx: Mutex::new(Some(tx)), latest, worker: Mutex::new(Some(worker)) }
    }
}

impl AudioHandle for LocalService {
    fn send(&self, command: Command) -> Result<(), String> {
        match lock(&self.tx).as_ref() {
            Some(tx) => tx.try_send(command).map_err(|e| format!("Comando no enviado: {e}")),
            None => Err("Servicio cerrado".into()),
        }
    }
    fn snapshot(&self) -> Snapshot { lock(&self.latest).clone() }
}

impl Drop for LocalService {
    fn drop(&mut self) {
        // Cerrar el canal basta; no se bloquea la UI al salir.
        lock(&self.tx).take();
        if let Some(worker) = lock(&self.worker).take() { let _ = worker.join(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, path::Path, sync::atomic::{AtomicU32, Ordering}};

    #[derive(Default)]
    struct Fake {
        ended: Cell<bool>, position: Cell<Duration>, paused: Cell<bool>, loads: Cell<u32>,
        /// Aperturas que deben fallar antes de volver a funcionar.
        fail: Arc<AtomicU32>,
        title: std::cell::RefCell<Option<String>>,
    }

    impl AudioEngine for Fake {
        type Prepared = ();
        fn opener(&self) -> crate::audio::Opener<()> {
            let fail = self.fail.clone();
            Box::new(move |item| {
                if item.file_path() == Some(Path::new("bad")) { return Err("bad file".into()); }
                if fail.load(Ordering::SeqCst) > 0 { fail.fetch_sub(1, Ordering::SeqCst); return Err("sin red".into()); }
                Ok(())
            })
        }
        fn attach(&mut self, _: (), _: &QueueItem, _: Duration) -> Result<Option<Duration>, String> {
            self.ended.set(false);
            self.position.set(Duration::ZERO);
            self.loads.set(self.loads.get() + 1);
            Ok(Some(Duration::from_secs(100)))
        }
        fn pause(&self) { self.paused.set(true); }
        fn resume(&self) { self.paused.set(false); }
        fn stop(&self) { self.ended.set(true); }
        fn volume(&self, _: f32) {}
        fn seek(&self, p: Duration) -> Result<(), String> { self.position.set(p); Ok(()) }
        fn position(&self) -> Duration { self.position.get() }
        fn ended(&self) -> bool { self.ended.get() }
        fn stream_title(&self) -> Option<String> { self.title.borrow().clone() }
    }

    fn radio() -> QueueItem { QueueItem::stream("Radio", "Jazz", "https://x.test/s", true) }

    fn items(names: &[&str]) -> Vec<QueueItem> {
        names.iter().map(|n| QueueItem::from_path(Path::new(n))).collect()
    }

    #[test]
    fn playback_pause_seek_volume() {
        let mut core = Core::new(Fake::default(), items(&["one"]));
        core.handle(Command::PlayIndex(0)).unwrap();
        core.handle(Command::Toggle).unwrap();
        assert_eq!(core.state.playback, Playback::Paused);
        core.handle(Command::SeekBy(200)).unwrap();
        assert_eq!(core.state.position.as_secs(), 100);
        core.handle(Command::VolumeDelta(9.0)).unwrap();
        assert_eq!(core.state.volume, 1.0);
    }

    #[test]
    fn end_advances_and_bad_file_preserves_current() {
        let mut core = Core::new(Fake::default(), items(&["one", "two"]));
        core.handle(Command::PlayIndex(0)).unwrap();
        assert!(core.handle(Command::Play(QueueItem::from_path(Path::new("bad")))).is_err());
        assert_eq!(core.state.queue.len(), 2);
        assert_eq!(core.state.current, Some(0));
        core.engine.ended.set(true); core.tick();
        assert_eq!(core.state.current, Some(1));
        core.engine.ended.set(true); core.tick();
        assert_eq!(core.state.playback, Playback::Stopped);
    }

    #[test]
    fn repeat_modes() {
        let mut core = Core::new(Fake::default(), items(&["a", "b"]));
        core.handle(Command::PlayIndex(1)).unwrap();
        core.state.repeat = Repeat::All;
        core.engine.ended.set(true); core.tick();
        assert_eq!(core.state.current, Some(0));
        core.state.repeat = Repeat::One;
        core.engine.ended.set(true); core.tick();
        assert_eq!(core.state.current, Some(0));
        assert_eq!(core.state.playback, Playback::Playing);
    }

    #[test]
    fn shuffle_never_repeats_same_index() {
        let mut core = Core::new(Fake::default(), items(&["a", "b", "c", "d"]));
        core.handle(Command::PlayIndex(0)).unwrap();
        core.state.shuffle = true;
        for _ in 0..50 {
            let before = core.state.current;
            core.handle(Command::Next).unwrap();
            assert_ne!(core.state.current, before);
        }
    }

    #[test]
    fn queue_editing_keeps_current_consistent() {
        let mut core = Core::new(Fake::default(), items(&["a", "b", "c"]));
        core.handle(Command::PlayIndex(1)).unwrap();
        core.handle(Command::MoveUp(1)).unwrap();
        assert_eq!(core.state.current, Some(0));
        core.handle(Command::MoveDown(0)).unwrap();
        assert_eq!(core.state.current, Some(1));
        core.handle(Command::Remove(0)).unwrap();
        assert_eq!(core.state.current, Some(0));
        core.handle(Command::Remove(0)).unwrap();
        assert_eq!(core.state.current, None);
        assert_eq!(core.state.playback, Playback::Stopped);
    }

    #[test]
    fn crossfade_starts_next_before_end() {
        let mut core = Core::new(Fake::default(), items(&["a", "b"]));
        core.handle(Command::SetCrossfade(5)).unwrap();
        core.handle(Command::PlayIndex(0)).unwrap();
        core.engine.position.set(Duration::from_secs(96));
        core.tick();
        assert_eq!(core.state.current, Some(1));
        assert_eq!(core.engine.loads.get(), 2);
    }

    #[test]
    fn previous_restarts_when_far_into_track() {
        let mut core = Core::new(Fake::default(), items(&["a", "b"]));
        core.handle(Command::PlayIndex(1)).unwrap();
        core.state.position = Duration::from_secs(10);
        core.handle(Command::Previous).unwrap();
        assert_eq!(core.state.current, Some(1));
        core.handle(Command::Previous).unwrap();
        assert_eq!(core.state.current, Some(0));
    }

    #[test]
    fn eq_values_are_clamped() {
        let mut core = Core::new(Fake::default(), Vec::new());
        core.handle(Command::SetEq([99.0; EQ_BANDS])).unwrap();
        assert!(core.state.eq.iter().all(|g| *g == 12.0));
    }

    /// Hace avanzar el núcleo hasta que termina la apertura en segundo plano.
    fn settle(core: &mut Core<Fake>) {
        for _ in 0..400 {
            core.tick();
            if core.pending.is_none() { return; }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("la apertura no terminó");
    }

    #[test]
    fn opening_a_stream_does_not_block_and_reports_connecting() {
        let mut core = Core::new(Fake::default(), vec![radio()]);
        core.handle(Command::PlayIndex(0)).unwrap();
        assert_eq!(core.state.stream, StreamState::Connecting);
        assert_eq!(core.state.playback, Playback::Stopped);
        settle(&mut core);
        assert_eq!(core.state.stream, StreamState::Live);
        assert_eq!(core.state.playback, Playback::Playing);
        assert_eq!(core.state.current, Some(0));
    }

    #[test]
    fn stop_cancels_a_pending_connection() {
        let mut core = Core::new(Fake::default(), vec![radio()]);
        core.handle(Command::PlayIndex(0)).unwrap();
        core.handle(Command::Stop).unwrap();
        assert!(core.pending.is_none());
        std::thread::sleep(Duration::from_millis(30));
        core.tick();
        assert_eq!(core.state.playback, Playback::Stopped);
        assert_eq!(core.state.stream, StreamState::Idle);
        assert_eq!(core.engine.loads.get(), 0);
    }

    #[test]
    fn live_drop_reconnects_without_skipping_to_next() {
        let mut core = Core::new(Fake::default(), vec![radio(), QueueItem::from_path(Path::new("b"))]);
        core.retry_delays = vec![Duration::ZERO; 4];
        core.handle(Command::PlayIndex(0)).unwrap();
        settle(&mut core);
        assert_eq!(core.state.stream, StreamState::Live);
        core.engine.fail.store(2, Ordering::SeqCst);
        core.engine.ended.set(true);
        for _ in 0..20 {
            core.tick();
            assert_eq!(core.state.current, Some(0));
            if core.state.stream == StreamState::Live { break; }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(core.state.stream, StreamState::Live);
        assert_eq!(core.state.playback, Playback::Playing);
        assert!(core.state.error.is_none());
    }

    #[test]
    fn live_gives_up_after_all_attempts() {
        let mut core = Core::new(Fake::default(), vec![radio()]);
        core.retry_delays = vec![Duration::ZERO; 3];
        core.handle(Command::PlayIndex(0)).unwrap();
        settle(&mut core);
        core.engine.fail.store(99, Ordering::SeqCst);
        core.engine.ended.set(true);
        for _ in 0..400 {
            core.tick();
            if core.state.stream == StreamState::Unavailable { break; }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(core.state.stream, StreamState::Unavailable);
        assert_eq!(core.state.playback, Playback::Stopped);
        assert!(core.state.error.as_deref().unwrap_or("").contains("no disponible"));
    }

    #[test]
    fn failed_connect_keeps_previous_track_and_reports_error() {
        let mut core = Core::new(Fake::default(), vec![QueueItem::from_path(Path::new("a"))]);
        core.handle(Command::PlayIndex(0)).unwrap();
        core.engine.fail.store(1, Ordering::SeqCst);
        core.handle(Command::Play(radio())).unwrap();
        assert_eq!(core.state.queue.len(), 2);
        settle(&mut core);
        assert!(core.state.error.as_deref().unwrap_or("").starts_with("Emisora no disponible"));
        assert_eq!(core.state.stream, StreamState::Idle);
        assert_eq!(core.state.current, Some(0));
        assert_eq!(core.state.queue.len(), 1, "la emisora fallida no queda en la cola");
        assert_eq!(core.state.playback, Playback::Playing);
    }

    #[test]
    fn now_playing_comes_from_stream_and_clears_on_files() {
        let mut core = Core::new(Fake::default(), vec![radio(), QueueItem::from_path(Path::new("b"))]);
        core.handle(Command::PlayIndex(0)).unwrap();
        settle(&mut core);
        assert_eq!(core.state.now_playing, None);
        *core.engine.title.borrow_mut() = Some("Miles Davis - So What".into());
        core.tick();
        assert_eq!(core.state.now_playing.as_deref(), Some("Miles Davis - So What"));
        core.handle(Command::PlayIndex(1)).unwrap();
        core.tick();
        assert_eq!(core.state.now_playing, None);
        assert_eq!(core.state.stream, StreamState::Idle);
    }
}
