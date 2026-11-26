//! Motor de audio: Rodio + cadena DSP (ecualizador, ReplayGain, espectro) y crossfade.
use crate::{
    dsp::{self, Biquad, EQ_BANDS, EQ_FREQS},
    media::{self, MediaSource, QueueItem},
};
use rodio::{
    source::SeekError, ChannelCount, Decoder, OutputStream, OutputStreamBuilder, SampleRate, Sink,
    Source,
};
use std::{
    fs::File,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, MutexGuard,
    },
    time::{Duration, Instant},
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Función que abre una fuente (red o disco) y puede ejecutarse en otro hilo.
pub type Opener<P> = Box<dyn Fn(&QueueItem) -> Result<P, String> + Send + 'static>;

pub trait AudioEngine {
    /// Fuente ya abierta, lista para reproducir; se prepara en otro hilo y se engancha en el motor.
    type Prepared: Send + 'static;
    /// Devuelve la función que abre una fuente sin tocar el motor (apta para un hilo aparte).
    fn opener(&self) -> Opener<Self::Prepared>;
    /// Empieza a reproducir una fuente abierta. Con `crossfade > 0` y reproducción activa,
    /// la pista anterior se desvanece mientras entra la nueva.
    fn attach(
        &mut self,
        prepared: Self::Prepared,
        item: &QueueItem,
        crossfade: Duration,
    ) -> Result<Option<Duration>, String>;
    /// Abre y empieza a reproducir en el acto (bloquea hasta abrir; para archivos locales).
    fn load(&mut self, item: &QueueItem, crossfade: Duration) -> Result<Option<Duration>, String> {
        let prepared = (self.opener())(item)?;
        self.attach(prepared, item, crossfade)
    }
    /// `true` si el motor puede encadenar la pista siguiente sin silencio (gapless).
    fn supports_gapless(&self) -> bool {
        false
    }
    /// Pone en cola, detrás de la pista que suena, una fuente ya abierta: empieza justo
    /// cuando termina la actual, sin hueco. Devuelve la duración de la pista encadenada.
    fn append_next(
        &mut self,
        _prepared: Self::Prepared,
        _item: &QueueItem,
    ) -> Result<Option<Duration>, String> {
        Err("Este motor no encadena pistas".into())
    }
    /// `true` mientras hay una pista encadenada esperando a que termine la actual.
    fn has_queued_next(&self) -> bool {
        false
    }
    fn pause(&self);
    fn resume(&self);
    fn stop(&self);
    fn volume(&self, value: f32);
    fn seek(&self, position: Duration) -> Result<(), String>;
    fn position(&self) -> Duration;
    fn ended(&self) -> bool;
    fn set_eq(&self, _gains: [f32; EQ_BANDS]) {}
    fn set_eq_enabled(&self, _on: bool) {}
    fn set_replay_gain(&mut self, _on: bool) {}
    /// Últimas muestras mono y su frecuencia de muestreo, para el espectro.
    fn spectrum_samples(&self) -> (Vec<f32>, u32) {
        (Vec::new(), 44100)
    }
    /// Canción en vivo que anuncia la emisora (ICY), si la hay.
    fn stream_title(&self) -> Option<String> {
        None
    }
    /// Mantenimiento periódico (fundidos).
    fn tick(&mut self) {}
}

/// Estado compartido entre el hilo del worker y el hilo de audio de Rodio.
pub struct Shared {
    eq_gains: Mutex<[f32; EQ_BANDS]>,
    eq_version: AtomicU32,
    eq_enabled: AtomicBool,
    tap: Mutex<Vec<f32>>,
    rate: AtomicU32,
}

impl Shared {
    pub fn new() -> Arc<Shared> {
        Arc::new(Shared {
            eq_gains: Mutex::new([0.0; EQ_BANDS]),
            eq_version: AtomicU32::new(1),
            eq_enabled: AtomicBool::new(true),
            tap: Mutex::new(Vec::new()),
            rate: AtomicU32::new(44100),
        })
    }
}

/// Envuelve una fuente aplicando ecualizador, ganancia y captura para el espectro.
pub struct DspSource {
    inner: Box<dyn Source + Send>,
    shared: Arc<Shared>,
    channels: ChannelCount,
    rate: SampleRate,
    filters: Vec<[Biquad; EQ_BANDS]>,
    seen_version: u32,
    eq_on: bool,
    gain: f32,
    channel: usize,
    counter: u32,
    tap: Vec<f32>,
}

impl DspSource {
    pub fn new(inner: Box<dyn Source + Send>, shared: Arc<Shared>, gain: f32) -> Self {
        let channels = inner.channels();
        let rate = inner.sample_rate();
        shared.rate.store(rate, Ordering::Relaxed);
        let mut source = DspSource {
            inner,
            shared,
            channels,
            rate,
            filters: vec![[Biquad::IDENTITY; EQ_BANDS]; (channels as usize).max(1)],
            seen_version: 0,
            eq_on: true,
            gain,
            channel: 0,
            counter: 0,
            tap: Vec::with_capacity(512),
        };
        source.refresh();
        source
    }

    fn refresh(&mut self) {
        self.eq_on = self.shared.eq_enabled.load(Ordering::Relaxed);
        let version = self.shared.eq_version.load(Ordering::Relaxed);
        if version != self.seen_version {
            self.seen_version = version;
            let gains = *lock(&self.shared.eq_gains);
            for bank in self.filters.iter_mut() {
                for (band, filter) in bank.iter_mut().enumerate() {
                    let fresh = Biquad::peaking(self.rate as f32, EQ_FREQS[band], gains[band], 1.2);
                    *filter = fresh.with_state_of(filter);
                }
            }
        }
    }

    fn flush_tap(&mut self) {
        if let Ok(mut shared_tap) = self.shared.tap.try_lock() {
            shared_tap.extend_from_slice(&self.tap);
            let len = shared_tap.len();
            if len > 2048 {
                shared_tap.drain(0..len - 2048);
            }
            self.tap.clear();
        } else if self.tap.len() > 4096 {
            self.tap.clear();
        }
    }
}

impl Iterator for DspSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let x = self.inner.next()?;
        self.counter = self.counter.wrapping_add(1);
        if self.counter % 1024 == 0 {
            self.refresh();
        }
        let channel = self.channel;
        self.channel += 1;
        if self.channel >= self.filters.len() {
            self.channel = 0;
        }
        let mut y = x;
        if self.eq_on {
            for filter in self.filters[channel].iter_mut() {
                y = filter.process(y);
            }
        }
        y = (y * self.gain).clamp(-1.0, 1.0);
        if channel == 0 {
            self.tap.push(y);
            if self.tap.len() >= 256 {
                self.flush_tap();
            }
        }
        Some(y)
    }
}

impl Source for DspSource {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)
    }
}

pub type Opened = (
    Box<dyn Source + Send>,
    Option<Duration>,
    Option<media::StreamTitle>,
);

fn open_source(item: &QueueItem) -> Result<Opened, String> {
    match &item.media {
        MediaSource::File(path) => {
            let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let decoder =
                Decoder::try_from(file).map_err(|e| format!("{}: {e}", path.display()))?;
            let duration = decoder.total_duration();
            Ok((Box::new(decoder), duration, None))
        }
        MediaSource::Url(url) => {
            let (reader, title) = media::open_url(url, item.live)?;
            // `Decoder::new` no asume que el origen sea buscable (rodio >= 0.21).
            let decoder =
                Decoder::new(reader).map_err(|e| format!("Flujo no reproducible: {e}"))?;
            Ok((Box::new(decoder), None, item.live.then_some(title)))
        }
    }
}

struct Fading {
    sink: Sink,
    started: Instant,
    length: Duration,
}

pub struct RodioEngine {
    stream: OutputStream,
    sink: Sink,
    fading_out: Vec<Fading>,
    fade_in: Option<(Instant, Duration)>,
    shared: Arc<Shared>,
    volume: std::cell::Cell<f32>,
    replay_gain: bool,
    /// Título ICY del flujo en vivo actual.
    icy: Option<media::StreamTitle>,
}

impl RodioEngine {
    pub fn new() -> Result<Self, String> {
        let mut stream = OutputStreamBuilder::open_default_stream().map_err(|e| e.to_string())?;
        // Evita que Rodio escriba en la pantalla de la TUI al cerrar.
        stream.log_on_drop(false);
        let sink = Sink::connect_new(stream.mixer());
        Ok(Self {
            stream,
            sink,
            fading_out: Vec::new(),
            fade_in: None,
            shared: Shared::new(),
            volume: std::cell::Cell::new(0.7),
            replay_gain: true,
            icy: None,
        })
    }
}

fn progress(started: Instant, length: Duration) -> f32 {
    if length.is_zero() {
        1.0
    } else {
        (started.elapsed().as_secs_f32() / length.as_secs_f32()).min(1.0)
    }
}

impl AudioEngine for RodioEngine {
    type Prepared = Opened;
    fn opener(&self) -> Opener<Opened> {
        Box::new(open_source)
    }
    fn attach(
        &mut self,
        opened: Opened,
        item: &QueueItem,
        crossfade: Duration,
    ) -> Result<Option<Duration>, String> {
        // Se decodifica antes de tocar nada: una fuente defectuosa conserva la reproducción actual.
        let (source, duration, icy) = opened;
        let gain = if self.replay_gain {
            item.rg_db.map(dsp::db_to_linear).unwrap_or(1.0)
        } else {
            1.0
        };
        let dsp_source = DspSource::new(source, self.shared.clone(), gain.clamp(0.05, 4.0));
        let fade = !crossfade.is_zero() && !self.sink.empty() && !self.sink.is_paused();
        let new_sink = Sink::connect_new(self.stream.mixer());
        new_sink.set_volume(if fade { 0.0 } else { self.volume.get() });
        new_sink.append(dsp_source);
        self.icy = icy;
        let old = std::mem::replace(&mut self.sink, new_sink);
        if fade {
            self.fading_out.push(Fading {
                sink: old,
                started: Instant::now(),
                length: crossfade,
            });
            self.fade_in = Some((Instant::now(), crossfade));
        } else {
            old.stop();
            self.fade_in = None;
        }
        Ok(duration)
    }
    fn supports_gapless(&self) -> bool {
        true
    }
    fn append_next(
        &mut self,
        opened: Opened,
        item: &QueueItem,
    ) -> Result<Option<Duration>, String> {
        if self.sink.empty() {
            return Err("No hay pista en curso a la que encadenar".into());
        }
        // Misma cadena DSP que `attach`, pero en el mismo `Sink`: la cola de Rodio empieza la
        // fuente siguiente en la muestra exacta en que acaba la anterior.
        let (source, duration, _icy) = opened;
        let gain = if self.replay_gain {
            item.rg_db.map(dsp::db_to_linear).unwrap_or(1.0)
        } else {
            1.0
        };
        let dsp_source = DspSource::new(source, self.shared.clone(), gain.clamp(0.05, 4.0));
        self.sink.append(dsp_source);
        Ok(duration)
    }
    fn has_queued_next(&self) -> bool {
        self.sink.len() > 1
    }
    fn pause(&self) {
        self.sink.pause();
    }
    fn resume(&self) {
        self.sink.play();
    }
    fn stop(&self) {
        self.sink.stop();
    }
    fn volume(&self, value: f32) {
        self.volume.set(value);
        if self.fade_in.is_none() {
            self.sink.set_volume(value);
        }
    }
    fn seek(&self, position: Duration) -> Result<(), String> {
        self.sink
            .try_seek(position)
            .map_err(|_| "Esta fuente no admite buscar".to_string())
    }
    fn position(&self) -> Duration {
        self.sink.get_pos()
    }
    fn ended(&self) -> bool {
        self.sink.empty()
    }
    fn set_eq(&self, gains: [f32; EQ_BANDS]) {
        *lock(&self.shared.eq_gains) = gains;
        self.shared.eq_version.fetch_add(1, Ordering::Relaxed);
    }
    fn set_eq_enabled(&self, on: bool) {
        self.shared.eq_enabled.store(on, Ordering::Relaxed);
    }
    fn set_replay_gain(&mut self, on: bool) {
        self.replay_gain = on;
    }
    fn spectrum_samples(&self) -> (Vec<f32>, u32) {
        (
            lock(&self.shared.tap).clone(),
            self.shared.rate.load(Ordering::Relaxed),
        )
    }
    fn stream_title(&self) -> Option<String> {
        let title = self
            .icy
            .as_ref()?
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Some(title).filter(|t| !t.is_empty())
    }
    fn tick(&mut self) {
        let volume = self.volume.get();
        if let Some((started, length)) = self.fade_in {
            let t = progress(started, length);
            self.sink.set_volume(volume * t);
            if t >= 1.0 {
                self.fade_in = None;
            }
        }
        let current = std::mem::take(&mut self.fading_out);
        for fade in current {
            let t = progress(fade.started, fade.length);
            if t >= 1.0 || fade.sink.empty() {
                fade.sink.stop();
            } else {
                fade.sink.set_volume(volume * (1.0 - t));
                self.fading_out.push(fade);
            }
        }
    }
}

/// Motor nulo cuando no hay dispositivo de audio: la interfaz sigue usable.
pub struct NullEngine {
    reason: String,
}

impl NullEngine {
    pub fn new(reason: String) -> Self {
        Self { reason }
    }
}

impl AudioEngine for NullEngine {
    type Prepared = ();
    fn opener(&self) -> Opener<()> {
        let reason = self.reason.clone();
        Box::new(move |_| Err(format!("Audio no disponible: {reason}")))
    }
    fn attach(&mut self, _: (), _: &QueueItem, _: Duration) -> Result<Option<Duration>, String> {
        Err(format!("Audio no disponible: {}", self.reason))
    }
    fn pause(&self) {}
    fn resume(&self) {}
    fn stop(&self) {}
    fn volume(&self, _: f32) {}
    fn seek(&self, _: Duration) -> Result<(), String> {
        Err("Audio no disponible".into())
    }
    fn position(&self) -> Duration {
        Duration::ZERO
    }
    fn ended(&self) -> bool {
        true
    }
}
