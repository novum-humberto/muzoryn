//! Rutas y ajustes persistentes (settings.toml).
use std::{env, fs, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_dir() -> PathBuf {
    if let Some(p) = env::var_os("MUZORYN_CONFIG_DIR") { return PathBuf::from(p); }
    if let Some(p) = env::var_os("XDG_CONFIG_HOME") { return PathBuf::from(p).join("muzoryn"); }
    home().join(".config").join("muzoryn")
}

pub fn data_dir() -> PathBuf {
    if let Some(p) = env::var_os("MUZORYN_DATA_DIR") { return PathBuf::from(p); }
    if let Some(p) = env::var_os("XDG_DATA_HOME") { return PathBuf::from(p).join("muzoryn"); }
    home().join(".local").join("share").join("muzoryn")
}

pub fn socket_path() -> PathBuf {
    if let Some(p) = env::var_os("MUZORYN_SOCKET") { return PathBuf::from(p); }
    if let Some(p) = env::var_os("XDG_RUNTIME_DIR") { return PathBuf::from(p).join("muzoryn.sock"); }
    let user = env::var("USER").unwrap_or_else(|_| "user".into());
    env::temp_dir().join(format!("muzoryn-{user}.sock"))
}

pub fn settings_path() -> PathBuf { config_dir().join("settings.toml") }
pub fn database_path() -> PathBuf { data_dir().join("library.db") }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Station {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub bitrate: u32,
    /// Marcada como favorita (★).
    #[serde(default, skip_serializing_if = "is_false")]
    pub fav: bool,
    /// `stationuuid` de Radio Browser, si la emisora vino de allí.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uuid: String,
    /// Género(s) de la emisora, usado como respaldo cuando no llegan metadatos de canción.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tags: String,
}

fn is_false(v: &bool) -> bool { !*v }

impl Station {
    pub fn new(name: impl Into<String>, url: impl Into<String>, bitrate: u32) -> Self {
        Self { name: name.into(), url: url.into(), bitrate, fav: false, uuid: String::new(), tags: String::new() }
    }

    /// Subtítulo para el reproductor: primer género o «Radio en directo».
    pub fn subtitle(&self) -> String {
        let genre = self.tags.split(',').map(str::trim).find(|t| !t.is_empty());
        genre.map(str::to_string).unwrap_or_else(|| "Radio en directo".into())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodcastFeed {
    pub name: String,
    pub url: String,
}

/// Plugin simple: ejecuta un comando de shell cuando ocurre un evento.
/// Eventos: `track_changed`, `playback_stopped`. Variables de entorno:
/// MUZORYN_TITLE, MUZORYN_ARTIST, MUZORYN_ALBUM.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hook {
    pub event: String,
    pub command: String,
}

/// Los valores simples van antes que las tablas para que TOML pueda serializarse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    /// Estilo del visualizador (0..10, ver `widgets::VIZ_NAMES`).
    pub visualizer: usize,
    pub volume: f32,
    pub eq_enabled: bool,
    pub replay_gain: bool,
    pub crossfade_secs: u32,
    /// Avisa a Radio Browser (anónimo, sin datos personales) al reproducir una emisora suya.
    pub radio_click_count: bool,
    /// Tienda de Apple Podcasts (código de país) para buscar podcasts en español.
    pub podcast_country: String,
    pub shuffle: bool,
    pub repeat: String,
    /// Última vista y panel abiertos: se restauran al volver a abrir la interfaz.
    pub last_view: usize,
    pub last_focus: usize,
    pub music_dirs: Vec<PathBuf>,
    pub eq: Vec<f32>,
    /// URLs de las últimas emisoras escuchadas, la más reciente primero (máx. 10).
    pub recent_stations: Vec<String>,
    pub stations: Vec<Station>,
    pub podcasts: Vec<PodcastFeed>,
    pub hooks: Vec<Hook>,
}

impl Default for Settings {
    fn default() -> Self {
        let st = |name: &str, url: &str, bitrate| Station::new(name, url, bitrate);
        Self {
            theme: "nocturne".into(),
            visualizer: 0,
            volume: 0.7,
            eq_enabled: true,
            replay_gain: true,
            crossfade_secs: 0,
            radio_click_count: true,
            podcast_country: crate::podcast_directory::DEFAULT_COUNTRY.to_string(),
            shuffle: false,
            repeat: "off".into(),
            last_view: 0,
            last_focus: 0,
            music_dirs: vec![home().join("Music")],
            eq: vec![0.0; 10],
            recent_stations: Vec::new(),
            stations: vec![
                st("SomaFM Groove Salad", "https://ice1.somafm.com/groovesalad-128-mp3", 128),
                st("SomaFM Drone Zone", "https://ice1.somafm.com/dronezone-128-mp3", 128),
                st("SomaFM Secret Agent", "https://ice1.somafm.com/secretagent-128-mp3", 128),
                st("SomaFM Lush", "https://ice1.somafm.com/lush-128-mp3", 128),
                st("Radio Paradise", "https://stream.radioparadise.com/mp3-128", 128),
            ],
            podcasts: vec![
                PodcastFeed { name: "NPR Up First".into(), url: "https://feeds.npr.org/510318/podcast.xml".into() },
                PodcastFeed { name: "NPR Short Wave".into(), url: "https://feeds.npr.org/510351/podcast.xml".into() },
            ],
            hooks: Vec::new(),
        }
    }
}

/// Acepta URLs sin esquema (añade https://) y rechaza lo que no sea http(s).
pub fn normalize_url(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() { return Err("Escribe una URL".into()); }
    if t.chars().any(char::is_whitespace) { return Err("La URL no puede contener espacios".into()); }
    let url = if t.contains("://") { t.to_string() } else { format!("https://{t}") };
    if !(url.starts_with("http://") || url.starts_with("https://")) { return Err("Solo se admiten URLs http o https".into()); }
    if url.split("://").nth(1).map(|r| r.len() < 3 || !r.contains('.') && !r.starts_with("localhost")).unwrap_or(true) {
        return Err("La URL no parece válida".into());
    }
    Ok(url)
}

pub const MAX_RECENT: usize = 10;

impl Settings {
    /// Registra una emisora escuchada: queda la primera, sin duplicados, hasta `MAX_RECENT`.
    pub fn push_recent(&mut self, url: &str) {
        self.recent_stations.retain(|u| u != url);
        self.recent_stations.insert(0, url.to_string());
        self.recent_stations.truncate(MAX_RECENT);
    }

    /// Añade las emisoras y podcasts de `other` que aún no estén (por URL).
    /// Devuelve cuántos elementos nuevos se incorporaron.
    pub fn merge_sources(&mut self, other: &Settings) -> usize {
        let mut added = 0;
        for st in &other.stations {
            if !self.stations.iter().any(|s| s.url == st.url) { self.stations.push(st.clone()); added += 1; }
        }
        for p in &other.podcasts {
            if !self.podcasts.iter().any(|q| q.url == p.url) { self.podcasts.push(p.clone()); added += 1; }
        }
        added
    }

    pub fn load(path: &Path) -> Result<Settings, String> {
        if !path.exists() { return Ok(Settings::default()); }
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut settings: Settings = toml::from_str(&text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        settings.sanitize();
        Ok(settings)
    }

    pub fn sanitize(&mut self) {
        self.eq.resize(10, 0.0);
        for g in self.eq.iter_mut() { *g = if g.is_finite() { g.clamp(-12.0, 12.0) } else { 0.0 }; }
        self.volume = if self.volume.is_finite() { self.volume.clamp(0.0, 1.0) } else { 0.7 };
        self.crossfade_secs = self.crossfade_secs.min(12);
        self.last_view = self.last_view.min(11);
        self.last_focus = self.last_focus.min(1);
        if !matches!(self.repeat.as_str(), "off" | "all" | "one") { self.repeat = "off".into(); }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())
    }

    pub fn eq_array(&self) -> [f32; 10] {
        let mut out = [0.0; 10];
        for (i, g) in self.eq.iter().take(10).enumerate() { out[i] = *g; }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_through_toml() {
        let mut s = Settings::default();
        s.hooks.push(Hook { event: "track_changed".into(), command: "true".into() });
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }
    #[test]
    fn sanitize_clamps_values() {
        let mut s = Settings::default();
        s.eq = vec![99.0; 3];
        s.volume = f32::NAN;
        s.repeat = "raro".into();
        s.sanitize();
        assert_eq!(s.eq.len(), 10);
        assert!(s.eq.iter().all(|g| *g <= 12.0));
        assert_eq!(s.repeat, "off");
    }
    #[test]
    fn urls_are_normalized() {
        assert_eq!(normalize_url(" radio.example.com/live ").unwrap(), "https://radio.example.com/live");
        assert_eq!(normalize_url("http://a.test/x").unwrap(), "http://a.test/x");
        assert!(normalize_url("ftp://a.test").is_err());
        assert!(normalize_url("dos palabras.com").is_err());
        assert!(normalize_url("").is_err());
    }
    #[test]
    fn merge_adds_only_new_sources() {
        let mut a = Settings::default();
        let mut b = Settings::default();
        b.stations.push(Station::new("Nueva", "https://n.test/s", 0));
        b.podcasts.push(PodcastFeed { name: "P".into(), url: "https://p.test/f.xml".into() });
        assert_eq!(a.merge_sources(&b), 2);
        assert_eq!(a.merge_sources(&b), 0);
    }
    #[test]
    fn partial_file_uses_defaults() {
        let s: Settings = toml::from_str("volume = 0.3").unwrap();
        assert_eq!(s.volume, 0.3);
        assert!(!s.stations.is_empty());
    }
}

#[cfg(test)]
mod radio_tests {
    use super::*;

    #[test]
    fn recent_stations_are_unique_newest_first_and_capped() {
        let mut s = Settings::default();
        for i in 0..15 { s.push_recent(&format!("https://r{i}.test/s")); }
        s.push_recent("https://r5.test/s");
        assert_eq!(s.recent_stations.len(), MAX_RECENT);
        assert_eq!(s.recent_stations[0], "https://r5.test/s");
        assert_eq!(s.recent_stations.iter().filter(|u| *u == "https://r5.test/s").count(), 1);
    }

    #[test]
    fn favorites_and_old_files_roundtrip() {
        let mut s = Settings::default();
        s.stations[0].fav = true;
        s.stations[0].uuid = "960e57c5-0601-11e8-ae97-52543be04c81".into();
        s.push_recent(&s.stations[0].url.clone());
        let back: Settings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(s, back);
        // Un settings.toml anterior (sin estos campos) sigue cargando.
        let old: Settings = toml::from_str("[[stations]]\nname = \"A\"\nurl = \"https://a.test/s\"\n").unwrap();
        assert!(!old.stations[0].fav && old.recent_stations.is_empty() && old.radio_click_count);
        assert_eq!(old.podcast_country, "mx");
    }

    #[test]
    fn subtitle_falls_back_to_live_radio() {
        let mut st = Station::new("X", "https://x.test/s", 0);
        assert_eq!(st.subtitle(), "Radio en directo");
        st.tags = " , jazz, blues".into();
        assert_eq!(st.subtitle(), "jazz");
    }
}
