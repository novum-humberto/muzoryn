//! MPRIS 2 (D-Bus): teclas multimedia, widgets de escritorio y `playerctl`.
//! Se compila por defecto en Linux y otros sistemas Unix. Corre en el daemon, en hilos propios.
//! Si no hay bus de sesión, el daemon sigue sin MPRIS (se avisa por stderr).
use crate::core::{AudioHandle, Command, LocalService, Playback, Repeat, Snapshot};
use crate::media::MediaSource;
use std::{collections::HashMap, sync::Arc, thread, time::Duration};
use zbus::{blocking::ConnectionBuilder, interface, zvariant::Value};

const BUS_NAME: &str = "org.mpris.MediaPlayer2.muzoryn";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";

pub fn playback_str(p: Playback) -> &'static str {
    match p {
        Playback::Playing => "Playing",
        Playback::Paused => "Paused",
        Playback::Stopped => "Stopped",
    }
}

pub fn loop_str(r: Repeat) -> &'static str {
    match r {
        Repeat::Off => "None",
        Repeat::All => "Playlist",
        Repeat::One => "Track",
    }
}

pub fn loop_parse(s: &str) -> Option<Repeat> {
    match s {
        "None" => Some(Repeat::Off),
        "Playlist" => Some(Repeat::All),
        "Track" => Some(Repeat::One),
        _ => None,
    }
}

fn micros(d: Duration) -> i64 {
    i64::try_from(d.as_micros()).unwrap_or(i64::MAX)
}

/// Metadatos MPRIS de la pista actual (vacío si no hay ninguna).
pub fn metadata(s: &Snapshot) -> HashMap<String, Value<'static>> {
    let mut m: HashMap<String, Value<'static>> = HashMap::new();
    let (Some(i), Some(item)) = (s.current, s.current_item()) else {
        return m;
    };
    let id = format!("/org/muzoryn/track/{i}");
    if let Ok(path) = zbus::zvariant::ObjectPath::try_from(id) {
        m.insert("mpris:trackid".into(), Value::from(path.into_owned()));
    }
    if let Some(d) = s.duration.or(item.duration) {
        m.insert("mpris:length".into(), Value::from(micros(d)));
    }
    let title = if item.live {
        s.now_playing.clone().unwrap_or_else(|| item.title.clone())
    } else {
        item.title.clone()
    };
    m.insert("xesam:title".into(), Value::from(title));
    if !item.artist.is_empty() {
        m.insert(
            "xesam:artist".into(),
            Value::from(vec![item.artist.clone()]),
        );
    }
    if !item.album.is_empty() {
        m.insert("xesam:album".into(), Value::from(item.album.clone()));
    }
    if let MediaSource::Url(u) = &item.media {
        m.insert("xesam:url".into(), Value::from(u.clone()));
    }
    m
}

struct Root;

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "MUZORYN".into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "muzoryn".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".into(), "http".into(), "https".into()]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    svc: Arc<LocalService>,
}

impl Player {
    fn snap(&self) -> Snapshot {
        self.svc.snapshot()
    }
    fn cmd(&self, c: Command) {
        let _ = self.svc.send(c);
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.cmd(Command::Next);
    }
    fn previous(&self) {
        self.cmd(Command::Previous);
    }
    fn pause(&self) {
        if self.snap().playback == Playback::Playing {
            self.cmd(Command::Toggle);
        }
    }
    fn play_pause(&self) {
        self.cmd(Command::Toggle);
    }
    fn stop(&self) {
        self.cmd(Command::Stop);
    }
    fn play(&self) {
        let s = self.snap();
        match s.playback {
            Playback::Playing => {}
            Playback::Paused => self.cmd(Command::Toggle),
            Playback::Stopped => {
                if !s.queue.is_empty() {
                    self.cmd(Command::PlayIndex(s.current.unwrap_or(0)));
                }
            }
        }
    }
    /// `offset` en microsegundos, relativo a la posición actual.
    fn seek(&self, offset: i64) {
        self.cmd(Command::SeekByMicros(offset));
    }
    fn set_position(&self, track_id: zbus::zvariant::ObjectPath<'_>, position: i64) {
        let s = self.snap();
        let Some(i) = s.current else {
            return;
        };
        if track_id.as_str() != format!("/org/muzoryn/track/{i}") || position < 0 {
            return;
        }
        self.cmd(Command::SeekTo(Duration::from_micros(position as u64)));
    }
    fn open_uri(&self, _uri: String) {}

    #[zbus(property)]
    fn playback_status(&self) -> String {
        playback_str(self.snap().playback).into()
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
        loop_str(self.snap().repeat).into()
    }
    #[zbus(property)]
    fn set_loop_status(&self, value: String) {
        let Some(target) = loop_parse(&value) else {
            return;
        };
        let mut cur = self.snap().repeat;
        for _ in 0..2 {
            if cur == target {
                break;
            }
            self.cmd(Command::CycleRepeat);
            cur = cur.cycle();
        }
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_rate(&self, _value: f64) {}
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.snap().shuffle
    }
    #[zbus(property)]
    fn set_shuffle(&self, value: bool) {
        if self.snap().shuffle != value {
            self.cmd(Command::ToggleShuffle);
        }
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, Value<'static>> {
        metadata(&self.snap())
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        f64::from(self.snap().volume)
    }
    #[zbus(property)]
    fn set_volume(&self, value: f64) {
        if value.is_finite() {
            let target = value.clamp(0.0, 1.0) as f32;
            self.cmd(Command::VolumeDelta(target - self.snap().volume));
        }
    }
    #[zbus(property)]
    fn position(&self) -> i64 {
        micros(self.snap().position)
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        let s = self.snap();
        match s.current {
            None => !s.queue.is_empty(),
            Some(i) => {
                i + 1 < s.queue.len() || s.repeat == Repeat::All || (s.shuffle && s.queue.len() > 1)
            }
        }
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        let s = self.snap();
        s.current.is_some()
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        !self.snap().queue.is_empty()
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.snap().playback != Playback::Stopped
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.snap().current_item().is_some_and(|i| !i.live)
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

/// Estado que, si cambia, obliga a emitir `PropertiesChanged`.
#[derive(PartialEq)]
struct Watched {
    playback: Playback,
    repeat: Repeat,
    shuffle: bool,
    volume_pct: u32,
    track: Option<usize>,
    title: Option<String>,
    queue_len: usize,
}

fn watched(s: &Snapshot) -> Watched {
    Watched {
        playback: s.playback,
        repeat: s.repeat,
        shuffle: s.shuffle,
        volume_pct: (s.volume * 100.0).round() as u32,
        track: s.current,
        title: s.now_playing.clone(),
        queue_len: s.queue.len(),
    }
}

/// Registra el servicio en el bus de sesión y lanza el hilo que emite los cambios.
pub fn spawn(svc: Arc<LocalService>) {
    thread::spawn(move || {
        let conn = match ConnectionBuilder::session()
            .and_then(|b| b.name(BUS_NAME))
            .and_then(|b| b.serve_at(OBJECT_PATH, Root))
            .and_then(|b| b.serve_at(OBJECT_PATH, Player { svc: svc.clone() }))
            .and_then(|b| b.build())
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("MPRIS no disponible: {e}");
                return;
            }
        };
        let Ok(iface) = conn.object_server().interface::<_, Player>(OBJECT_PATH) else {
            return;
        };
        let mut last = watched(&svc.snapshot());
        loop {
            thread::sleep(Duration::from_millis(400));
            let now = watched(&svc.snapshot());
            if now == last {
                continue;
            }
            let ctx = iface.signal_context().clone();
            let player = iface.get();
            zbus::block_on(async {
                let _ = player.playback_status_changed(&ctx).await;
                let _ = player.loop_status_changed(&ctx).await;
                let _ = player.shuffle_changed(&ctx).await;
                let _ = player.volume_changed(&ctx).await;
                let _ = player.metadata_changed(&ctx).await;
                let _ = player.can_go_next_changed(&ctx).await;
                let _ = player.can_go_previous_changed(&ctx).await;
                let _ = player.can_play_changed(&ctx).await;
                let _ = player.can_seek_changed(&ctx).await;
            });
            last = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::QueueItem;

    #[test]
    fn loop_roundtrip() {
        for r in [Repeat::Off, Repeat::All, Repeat::One] {
            assert_eq!(loop_parse(loop_str(r)), Some(r));
        }
        assert_eq!(loop_parse("x"), None);
    }

    #[test]
    fn metadata_vacia_sin_pista() {
        assert!(metadata(&Snapshot::new(Vec::new())).is_empty());
    }

    #[test]
    fn metadata_con_pista() {
        let mut item = QueueItem::from_path(std::path::Path::new("/m/a.flac"));
        item.artist = "Artista".into();
        item.duration = Some(Duration::from_secs(2));
        let mut s = Snapshot::new(vec![item]);
        s.current = Some(0);
        let m = metadata(&s);
        assert!(m.contains_key("mpris:trackid"));
        assert!(m.contains_key("xesam:artist"));
        assert_eq!(m.get("xesam:title"), Some(&Value::from("a".to_string())));
    }
}
