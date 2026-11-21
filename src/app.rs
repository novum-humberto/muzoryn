//! Estado de la interfaz y lógica de acciones de las 12 vistas.
use crate::{
    config::{self, PodcastFeed, Settings, Station},
    core::{Command, Playback, Snapshot},
    cover::{self, CoverSource, CoverState, Thumb},
    downloads::{DownloadManager, DownloadMode},
    dsp::{self, EQ_BANDS, EQ_PRESETS},
    feeds::{self, Episode},
    keys::Action,
    library::{self, Library, ScanStats, Track},
    lyrics::Lyrics,
    media::{self, MediaSource, QueueItem},
    playlist_io,
    podcast_directory::{self, ShowResult, CATEGORIES},
    radio_browser::{self, StationResult},
    theme::Theme,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Inicio,
    Biblioteca,
    Busqueda,
    Cola,
    Albumes,
    Listas,
    Radio,
    Podcasts,
    Letras,
    Audio,
    Completa,
    Ajustes,
}

impl View {
    pub const ALL: [View; 12] = [
        View::Inicio,
        View::Biblioteca,
        View::Busqueda,
        View::Cola,
        View::Albumes,
        View::Listas,
        View::Radio,
        View::Podcasts,
        View::Letras,
        View::Audio,
        View::Completa,
        View::Ajustes,
    ];
    pub fn title(self) -> &'static str {
        match self {
            View::Inicio => "Inicio",
            View::Biblioteca => "Biblioteca",
            View::Busqueda => "Búsqueda",
            View::Cola => "Cola",
            View::Albumes => "Álbumes y artistas",
            View::Listas => "Listas",
            View::Radio => "Radio",
            View::Podcasts => "Podcasts",
            View::Letras => "Letras",
            View::Audio => "Audio y ecualizador",
            View::Completa => "Vista completa",
            View::Ajustes => "Ajustes",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            View::Albumes => "Álbumes",
            View::Audio => "Audio",
            View::Completa => "Completa",
            other => other.title(),
        }
    }
    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
    /// Etiqueta de la tecla que abre la vista.
    pub fn key_label(self) -> &'static str {
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "v", ","][self.index()]
    }
}

#[derive(Default)]
pub struct Cursor {
    pub index: usize,
    pub state: ListState,
}

impl Cursor {
    pub fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.index = 0;
        } else if self.index >= len {
            self.index = len - 1;
        }
    }
    pub fn step(&mut self, len: usize, delta: i32) {
        if len == 0 {
            self.index = 0;
            return;
        }
        self.index = (self.index as i64 + delta as i64).rem_euclid(len as i64) as usize;
    }
    pub fn sync(&mut self, len: usize) {
        self.clamp(len);
        self.state
            .select(if len == 0 { None } else { Some(self.index) });
    }
}

/// Resumen visual de un lote de episodios del canal, cuyos trabajos viven en DownloadManager.
#[derive(Clone, Debug)]
pub struct DlChoice {
    pub episodes: Vec<Episode>,
    /// Opción resaltada (también la que se aplica con Enter).
    pub mode: DownloadMode,
    /// Marcados que se omiten por estar ya descargados o en curso.
    pub skipped: usize,
}

pub struct BulkDownload {
    pub name: String,
    pub done: usize,
    pub total: usize,
    pub failed: usize,
    /// Título y bytes del episodio del canal que informa progreso actualmente.
    pub current: String,
    pub received: u64,
    pub size: Option<u64>,
    pub cancelled: bool,
    pub cancelled_jobs: usize,
}

/// Aviso flotante que se muestra unos segundos (p. ej. al terminar una descarga).
pub struct Toast {
    pub text: String,
    pub ok: bool,
    until: u64,
}

/// Sección del directorio de podcasts (←/→ alterna Populares y Categorías; / abre Resultados).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodcastSection {
    Popular,
    Categories,
    Results,
}

/// Sección de «Mis emisoras» que se está viendo en Radio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioFilter {
    All,
    Favs,
    Recent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Search,
    RadioSearch,
    PodcastSearch,
    NewList,
    RenameList,
    ImportM3u,
    ExportList,
    ExportRadioName,
    ExportRadioFormat,
    AddStation,
    NameStation,
    AddPodcast,
    NamePodcast,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub mode: InputMode,
    pub text: String,
}

/// Navegador modal del sistema para localizar archivos de playlist.
#[derive(Clone, Debug)]
pub struct DirectoryPicker {
    pub current: PathBuf,
    pub entries: Vec<PathBuf>,
    pub selected: usize,
    pub purpose: PickerPurpose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerPurpose {
    ImportPlaylist,
    ImportRadioPlaylist,
    MusicFolder,
}

/// Compara rutas de carpetas tolerando rutas relativas, componentes `..` y
/// enlaces simbólicos cuando el sistema puede resolverlos.
fn same_directory(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| normalize_absolute(a));
    let b = b.canonicalize().unwrap_or_else(|_| normalize_absolute(b));
    a == b
}

fn normalize_absolute(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[derive(Clone, Debug, PartialEq)]
pub enum SearchRow {
    Header(String),
    Track(usize),
    Album(usize),
    Artist(usize),
}

/// Álbum, artista o género con las pistas (índices en `App::tracks`) que contiene.
#[derive(Clone, Debug)]
pub struct Group {
    pub name: String,
    pub sub: String,
    pub tracks: Vec<usize>,
    pub seed: u64,
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub name: String,
    pub path: Option<PathBuf>,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
    pub count: usize,
}

enum Bg {
    ScanProgress(usize),
    ScanDone(Result<ScanStats, String>),
    Feed(usize, Result<Vec<Episode>, String>),
    Downloaded(u64, String, Result<PathBuf, String>),
    /// Bytes recibidos de una descarga del gestor unificado: (id, recibidos, total).
    DlBytes(u64, u64, Option<u64>),
    StationAdded(Station, Option<String>),
    PodcastAdded(PodcastFeed, Vec<Episode>),
    AddFailed(String),
    Cover(String, Option<Thumb>),
    RadioSearch(Result<Vec<StationResult>, String>),
    /// Lista del directorio de podcasts (generación de la petición, sección, resultado).
    Shows(u64, PodcastSection, Result<Vec<ShowResult>, String>),
}

pub const CROSSFADE_STEPS: [u32; 4] = [0, 3, 6, 9];
pub const SETTINGS_SECTIONS: [&str; 7] = [
    "General",
    "Audio",
    "Biblioteca",
    "Daemon / IPC",
    "Avanzado",
    "Acerca de",
    "Temas",
];
pub const LIST_MENU: [&str; 5] = [
    "Crear lista",
    "Editar nombre",
    "Eliminar",
    "Importar lista",
    "Exportar lista",
];
pub const ALBUM_TABS: [&str; 3] = ["Álbumes", "Artistas", "Géneros"];

pub fn hash_str(text: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub struct App {
    pub view: View,
    pub focus: usize,
    pub snapshot: Snapshot,
    pub theme: Theme,
    pub theme_source: String,
    pub settings: Settings,
    pub settings_path: Option<PathBuf>,
    pub message: String,
    pub help: bool,
    pub manual: bool,
    pub manual_page: usize,
    /// Subventana de descargas abierta y primera entrada visible.
    pub dl_window: bool,
    pub dl_scroll: usize,
    /// Tokens para detener de forma cooperativa cada trabajo del gestor unificado.
    dl_cancellations: HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// IDs de los trabajos que pertenecen a la descarga de canal completo.
    bulk_job_ids: HashSet<u64>,
    /// Diálogo «¿en paralelo o en cola?» pendiente de respuesta (descarga de varios episodios).
    pub dl_choice: Option<DlChoice>,
    pub keys_help: String,
    pub quit: bool,
    /// `true` si se pidió salir y detener también el daemon (tecla `W`).
    pub quit_all: bool,
    pub remote: bool,
    pub tick: u64,
    message_until: u64,

    library: Library,
    db_path: Option<PathBuf>,
    pub tracks: Vec<Track>,
    haystack: Vec<String>,
    track_by_path: HashMap<PathBuf, usize>,
    track_by_id: HashMap<i64, usize>,
    pub favorites: HashSet<i64>,
    /// Pistas marcadas (selección múltiple entre vistas) para añadir a una lista.
    pub marked: HashSet<i64>,
    pub albums: Vec<Group>,
    pub artists: Vec<Group>,
    pub genres: Vec<Group>,

    pub folders: Vec<Folder>,
    pub folder_cur: Cursor,
    dir_kids: HashMap<PathBuf, Vec<PathBuf>>,
    dir_count: HashMap<PathBuf, usize>,
    dir_roots: Vec<PathBuf>,
    expanded: HashSet<PathBuf>,
    pub lib_rows: Vec<usize>,
    pub lib_cur: Cursor,

    pub input: Option<Input>,
    pub directory_picker: Option<DirectoryPicker>,
    pub search_text: String,
    pub search_rows: Vec<SearchRow>,
    pub search_total: [usize; 3],
    pub search_cur: Cursor,

    pub queue_cur: Cursor,

    pub album_tab: usize,
    pub grid_sel: usize,
    pub grid_cols: usize,

    pub playlists: Vec<(i64, String)>,
    pub list_cur: Cursor,
    pub menu_cur: Cursor,
    pub list_tracks: Vec<usize>,
    pending_delete: Option<i64>,
    pending_url: String,
    pending_source: Option<String>,
    pending_radio_export_name: Option<String>,

    pub station_cur: Cursor,
    pub radio_browser_mode: bool,
    pub radio_filter: RadioFilter,
    pub radio_results: Vec<StationResult>,
    pub radio_result_cur: Cursor,
    pub radio_searching: bool,
    pub podcast_cur: Cursor,
    pub podcast_browser: bool,
    /// Resumen del lote de episodios de canal completo (los trabajos están en `dl`).
    pub bulk: Option<BulkDownload>,
    /// Gestor único para descargas individuales, múltiples y de canal completo.
    pub dl: DownloadManager,
    /// Aviso flotante (esquina superior derecha).
    pub toast: Option<Toast>,
    bulk_confirm: Option<(usize, std::time::Instant)>,
    /// Episodios marcados (por URL) para descargarlos juntos.
    pub episode_marks: HashSet<String>,
    pub podcast_section: PodcastSection,
    /// Podcasts de la lista visible del explorador (populares, resultados o una categoría).
    pub shows: Vec<ShowResult>,
    pub show_cur: Cursor,
    pub category_cur: Cursor,
    pub category_open: bool,
    pub shows_loading: bool,
    pub popular_cache: Option<Vec<ShowResult>>,
    shows_generation: u64,
    pub episode_cur: Cursor,
    pub episodes: HashMap<usize, Vec<Episode>>,
    pub feed_loading: Option<usize>,

    pub lyrics: Option<Lyrics>,
    lyrics_media: Option<MediaSource>,
    pub lyrics_offset_ms: i64,

    pub eq: [f32; EQ_BANDS],
    eq_edit_tick: u64,
    pub eq_sel: usize,
    pub eq_preset: usize,
    pub settings_cur: Cursor,
    pub theme_cur: Cursor,

    pub covers: HashMap<String, CoverState>,
    /// Última carátula mostrada de la pista actual: se mantiene mientras carga la nueva.
    last_now_cover: Option<std::sync::Arc<Thumb>>,
    /// `cover` dejó una carátula cargándose en la última llamada.
    cover_pending: bool,
    pub scanning: bool,
    pub scan_progress: usize,
    bg_tx: mpsc::Sender<Bg>,
    bg_rx: mpsc::Receiver<Bg>,
}

impl App {
    pub fn new(
        settings: Settings,
        library: Library,
        db_path: Option<PathBuf>,
        theme_source: String,
        theme: Theme,
        remote: bool,
    ) -> App {
        let (bg_tx, bg_rx) = mpsc::channel();
        let mut snapshot = Snapshot::new(Vec::new());
        snapshot.eq = settings.eq_array();
        let mut app = App {
            view: View::Inicio,
            focus: 0,
            eq: settings.eq_array(),
            snapshot,
            theme,
            theme_source,
            settings,
            settings_path: None,
            message: String::new(),
            help: false,
            manual: false,
            manual_page: 0,
            dl_window: false,
            dl_scroll: 0,
            dl_cancellations: HashMap::new(),
            bulk_job_ids: HashSet::new(),
            dl_choice: None,
            keys_help: String::new(),
            quit: false,
            quit_all: false,
            remote,
            tick: 0,
            message_until: 0,
            dl: DownloadManager::new(),
            toast: None,
            library,
            db_path,
            tracks: Vec::new(),
            haystack: Vec::new(),
            track_by_path: HashMap::new(),
            track_by_id: HashMap::new(),
            favorites: HashSet::new(),
            marked: HashSet::new(),
            albums: Vec::new(),
            artists: Vec::new(),
            genres: Vec::new(),
            folders: Vec::new(),
            folder_cur: Cursor::default(),
            dir_kids: HashMap::new(),
            dir_count: HashMap::new(),
            dir_roots: Vec::new(),
            expanded: HashSet::new(),
            lib_rows: Vec::new(),
            lib_cur: Cursor::default(),
            input: None,
            directory_picker: None,
            search_text: String::new(),
            search_rows: Vec::new(),
            search_total: [0; 3],
            search_cur: Cursor::default(),
            queue_cur: Cursor::default(),
            album_tab: 0,
            grid_sel: 0,
            grid_cols: 4,
            playlists: Vec::new(),
            list_cur: Cursor::default(),
            menu_cur: Cursor::default(),
            list_tracks: Vec::new(),
            pending_delete: None,
            pending_url: String::new(),
            pending_source: None,
            pending_radio_export_name: None,
            station_cur: Cursor::default(),
            radio_browser_mode: false,
            radio_filter: RadioFilter::All,
            radio_results: Vec::new(),
            radio_result_cur: Cursor::default(),
            radio_searching: false,
            podcast_cur: Cursor::default(),
            podcast_browser: false,
            bulk: None,
            bulk_confirm: None,
            episode_marks: HashSet::new(),
            podcast_section: PodcastSection::Popular,
            shows: Vec::new(),
            show_cur: Cursor::default(),
            category_cur: Cursor::default(),
            category_open: false,
            shows_loading: false,
            popular_cache: None,
            shows_generation: 0,
            episode_cur: Cursor::default(),
            episodes: HashMap::new(),
            feed_loading: None,
            lyrics: None,
            lyrics_media: None,
            lyrics_offset_ms: 0,
            eq_edit_tick: 0,
            eq_sel: 0,
            eq_preset: 0,
            settings_cur: Cursor::default(),
            theme_cur: Cursor::default(),
            covers: HashMap::new(),
            last_now_cover: None,
            cover_pending: false,
            scanning: false,
            scan_progress: 0,
            bg_tx,
            bg_rx,
        };
        app.dl.set_parallel_limit(app.settings.parallel_downloads);
        app.reload_library();
        app
    }

    /// Muestra un aviso flotante durante unos 6 segundos.
    pub fn toast(&mut self, text: impl Into<String>, ok: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            ok,
            until: self.tick + 90,
        });
    }

    /// ¿Hay algo descargándose ahora mismo?
    /// Entradas que muestra la subventana: resumen del canal (si hay) y trabajos del gestor.
    pub fn download_entry_count(&self) -> usize {
        self.dl.len() + usize::from(self.bulk.is_some())
    }

    /// Total de trabajos de la tanda actual, incluidos los que ya finalizaron.
    pub fn download_total_count(&self) -> usize {
        self.dl.total_count()
    }

    /// Progreso del canal completo, contando episodios finalizados y todos los que siguen en curso/cola.
    pub fn bulk_download_fraction(&self) -> Option<f64> {
        let bulk = self.bulk.as_ref()?;
        if bulk.total == 0 {
            return None;
        }
        let running: f64 = self
            .dl
            .jobs()
            .iter()
            .filter(|job| self.bulk_job_ids.contains(&job.id))
            .map(|job| job.fraction().unwrap_or(0.0))
            .sum();
        Some(((bulk.done as f64 + running) / bulk.total as f64).clamp(0.0, 1.0))
    }

    /// Progreso global de toda la tanda, incluidos los trabajos ya completados.
    pub fn download_overall_fraction(&self) -> Option<f64> {
        self.dl.overall_fraction()
    }

    pub fn downloading(&self) -> bool {
        self.bulk.is_some() || !self.dl.is_empty()
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.message = text.into();
        self.message_until = self.tick + 45;
    }

    /// Alinea el cursor de la lista de temas con el tema actualmente activo.
    fn sync_theme_cursor(&mut self) {
        let current = self.theme_source.trim().to_ascii_lowercase();
        self.theme_cur.index = crate::theme::THEMES
            .iter()
            .position(|(name, _)| *name == current)
            .unwrap_or(0);
        self.theme_cur.sync(crate::theme::THEMES.len());
    }

    /// Aplica directamente el tema seleccionado en Ajustes → Temas.
    fn select_theme(&mut self, index: usize) {
        let Some((name, _)) = crate::theme::THEMES.get(index) else {
            self.notify("Selección de tema no válida");
            return;
        };
        let name = *name;
        match Theme::resolve(name) {
            Ok(theme) => {
                self.theme = theme;
                self.theme_source = name.to_string();
                self.settings.theme = name.to_string();
                self.theme_cur.index = index;
                self.theme_cur.sync(crate::theme::THEMES.len());
                self.notify(format!("Tema aplicado: {name}"));
                self.save_settings();
            }
            Err(error) => self.notify(format!("No se pudo aplicar el tema: {error}")),
        }
    }

    // ---------- biblioteca ----------

    pub fn reload_library(&mut self) {
        match self.library.all_tracks() {
            Ok(tracks) => self.tracks = tracks,
            Err(error) => {
                self.notify(format!("Biblioteca: {error}"));
                return;
            }
        }
        self.favorites = self.library.favorites().unwrap_or_default();
        self.playlists = self.library.playlists().unwrap_or_default();
        self.haystack = self
            .tracks
            .iter()
            .map(|t| format!("{} {} {}", t.title, t.artist, t.album).to_lowercase())
            .collect();
        self.track_by_path = self
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.path.clone(), i))
            .collect();
        self.track_by_id = self
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id, i))
            .collect();
        self.build_groups();
        self.build_folders();
        self.refresh_lib_rows();
        self.refresh_list_tracks();
        if !self.search_text.is_empty() {
            self.run_search();
        }
        let groups = self.grid_len();
        self.grid_sel = self.grid_sel.min(groups.saturating_sub(1));
    }

    fn build_groups(&mut self) {
        let mut albums: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut artists: HashMap<String, Vec<usize>> = HashMap::new();
        let mut genres: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, t) in self.tracks.iter().enumerate() {
            albums
                .entry((t.album.clone(), t.artist.clone()))
                .or_default()
                .push(i);
            artists.entry(t.artist.clone()).or_default().push(i);
            let genre = if t.genre.is_empty() {
                "Sin género".to_string()
            } else {
                t.genre.clone()
            };
            genres.entry(genre).or_default().push(i);
        }
        let mut album_groups: Vec<Group> = albums
            .into_iter()
            .map(|((album, artist), tracks)| Group {
                seed: hash_str(&format!("{album}{artist}")),
                sub: format!("{artist} · {} pistas", tracks.len()),
                name: album,
                tracks,
            })
            .collect();
        album_groups.sort_by_key(|g| (g.name.to_lowercase(), g.sub.to_lowercase()));
        let mut artist_groups: Vec<Group> = artists
            .into_iter()
            .map(|(artist, tracks)| {
                let distinct: HashSet<&str> = tracks
                    .iter()
                    .map(|i| self.tracks[*i].album.as_str())
                    .collect();
                Group {
                    seed: hash_str(&artist),
                    sub: format!("{} álbumes", distinct.len()),
                    name: artist,
                    tracks,
                }
            })
            .collect();
        artist_groups.sort_by_key(|g| g.name.to_lowercase());
        let mut genre_groups: Vec<Group> = genres
            .into_iter()
            .map(|(genre, tracks)| Group {
                seed: hash_str(&genre),
                sub: format!("{} canciones", tracks.len()),
                name: genre,
                tracks,
            })
            .collect();
        genre_groups.sort_by_key(|g| g.name.to_lowercase());
        self.albums = album_groups;
        self.artists = artist_groups;
        self.genres = genre_groups;
    }

    /// Construye el árbol de carpetas a partir de las rutas de la biblioteca
    /// (solo carpetas con música, a cualquier profundidad).
    fn build_folders(&mut self) {
        let roots: Vec<PathBuf> = self.settings.music_dirs.clone();
        let mut kids: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
        let mut count: HashMap<PathBuf, usize> = HashMap::new();
        for t in &self.tracks {
            let Some(root) = roots
                .iter()
                .filter(|r| t.path.starts_with(r))
                .max_by_key(|r| r.as_os_str().len())
            else {
                continue;
            };
            let mut dir = t.path.parent();
            while let Some(d) = dir {
                *count.entry(d.to_path_buf()).or_insert(0) += 1;
                if d == root.as_path() {
                    break;
                }
                let Some(parent) = d.parent() else { break };
                kids.entry(parent.to_path_buf())
                    .or_default()
                    .insert(d.to_path_buf());
                dir = Some(parent);
            }
        }
        self.dir_kids = kids
            .into_iter()
            .map(|(k, v)| {
                let mut v: Vec<PathBuf> = v.into_iter().collect();
                v.sort_by_key(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default()
                });
                (k, v)
            })
            .collect();
        self.dir_count = count;
        self.dir_roots = roots
            .into_iter()
            .filter(|r| self.dir_count.contains_key(r))
            .collect();
        if self.expanded.is_empty() {
            self.expanded.extend(self.dir_roots.iter().cloned());
        }
        self.rebuild_visible_folders();
    }

    /// Aplana el árbol según las carpetas desplegadas, conservando la selección.
    fn rebuild_visible_folders(&mut self) {
        let keep = self
            .folders
            .get(self.folder_cur.index)
            .and_then(|f| f.path.clone());
        let mut out = vec![Folder {
            name: "Todo".into(),
            path: None,
            depth: 0,
            has_children: false,
            expanded: false,
            count: self.tracks.len(),
        }];
        let roots = self.dir_roots.clone();
        for root in &roots {
            self.push_folder(&mut out, root, 0);
        }
        self.folders = out;
        let idx = keep
            .and_then(|k| {
                self.folders
                    .iter()
                    .position(|f| f.path.as_ref() == Some(&k))
            })
            .unwrap_or(self.folder_cur.index);
        self.folder_cur.index = idx;
        let n = self.folders.len();
        self.folder_cur.clamp(n);
    }

    fn push_folder(&self, out: &mut Vec<Folder>, path: &Path, depth: usize) {
        let kids = self.dir_kids.get(path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Música".into());
        let expanded = self.expanded.contains(path);
        out.push(Folder {
            name,
            path: Some(path.to_path_buf()),
            depth,
            has_children: kids.map(|k| !k.is_empty()).unwrap_or(false),
            expanded,
            count: self.dir_count.get(path).copied().unwrap_or(0),
        });
        if expanded {
            if let Some(kids) = kids {
                for k in kids {
                    self.push_folder(out, k, depth + 1);
                }
            }
        }
    }

    /// Despliega (`open = true`) o pliega la carpeta seleccionada; al plegar una
    /// carpeta ya plegada, sube a su carpeta padre.
    pub fn folder_expand(&mut self, open: bool) {
        let Some(f) = self.folders.get(self.folder_cur.index).cloned() else {
            return;
        };
        let Some(path) = f.path else { return };
        if open {
            if f.has_children {
                if !f.expanded {
                    self.expanded.insert(path);
                } else if let Some(first) =
                    self.dir_kids.get(&path).and_then(|k| k.first()).cloned()
                {
                    self.rebuild_visible_folders();
                    if let Some(i) = self
                        .folders
                        .iter()
                        .position(|x| x.path.as_ref() == Some(&first))
                    {
                        self.folder_cur.index = i;
                    }
                }
            }
        } else if f.expanded && f.has_children {
            self.expanded.remove(&path);
        } else if let Some(parent) = path.parent() {
            if let Some(i) = self
                .folders
                .iter()
                .position(|x| x.path.as_deref() == Some(parent))
            {
                self.folder_cur.index = i;
            }
        }
        self.rebuild_visible_folders();
        self.refresh_lib_rows();
        self.lib_cur.index = 0;
    }

    /// Todas las pistas de la carpeta seleccionada (subcarpetas incluidas).
    fn folder_tracks(&self) -> Vec<usize> {
        let folder = self
            .folders
            .get(self.folder_cur.index)
            .and_then(|f| f.path.clone());
        self.tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                folder
                    .as_ref()
                    .map(|p| t.path.starts_with(p))
                    .unwrap_or(true)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn refresh_lib_rows(&mut self) {
        let folder = self
            .folders
            .get(self.folder_cur.index)
            .and_then(|f| f.path.clone());
        self.lib_rows = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                folder
                    .as_ref()
                    .map(|p| t.path.starts_with(p))
                    .unwrap_or(true)
            })
            .map(|(i, _)| i)
            .collect();
        let n = self.lib_rows.len();
        self.lib_cur.clamp(n);
    }

    pub fn selected_list_id(&self) -> Option<i64> {
        if self.list_cur.index == 0 {
            None
        } else {
            self.playlists.get(self.list_cur.index - 1).map(|p| p.0)
        }
    }

    pub fn list_name(&self) -> String {
        if self.list_cur.index == 0 {
            "Favoritas".into()
        } else {
            self.playlists
                .get(self.list_cur.index - 1)
                .map(|p| p.1.clone())
                .unwrap_or_default()
        }
    }

    pub fn list_len(&self) -> usize {
        self.playlists.len() + 1
    }

    pub fn list_count(&self, index: usize) -> usize {
        if index == 0 {
            return self.favorites.len();
        }
        self.playlists
            .get(index - 1)
            .and_then(|p| self.library.playlist_track_ids(p.0).ok())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    pub fn refresh_list_tracks(&mut self) {
        self.list_tracks = match self.selected_list_id() {
            None => self
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| self.favorites.contains(&t.id))
                .map(|(i, _)| i)
                .collect(),
            Some(id) => self
                .library
                .playlist_track_ids(id)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|tid| self.track_by_id.get(&tid).copied())
                .collect(),
        };
    }

    pub fn grid(&self) -> &Vec<Group> {
        match self.album_tab {
            0 => &self.albums,
            1 => &self.artists,
            _ => &self.genres,
        }
    }
    pub fn grid_len(&self) -> usize {
        self.grid().len()
    }

    pub fn group_is_favorite(&self, group: &Group) -> bool {
        !group.tracks.is_empty()
            && group
                .tracks
                .iter()
                .all(|i| self.favorites.contains(&self.tracks[*i].id))
    }

    pub fn is_favorite(&self, track: usize) -> bool {
        self.tracks
            .get(track)
            .map(|t| self.favorites.contains(&t.id))
            .unwrap_or(false)
    }

    pub fn current_track_index(&self) -> Option<usize> {
        match &self.snapshot.current_item()?.media {
            MediaSource::File(path) => self.track_by_path.get(path).copied(),
            MediaSource::Url(_) => None,
        }
    }

    // ---------- búsqueda ----------

    pub fn run_search(&mut self) {
        let query = self.search_text.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        self.search_rows.clear();
        self.search_total = [0; 3];
        if words.is_empty() {
            self.search_cur.index = 0;
            return;
        }
        let hit = |text: &str| words.iter().all(|w| text.contains(w));
        let tracks: Vec<usize> = (0..self.tracks.len())
            .filter(|i| hit(&self.haystack[*i]))
            .collect();
        let albums: Vec<usize> = (0..self.albums.len())
            .filter(|i| {
                hit(&format!("{} {}", self.albums[*i].name, self.albums[*i].sub).to_lowercase())
            })
            .collect();
        let artists: Vec<usize> = (0..self.artists.len())
            .filter(|i| hit(&self.artists[*i].name.to_lowercase()))
            .collect();
        self.search_total = [tracks.len(), albums.len(), artists.len()];
        if !tracks.is_empty() {
            self.search_rows
                .push(SearchRow::Header(format!("Canciones ({})", tracks.len())));
            self.search_rows
                .extend(tracks.into_iter().take(200).map(SearchRow::Track));
        }
        if !albums.is_empty() {
            self.search_rows
                .push(SearchRow::Header(format!("Álbumes ({})", albums.len())));
            self.search_rows
                .extend(albums.into_iter().take(50).map(SearchRow::Album));
        }
        if !artists.is_empty() {
            self.search_rows
                .push(SearchRow::Header(format!("Artistas ({})", artists.len())));
            self.search_rows
                .extend(artists.into_iter().take(50).map(SearchRow::Artist));
        }
        self.search_cur.index = self
            .search_rows
            .iter()
            .position(|r| !matches!(r, SearchRow::Header(_)))
            .unwrap_or(0);
    }

    fn step_search(&mut self, delta: i32) {
        let n = self.search_rows.len();
        if n == 0 {
            return;
        }
        let mut i = self.search_cur.index as i64;
        for _ in 0..n {
            i = (i + delta as i64).rem_euclid(n as i64);
            if !matches!(self.search_rows[i as usize], SearchRow::Header(_)) {
                break;
            }
        }
        self.search_cur.index = i as usize;
    }

    // ---------- actualización periódica ----------

    pub fn update(&mut self, snapshot: Snapshot) {
        self.tick += 1;
        if self.tick >= self.message_until && !self.message.is_empty() {
            self.message.clear();
        }
        if matches!(&self.toast, Some(t) if self.tick >= t.until) {
            self.toast = None;
        }
        self.snapshot = snapshot;
        if self.tick > self.eq_edit_tick + 6 {
            self.eq = self.snapshot.eq;
        }
        let queue_len = self.snapshot.queue.len();
        self.queue_cur.clamp(queue_len);
        self.sync_lyrics();
        while let Ok(event) = self.bg_rx.try_recv() {
            self.on_background(event);
        }
    }

    fn sync_lyrics(&mut self) {
        let media = self.snapshot.current_item().map(|i| i.media.clone());
        if media != self.lyrics_media {
            self.lyrics = match &media {
                Some(MediaSource::File(path)) => Lyrics::load_for(path),
                _ => None,
            };
            self.lyrics_offset_ms = 0;
            self.lyrics_media = media;
        }
    }

    /// Miniatura lista de una carátula; la primera vez lanza su carga en segundo plano.
    pub fn cover(
        &mut self,
        key: &str,
        source: impl FnOnce() -> CoverSource,
    ) -> Option<std::sync::Arc<Thumb>> {
        match self.covers.get(key) {
            Some(CoverState::Ready(t)) => return Some(t.clone()),
            Some(CoverState::Loading) => {
                self.cover_pending = true;
                return None;
            }
            Some(_) => return None,
            None => {}
        }
        if self.covers.len() >= cover::CACHE_MAX {
            self.covers.clear();
        }
        self.cover_pending = true;
        self.covers.insert(key.to_string(), CoverState::Loading);
        let (src, tx, key) = (source(), self.bg_tx.clone(), key.to_string());
        thread::spawn(move || {
            let _ = tx.send(Bg::Cover(key, cover::load(&src)));
        });
        None
    }

    /// Clave y origen de la carátula de una pista (archivo, o podcast por nombre de feed).
    fn item_cover(&self, item: &QueueItem) -> Option<(String, CoverSource)> {
        if item.live {
            return None;
        }
        let feed = self
            .settings
            .podcasts
            .iter()
            .find(|p| !item.artist.is_empty() && p.name == item.artist)
            .map(|p| p.url.clone());
        let in_podcasts = item
            .file_path()
            .map(|p| p.starts_with(config::data_dir().join("podcasts")))
            .unwrap_or(true);
        if let (Some(url), true) = (feed, in_podcasts) {
            return Some((format!("p:{url}"), CoverSource::Feed(url)));
        }
        let path = item.file_path()?.to_path_buf();
        Some((
            format!("f:{}", path.display()),
            CoverSource::Files(vec![path]),
        ))
    }

    /// Carátula de la pista en reproducción. Al cambiar de pista conserva la anterior hasta
    /// que la nueva esté lista (sin hueco no hay parpadeo) y precarga la de la siguiente.
    pub fn now_cover(&mut self) -> Option<std::sync::Arc<Thumb>> {
        let current = self.snapshot.current_item().cloned();
        let next = self
            .snapshot
            .current
            .and_then(|i| self.snapshot.queue.get(i + 1))
            .cloned();
        if let Some((key, src)) = next.as_ref().and_then(|n| self.item_cover(n)) {
            let _ = self.cover(&key, || src);
        }
        self.cover_pending = false;
        let found = current
            .as_ref()
            .and_then(|item| self.item_cover(item))
            .and_then(|(key, src)| self.cover(&key, || src));
        match found {
            Some(t) => {
                self.last_now_cover = Some(t.clone());
                Some(t)
            }
            None if self.cover_pending => self.last_now_cover.clone(),
            None => {
                self.last_now_cover = None;
                None
            }
        }
    }

    /// Carátula de un álbum, artista o género de la cuadrícula.
    pub fn group_cover(&mut self, group: &Group) -> Option<std::sync::Arc<Thumb>> {
        let files: Vec<PathBuf> = group
            .tracks
            .iter()
            .take(4)
            .map(|i| self.tracks[*i].path.clone())
            .collect();
        self.cover(&format!("g{}:{}", self.album_tab, group.seed), || {
            CoverSource::Files(files)
        })
    }

    /// Carátula del podcast seleccionado.
    pub fn podcast_cover(&mut self, index: usize) -> Option<std::sync::Arc<Thumb>> {
        let url = self.settings.podcasts.get(index)?.url.clone();
        self.cover(&format!("p:{url}"), || CoverSource::Feed(url.clone()))
    }

    pub fn lyric_index(&self) -> Option<usize> {
        let ms = self.snapshot.position.as_millis() as i64 - self.lyrics_offset_ms;
        self.lyrics.as_ref().and_then(|l| l.current(ms))
    }

    fn on_background(&mut self, event: Bg) {
        match event {
            Bg::ScanProgress(n) => self.scan_progress = n,
            Bg::ScanDone(result) => {
                self.scanning = false;
                match result {
                    Ok(s) => {
                        self.reload_library();
                        self.notify(format!("Biblioteca actualizada: {} nuevas o cambiadas, {} sin cambios, {} eliminadas",
                            s.added, s.unchanged, s.removed));
                    }
                    Err(e) => self.notify(format!("Error al escanear: {e}")),
                }
            }
            Bg::Feed(index, result) => {
                if self.feed_loading == Some(index) {
                    self.feed_loading = None;
                }
                match result {
                    Ok(eps) => {
                        self.episodes.insert(index, eps);
                    }
                    Err(e) => self.notify(e),
                }
            }
            Bg::StationAdded(station, warn) => {
                self.sync_sources_from_file();
                let name = station.name.clone();
                if self.settings.stations.iter().any(|s| s.url == station.url) {
                    self.notify("Esa emisora ya estaba en la lista");
                    return;
                }
                self.settings.stations.push(station);
                self.radio_filter = RadioFilter::All;
                self.station_cur.index = self.settings.stations.len() - 1;
                self.save_settings();
                self.notify(match warn {
                    Some(w) => format!("«{name}»: {w}"),
                    None => format!("Emisora «{name}» añadida"),
                });
            }
            Bg::PodcastAdded(feed, eps) => {
                self.sync_sources_from_file();
                let name = feed.name.clone();
                if self.settings.podcasts.iter().any(|p| p.url == feed.url) {
                    self.notify("Ese podcast ya estaba en la lista");
                    return;
                }
                self.settings.podcasts.push(feed);
                let i = self.settings.podcasts.len() - 1;
                self.episodes.insert(i, eps);
                self.podcast_cur.index = i;
                self.episode_cur.index = 0;
                if self.podcast_browser {
                    self.podcast_browser = false;
                    self.focus = 1;
                }
                self.save_settings();
                self.notify(format!("Podcast «{name}» añadido"));
            }
            Bg::Shows(generation, section, result) => {
                if generation != self.shows_generation {
                    return;
                }
                self.shows_loading = false;
                match result {
                    Ok(list) => {
                        if section == PodcastSection::Popular {
                            self.popular_cache = Some(list.clone());
                        }
                        if list.is_empty() {
                            self.notify("Sin resultados");
                        }
                        self.shows = list;
                        self.show_cur.index = 0;
                    }
                    Err(e) => self.notify(e),
                }
            }
            Bg::AddFailed(e) => self.notify(e),
            Bg::RadioSearch(result) => {
                self.radio_searching = false;
                match result {
                    Ok(rows) => {
                        self.radio_results = rows;
                        self.radio_result_cur.index = 0;
                        self.notify(format!(
                            "Radio Browser: {} emisoras encontradas",
                            self.radio_results.len()
                        ));
                    }
                    Err(e) => self.notify(e),
                }
            }
            Bg::Cover(key, thumb) => {
                let state = match thumb {
                    Some(t) => CoverState::Ready(std::sync::Arc::new(t)),
                    None => CoverState::Missing,
                };
                self.covers.insert(key, state);
            }
            Bg::DlBytes(id, received, size) => {
                self.dl.progress(id, received, size);
                if self.bulk_job_ids.contains(&id) {
                    if let Some(bulk) = self.bulk.as_mut() {
                        bulk.current = self
                            .dl
                            .jobs()
                            .iter()
                            .find(|job| job.id == id)
                            .map(|job| job.title.clone())
                            .unwrap_or_default();
                        bulk.received = received;
                        bulk.size = size;
                    }
                }
            }
            Bg::Downloaded(id, title, result) => {
                self.dl_cancellations.remove(&id);
                let belongs_to_bulk = self.bulk_job_ids.remove(&id);
                let bulk_finished = if belongs_to_bulk {
                    if let Some(bulk) = self.bulk.as_mut() {
                        bulk.done += 1;
                        if matches!(&result, Err(e) if e == "Descarga cancelada") {
                            bulk.cancelled_jobs += 1;
                        } else if result.is_err() {
                            bulk.failed += 1;
                        }
                    }
                    self.bulk_job_ids.is_empty()
                } else {
                    false
                };
                self.dl.finish_with_result(
                    id,
                    result.is_ok(),
                    matches!(&result, Err(e) if e == "Descarga cancelada"),
                );
                self.pump_downloads();
                match result {
                    Ok(path) => {
                        self.notify(format!("Episodio descargado: {}", path.display()));
                        self.toast(format!("Descarga terminada · {title}"), true);
                    }
                    Err(e) if e == "Descarga cancelada" => {
                        self.notify(format!("Descarga cancelada · {title}"));
                    }
                    Err(e) => {
                        self.notify(format!("Falló «{title}»: {e}"));
                        self.toast(format!("Descarga fallida · {title}"), false);
                    }
                }
                if belongs_to_bulk && bulk_finished {
                    if let Some(bulk) = self.bulk.take() {
                        let downloaded =
                            bulk.done.saturating_sub(bulk.failed + bulk.cancelled_jobs);
                        let mut summary =
                            format!("«{}»: {} episodios descargados", bulk.name, downloaded);
                        if bulk.failed > 0 {
                            summary.push_str(&format!(
                                ", {} fallaron (repite D para reintentar)",
                                bulk.failed
                            ));
                        }
                        if bulk.cancelled {
                            summary.push_str(" · descarga cancelada");
                        }
                        self.notify(summary);
                        self.toast(
                            if bulk.cancelled {
                                format!("Descarga cancelada · «{}»", bulk.name)
                            } else if bulk.failed > 0 {
                                format!(
                                    "Descarga de «{}» terminada con {} fallos",
                                    bulk.name, bulk.failed
                                )
                            } else {
                                format!(
                                    "Descarga terminada · «{}» ({} episodios)",
                                    bulk.name, downloaded
                                )
                            },
                            !bulk.cancelled && bulk.failed == 0,
                        );
                    }
                }
            }
        }
    }

    fn start_scan(&mut self) {
        let Some(db_path) = self.db_path.clone() else {
            self.notify("Escaneo no disponible con una base de datos temporal");
            return;
        };
        if self.scanning {
            self.notify("Ya hay un escaneo en curso");
            return;
        }
        self.scanning = true;
        self.scan_progress = 0;
        let roots = self.settings.music_dirs.clone();
        let tx = self.bg_tx.clone();
        thread::spawn(move || {
            let result = Library::open(&db_path).and_then(|lib| {
                let progress_tx = tx.clone();
                library::scan(&lib, &roots, move |n| {
                    let _ = progress_tx.send(Bg::ScanProgress(n));
                })
            });
            let _ = tx.send(Bg::ScanDone(result));
        });
        self.notify("Escaneando biblioteca…");
    }

    fn fetch_feed(&mut self, index: usize) {
        let Some(feed) = self.settings.podcasts.get(index).cloned() else {
            return;
        };
        if self.feed_loading.is_some() {
            return;
        }
        self.feed_loading = Some(index);
        let tx = self.bg_tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Bg::Feed(index, feeds::fetch_feed(&feed.url)));
        });
    }

    // ---------- vistas ----------

    pub fn set_view(&mut self, view: View) {
        self.view = view;
        self.focus = 0;
        self.pending_source = None;
        if matches!(view, View::Radio | View::Podcasts) {
            self.sync_sources_from_file();
        }
        match view {
            View::Busqueda => {
                if self.input.is_none() {
                    self.input = Some(Input {
                        mode: InputMode::Search,
                        text: self.search_text.clone(),
                    });
                }
            }
            View::Podcasts => {
                let i = self.podcast_cur.index;
                if !self.episodes.contains_key(&i) {
                    self.fetch_feed(i);
                }
            }
            View::Listas => self.refresh_list_tracks(),
            _ => {}
        }
    }

    /// Vuelve a la vista y al panel guardados al cerrar con el daemon activo.
    pub fn restore_view(&mut self) {
        let view = View::ALL[self.settings.last_view.min(11)];
        let focus = self.settings.last_focus;
        self.set_view(view);
        // Búsqueda abre su entrada de texto; el resto recupera el panel exacto.
        if view != View::Busqueda {
            self.focus = focus.min(1);
        }
    }

    pub fn handle_manual_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        match key.code {
            Esc | Char('?') => self.manual = false,
            Right | Down | PageDown | Char(' ') => self.manual_page = (self.manual_page + 1).min(3),
            Left | Up | PageUp => self.manual_page = self.manual_page.saturating_sub(1),
            _ => {}
        }
    }

    pub fn input_active(&self) -> bool {
        self.input.is_some() || self.directory_picker.is_some()
    }

    fn refresh_picker(&mut self) {
        let Some(picker) = self.directory_picker.as_mut() else {
            return;
        };
        let mut entries = Vec::new();
        if let Ok(read) = fs::read_dir(&picker.current) {
            for item in read.flatten() {
                let path = item.path();
                let is_dir = path.is_dir();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let include = match picker.purpose {
                    PickerPurpose::ImportPlaylist => {
                        is_dir
                            || matches!(
                                ext.as_str(),
                                "m3u" | "m3u8" | "xspf" | "toml" | "html" | "htm"
                            )
                    }
                    PickerPurpose::ImportRadioPlaylist => {
                        is_dir || matches!(ext.as_str(), "m3u" | "m3u8" | "pls" | "xspf" | "json")
                    }
                    PickerPurpose::MusicFolder => is_dir,
                };
                if include {
                    entries.push(path);
                }
            }
        }
        entries.sort_by_key(|p| {
            (
                !p.is_dir(),
                p.file_name()
                    .map(|x| x.to_string_lossy().to_lowercase())
                    .unwrap_or_default(),
            )
        });
        picker.entries = entries;
        picker.selected = picker.selected.min(picker.entries.len().saturating_sub(1));
    }

    fn open_directory_picker(&mut self) {
        let start = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.directory_picker = Some(DirectoryPicker {
            current: start,
            entries: Vec::new(),
            selected: 0,
            purpose: PickerPurpose::ImportPlaylist,
        });
        self.refresh_picker();
    }

    fn open_radio_playlist_picker(&mut self) {
        let start = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.directory_picker = Some(DirectoryPicker {
            current: start,
            entries: Vec::new(),
            selected: 0,
            purpose: PickerPurpose::ImportRadioPlaylist,
        });
        self.refresh_picker();
    }

    fn import_radio_playlist(&mut self, path: &Path) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                self.notify(format!("No se pudo leer {}: {e}", path.display()));
                return;
            }
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let imported = match crate::radio_playlist_io::import(&text, ext) {
            Ok(rows) => rows,
            Err(e) => {
                self.notify(e);
                return;
            }
        };
        let mut added = 0usize;
        let mut skipped = 0usize;
        for station in imported {
            if self.settings.stations.iter().any(|s| {
                s.url
                    .trim()
                    .trim_end_matches('/')
                    .eq_ignore_ascii_case(station.url.trim().trim_end_matches('/'))
            }) {
                skipped += 1;
            } else {
                self.settings.stations.push(station);
                added += 1;
            }
        }
        self.save_settings();
        self.notify(format!(
            "Radio: {added} emisoras añadidas, {skipped} duplicadas omitidas"
        ));
    }

    pub fn open_music_folder_picker(&mut self) {
        let start = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/"));
        self.directory_picker = Some(DirectoryPicker {
            current: start,
            entries: Vec::new(),
            selected: 0,
            purpose: PickerPurpose::MusicFolder,
        });
        self.refresh_picker();
    }

    pub fn handle_picker_key(&mut self, key: KeyEvent) -> Vec<Command> {
        use KeyCode::*;
        let Some(mut picker) = self.directory_picker.take() else {
            return Vec::new();
        };
        match key.code {
            Esc => return Vec::new(),
            Char('s') if picker.purpose == PickerPurpose::MusicFolder => {
                let selected = picker.current.clone();
                if !self.settings.music_dirs.contains(&selected) {
                    self.settings.music_dirs.push(selected.clone());
                    self.save_settings();
                    self.notify(format!(
                        "Carpeta añadida a la biblioteca: {}",
                        selected.display()
                    ));
                    self.start_scan();
                } else {
                    self.notify("Esa carpeta ya está en la biblioteca");
                }
                return Vec::new();
            }
            Char('x') if picker.purpose == PickerPurpose::MusicFolder => {
                // Primero se intenta quitar la carpeta resaltada en la lista; si no
                // es una raíz registrada, se prueba la carpeta actual (compatibilidad).
                let highlighted = picker
                    .entries
                    .get(picker.selected)
                    .filter(|p| p.is_dir())
                    .cloned();
                let selected = highlighted
                    .filter(|candidate| {
                        self.settings
                            .music_dirs
                            .iter()
                            .any(|root| same_directory(root, candidate))
                    })
                    .or_else(|| {
                        self.settings
                            .music_dirs
                            .iter()
                            .find(|root| same_directory(root, &picker.current))
                            .cloned()
                    });
                if let Some(selected) = selected {
                    self.settings
                        .music_dirs
                        .retain(|root| !same_directory(root, &selected));
                    self.save_settings();
                    match self.library.remove_tracks_under_root(&selected) {
                        Ok(count) => {
                            self.reload_library();
                            self.notify(format!("Directorio retirado de la biblioteca: {} · {} canciones retiradas. Los archivos siguen en el disco.", selected.display(), count));
                        }
                        Err(error) => self.notify(format!("Se quitó la carpeta de la configuración, pero no se pudo limpiar la biblioteca: {error}")),
                    }
                    self.start_scan();
                } else {
                    self.notify(
                        "Selecciona una carpeta que esté agregada a la biblioteca para quitarla",
                    );
                }
                return Vec::new();
            }
            Up | Char('k') => picker.selected = picker.selected.saturating_sub(1),
            Down | Char('j') => {
                picker.selected = (picker.selected + 1).min(picker.entries.len().saturating_sub(1))
            }
            Backspace | Left => {
                if let Some(parent) = picker.current.parent() {
                    picker.current = parent.to_path_buf();
                    picker.selected = 0;
                }
            }
            Enter | Right => {
                if let Some(path) = picker.entries.get(picker.selected).cloned() {
                    if path.is_dir() {
                        picker.current = path;
                        picker.selected = 0;
                    } else if picker.purpose == PickerPurpose::ImportPlaylist {
                        self.import_m3u(&path.display().to_string());
                        return Vec::new();
                    } else if picker.purpose == PickerPurpose::ImportRadioPlaylist {
                        self.import_radio_playlist(&path);
                        return Vec::new();
                    }
                }
            }
            _ => {}
        }
        self.directory_picker = Some(picker);
        self.refresh_picker();
        Vec::new()
    }

    fn horizontal_view(&self) -> bool {
        matches!(self.view, View::Albumes | View::Audio)
            || (self.view == View::Biblioteca && self.focus == 0)
    }

    // ---------- entrada de texto ----------

    pub fn handle_text(&mut self, key: KeyEvent) -> Vec<Command> {
        if self.directory_picker.is_some() {
            return self.handle_picker_key(key);
        }
        let Some(mut input) = self.input.take() else {
            return Vec::new();
        };
        match key.code {
            KeyCode::Esc => {
                if input.mode == InputMode::Search {
                    self.search_text = input.text;
                }
                return Vec::new();
            }
            KeyCode::Enter => return self.submit_input(input),
            KeyCode::Backspace => {
                input.text.pop();
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                    && input.text.chars().count() < 200 =>
            {
                input.text.push(c);
            }
            _ => {}
        }
        if input.mode == InputMode::Search {
            self.search_text = input.text.clone();
            self.run_search();
        }
        self.input = Some(input);
        Vec::new()
    }

    fn submit_input(&mut self, input: Input) -> Vec<Command> {
        let text = input.text.trim().to_string();
        match input.mode {
            InputMode::Search => {
                self.search_text = input.text;
                self.run_search();
                self.focus = 1;
            }
            InputMode::RadioSearch => self.search_radio_browser(&text),
            InputMode::PodcastSearch => self.search_podcasts(&text),
            InputMode::NewList => match self.library.create_playlist(&text) {
                Ok(id) => {
                    self.reload_library();
                    if let Some(pos) = self.playlists.iter().position(|p| p.0 == id) {
                        self.list_cur.index = pos + 1;
                    }
                    self.refresh_list_tracks();
                    self.notify(format!("Lista «{text}» creada"));
                }
                Err(e) => self.notify(e),
            },
            InputMode::RenameList => match self.selected_list_id() {
                Some(id) => match self.library.rename_playlist(id, &text) {
                    Ok(()) => {
                        self.reload_library();
                        self.notify(format!("Lista renombrada a «{text}»"));
                    }
                    Err(e) => self.notify(e),
                },
                None => self.notify("«Favoritas» no se puede renombrar"),
            },
            InputMode::ImportM3u => self.import_m3u(&text),
            InputMode::ExportList => self.export_list(&text),
            InputMode::ExportRadioName => {
                if text.is_empty() {
                    self.notify("Escribe un nombre para la lista de radio");
                } else {
                    self.pending_radio_export_name = Some(text);
                    self.input = Some(Input {
                        mode: InputMode::ExportRadioFormat,
                        text: String::new(),
                    });
                }
            }
            InputMode::ExportRadioFormat => {
                let name = self
                    .pending_radio_export_name
                    .take()
                    .unwrap_or_else(|| "emisoras".into());
                self.export_radio_playlist(&name, &text);
            }
            InputMode::AddStation | InputMode::AddPodcast => {
                let podcast = input.mode == InputMode::AddPodcast;
                match config::normalize_url(&text) {
                    Err(e) => self.notify(e),
                    Ok(url) => {
                        let exists = if podcast {
                            self.settings.podcasts.iter().any(|p| p.url == url)
                        } else {
                            self.settings.stations.iter().any(|s| s.url == url)
                        };
                        if exists {
                            self.notify("Esa URL ya está en la lista");
                        } else {
                            self.pending_url = url;
                            let mode = if podcast {
                                InputMode::NamePodcast
                            } else {
                                InputMode::NameStation
                            };
                            self.input = Some(Input {
                                mode,
                                text: String::new(),
                            });
                        }
                    }
                }
            }
            InputMode::NameStation | InputMode::NamePodcast => {
                let podcast = input.mode == InputMode::NamePodcast;
                self.add_source(podcast, text);
            }
        }
        Vec::new()
    }

    /// Alta de emisora o podcast: consulta la red en segundo plano. Nombre vacío = automático.
    fn add_source(&mut self, podcast: bool, custom: String) {
        let url = std::mem::take(&mut self.pending_url);
        if url.is_empty() {
            return;
        }
        let tx = self.bg_tx.clone();
        self.notify("Comprobando la URL…");
        thread::spawn(move || {
            let custom = Some(custom).filter(|c| !c.is_empty());
            let event = if podcast {
                match feeds::fetch_feed_full(&url) {
                    Ok((title, eps)) => {
                        let name = custom.or(title).unwrap_or_else(|| media::host_name(&url));
                        Bg::PodcastAdded(PodcastFeed { name, url }, eps)
                    }
                    Err(e) => Bg::AddFailed(e),
                }
            } else {
                let (found, bitrate, genre, warn) = match media::station_info(&url) {
                    Ok((n, b, g)) => (n, b, g, None),
                    Err(e) => (None, 0, None, Some(format!("{e}; se añade igualmente"))),
                };
                let name = custom.or(found).unwrap_or_else(|| media::host_name(&url));
                let mut station = Station::new(name, url, bitrate);
                station.tags = genre.unwrap_or_default();
                Bg::StationAdded(station, warn)
            };
            let _ = tx.send(event);
        });
    }

    /// Incorpora las emisoras y podcasts añadidos a mano en settings.toml mientras la app está abierta.
    fn sync_sources_from_file(&mut self) {
        let Some(path) = self.settings_path.clone() else {
            return;
        };
        if !path.exists() {
            return;
        }
        match Settings::load(&path) {
            Ok(file) => {
                let n = self.settings.merge_sources(&file);
                if n > 0 {
                    self.notify(format!("{n} elemento(s) nuevo(s) leídos de settings.toml"));
                }
            }
            Err(e) => self.notify(format!("settings.toml no válido: {e}")),
        }
    }

    fn import_m3u(&mut self, path_text: &str) {
        let path = PathBuf::from(path_text.trim());
        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.notify(format!("{}: {e}", path.display()));
                return;
            }
        };
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (meta_name, paths) = match playlist_io::import(&text, &ext, &base) {
            Ok(r) => r,
            Err(e) => {
                self.notify(e);
                return;
            }
        };
        let import_metadata = match ext.to_ascii_lowercase().as_str() {
            "m3u" | "m3u8" => playlist_io::m3u_metadata(&text),
            "toml" => playlist_io::toml_metadata(&text),
            _ => Vec::new(),
        };
        let stem = meta_name
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Importada".into());
        let mut name = stem.clone();
        let mut n = 2;
        let id = loop {
            match self.library.create_playlist(&name) {
                Ok(id) => break id,
                Err(_) if n < 50 => {
                    name = format!("{stem} ({n})");
                    n += 1;
                }
                Err(e) => {
                    self.notify(e);
                    return;
                }
            }
        };
        let (mut added, mut missing) = (0, 0);
        for (index, p) in paths.iter().enumerate() {
            let track_id = self.library.track_id_lenient(p).or_else(|| {
                let metadata = import_metadata.get(index);
                let metadata_title = metadata.map(|m| m.0.as_str()).filter(|s| !s.is_empty());
                let metadata_artist = metadata.map(|m| m.1.as_str()).unwrap_or("");
                let metadata_duration = metadata.map(|m| m.2).unwrap_or(0);
                // Conserva la referencia aunque el archivo no exista o aún no esté
                // indexado. Así la lista muestra la entrada y el error se produce
                // únicamente al intentar reproducirla, como en otros reproductores.
                let title = metadata_title
                    .map(str::to_string)
                    .or_else(|| {
                        p.file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .filter(|s| !s.is_empty())
                    })
                    .unwrap_or_else(|| p.to_string_lossy().into_owned());
                self.library
                    .insert_simple(
                        &p.to_string_lossy(),
                        &title,
                        metadata_artist,
                        "",
                        "",
                        metadata_duration,
                    )
                    .ok()
            });
            if let Some(tid) = track_id {
                if self.library.add_to_playlist(id, tid).is_ok() {
                    added += 1;
                } else {
                    missing += 1;
                }
            } else {
                missing += 1;
            }
        }
        self.reload_library();
        if let Some(pos) = self.playlists.iter().position(|p| p.0 == id) {
            self.list_cur.index = pos + 1;
        }
        self.refresh_list_tracks();
        self.notify(format!(
            "Lista «{name}»: {added} pistas importadas, {missing} no están en la biblioteca"
        ));
    }

    /// `format` vacío = todos los formatos; si no, uno de `playlist_io::FORMATS`.
    fn export_list(&mut self, format: &str) {
        let formats: Vec<&str> = if format.is_empty() {
            playlist_io::FORMATS.to_vec()
        } else {
            match playlist_io::format_of(format) {
                Some(f) => vec![f],
                None => {
                    self.notify(format!(
                        "Formato no admitido (usa {} o deja vacío)",
                        playlist_io::FORMATS.join(", ")
                    ));
                    return;
                }
            }
        };
        let tracks: Vec<&Track> = self
            .list_tracks
            .iter()
            .filter_map(|i| self.tracks.get(*i))
            .collect();
        let list_name = self.list_name();
        let file = feeds::safe_file_name(&list_name);
        let dir = config::data_dir().join("exports");
        for f in &formats {
            if let Err(e) =
                playlist_io::export(&tracks, &list_name, f, &dir.join(format!("{file}.{f}")))
            {
                self.notify(e);
                return;
            }
        }
        self.notify(format!(
            "Exportadas {} pistas ({}) a {}",
            tracks.len(),
            formats.join(", "),
            dir.display()
        ));
    }

    fn export_radio_playlist(&mut self, list_name: &str, format: &str) {
        let stations: Vec<Station> = if self.radio_browser_mode {
            self.radio_results
                .iter()
                .map(|r| {
                    let mut station = Station::new(r.name.clone(), r.url.clone(), r.bitrate);
                    station.uuid = r.uuid.clone();
                    station.tags = r.tags.clone();
                    station
                })
                .collect()
        } else {
            self.settings.stations.clone()
        };
        if stations.is_empty() {
            self.notify("No hay emisoras para exportar");
            return;
        }
        let supported = crate::radio_playlist_io::FORMATS;
        let formats: Vec<&str> = if format.trim().is_empty() {
            supported.to_vec()
        } else {
            let requested = format.trim().trim_start_matches('.').to_ascii_lowercase();
            match supported.iter().copied().find(|f| *f == requested) {
                Some(ext) => vec![ext],
                None => {
                    self.notify(format!(
                        "Formato no admitido (usa {} o deja vacío para todos)",
                        supported.join(", ")
                    ));
                    return;
                }
            }
        };
        let dir = config::data_dir().join("exports").join("radio");
        if let Err(e) = fs::create_dir_all(&dir) {
            self.notify(format!("No se pudo crear {}: {e}", dir.display()));
            return;
        }
        let base = feeds::safe_file_name(list_name);
        let mut written = Vec::new();
        for ext in formats {
            let contents = match crate::radio_playlist_io::export(&stations, ext) {
                Ok(contents) => contents,
                Err(e) => {
                    self.notify(e);
                    return;
                }
            };
            let mut path = dir.join(format!("{base}.{ext}"));
            if path.exists() {
                let mut suffix = 2usize;
                loop {
                    let candidate = dir.join(format!("{base}-{suffix}.{ext}"));
                    if !candidate.exists() {
                        path = candidate;
                        break;
                    }
                    suffix += 1;
                }
            }
            match fs::write(&path, contents) {
                Ok(()) => written.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                ),
                Err(e) => {
                    self.notify(format!("Error escribiendo {}: {e}", path.display()));
                    return;
                }
            }
        }
        self.notify(format!(
            "{} emisoras exportadas: {} · {}",
            stations.len(),
            written.join(", "),
            dir.display()
        ));
    }

    // ---------- acciones ----------

    fn items_window(&self, indices: &[usize], start: usize) -> (Vec<QueueItem>, usize) {
        let begin = start.saturating_sub(20);
        let end = (begin + 300).min(indices.len());
        let items = indices[begin..end]
            .iter()
            .map(|i| self.tracks[*i].to_item())
            .collect();
        (items, start - begin)
    }

    fn replace_with(&self, indices: &[usize], start: usize) -> Vec<Command> {
        if indices.is_empty() {
            return Vec::new();
        }
        let (items, start) = self.items_window(indices, start.min(indices.len() - 1));
        vec![Command::Replace { items, start }]
    }

    fn contextual_track(&self) -> Option<usize> {
        match self.view {
            View::Biblioteca if self.focus == 1 => self.lib_rows.get(self.lib_cur.index).copied(),
            View::Busqueda => match self.search_rows.get(self.search_cur.index) {
                Some(SearchRow::Track(i)) => Some(*i),
                _ => self.current_track_index(),
            },
            _ => self.current_track_index(),
        }
    }

    // ---------- emisoras: favoritas, recientes ----------

    /// Índices (en `settings.stations`) de las emisoras de la sección actual.
    pub fn radio_visible(&self) -> Vec<usize> {
        let stations = &self.settings.stations;
        match self.radio_filter {
            RadioFilter::All => (0..stations.len()).collect(),
            RadioFilter::Favs => stations
                .iter()
                .enumerate()
                .filter(|(_, s)| s.fav)
                .map(|(i, _)| i)
                .collect(),
            RadioFilter::Recent => self
                .settings
                .recent_stations
                .iter()
                .filter_map(|u| stations.iter().position(|s| &s.url == u))
                .collect(),
        }
    }

    fn selected_station(&self) -> Option<usize> {
        self.radio_visible().get(self.station_cur.index).copied()
    }

    /// Emisora en directo que suena ahora, si está en Mis emisoras.
    fn current_station_index(&self) -> Option<usize> {
        let item = self.snapshot.current_item().filter(|i| i.live)?;
        match &item.media {
            MediaSource::Url(url) => self.settings.stations.iter().position(|s| &s.url == url),
            MediaSource::File(_) => None,
        }
    }

    fn cycle_radio_section(&mut self) {
        self.station_cur.index = 0;
        if self.radio_browser_mode {
            self.radio_browser_mode = false;
            self.radio_filter = RadioFilter::All;
            self.notify("Mis emisoras");
            return;
        }
        match self.radio_filter {
            RadioFilter::All => {
                self.radio_filter = RadioFilter::Favs;
                self.notify("Favoritas ★");
            }
            RadioFilter::Favs => {
                self.radio_filter = RadioFilter::Recent;
                self.notify("Recientes");
            }
            RadioFilter::Recent => {
                self.radio_filter = RadioFilter::All;
                self.radio_browser_mode = true;
                self.notify("Radio Browser · busca emisoras con /");
            }
        }
    }

    /// Devuelve el índice de la emisora del resultado, añadiéndola a Mis emisoras si falta.
    fn station_from_result(&mut self, found: &StationResult, fav: bool) -> usize {
        let index = match self
            .settings
            .stations
            .iter()
            .position(|s| s.url == found.url)
        {
            Some(i) => i,
            None => {
                let mut station =
                    Station::new(found.name.clone(), found.url.clone(), found.bitrate);
                station.tags = found.tags.clone();
                self.settings.stations.push(station);
                self.settings.stations.len() - 1
            }
        };
        let st = &mut self.settings.stations[index];
        if st.uuid.is_empty() {
            st.uuid = found.uuid.clone();
        }
        if st.tags.is_empty() {
            st.tags = found.tags.clone();
        }
        if fav {
            st.fav = true;
        }
        self.save_settings();
        if self.radio_filter == RadioFilter::All {
            self.station_cur.index = index;
        }
        index
    }

    /// Anota la emisora en el historial, avisa a Radio Browser (si procede) y devuelve la orden de reproducir.
    fn play_station(&mut self, index: usize) -> Vec<Command> {
        let Some(st) = self.settings.stations.get(index).cloned() else {
            return Vec::new();
        };
        self.settings.push_recent(&st.url);
        self.save_settings();
        if self.settings.radio_click_count && !st.uuid.is_empty() {
            let uuid = st.uuid.clone();
            thread::spawn(move || radio_browser::register_click(&uuid));
        }
        vec![Command::Play(QueueItem::stream(
            &st.name,
            &st.subtitle(),
            &st.url,
            true,
        ))]
    }

    fn toggle_station_favorite(&mut self) {
        let index = if self.view == View::Radio && self.radio_browser_mode {
            match self.radio_results.get(self.radio_result_cur.index).cloned() {
                Some(found) => {
                    let known = self
                        .settings
                        .stations
                        .iter()
                        .find(|s| s.url == found.url)
                        .map(|s| s.fav)
                        .unwrap_or(false);
                    if !known {
                        Some(self.station_from_result(&found, true))
                    } else {
                        self.settings
                            .stations
                            .iter()
                            .position(|s| s.url == found.url)
                    }
                }
                None => None,
            }
        } else if self.view == View::Radio {
            self.selected_station()
                .or_else(|| self.current_station_index())
        } else {
            self.current_station_index()
        };
        let Some(i) = index else {
            self.notify("No hay emisora seleccionada");
            return;
        };
        let st = &mut self.settings.stations[i];
        st.fav = !st.fav;
        let message = if st.fav {
            format!("«{}» añadida a favoritas ★", st.name)
        } else {
            format!("«{}» quitada de favoritas", st.name)
        };
        self.save_settings();
        self.notify(message);
    }

    fn toggle_favorite(&mut self) {
        if self.view == View::Radio {
            self.toggle_station_favorite();
            return;
        }
        if self.view == View::Podcasts && self.podcast_browser {
            self.toggle_show_subscription();
            return;
        }
        let targets: Vec<usize> = if self.view == View::Albumes {
            self.grid()
                .get(self.grid_sel)
                .map(|g| g.tracks.clone())
                .unwrap_or_default()
        } else {
            self.contextual_track().into_iter().collect()
        };
        if targets.is_empty() {
            // Sin pista seleccionada: si suena una emisora, se marca esa.
            if self.current_station_index().is_some() {
                self.toggle_station_favorite();
                return;
            }
            self.notify("No hay pista seleccionada");
            return;
        }
        let all_fav = targets.iter().all(|i| self.is_favorite(*i));
        for i in &targets {
            let id = self.tracks[*i].id;
            if let Err(e) = self.library.set_favorite(id, !all_fav) {
                self.notify(e);
                return;
            }
        }
        self.favorites = self.library.favorites().unwrap_or_default();
        self.refresh_list_tracks();
        self.notify(if all_fav {
            "Quitado de favoritos"
        } else {
            "Añadido a favoritos ★"
        });
    }

    /// Pistas a las que se refiere la acción actual: carpeta, álbum/artista/género,
    /// resultado de búsqueda, pista de la cola o pista seleccionada.
    fn target_tracks(&self) -> Vec<usize> {
        match self.view {
            View::Albumes => self
                .grid()
                .get(self.grid_sel)
                .map(|g| g.tracks.clone())
                .unwrap_or_default(),
            View::Biblioteca if self.focus == 0 => self.lib_rows.clone(),
            View::Busqueda => match self.search_rows.get(self.search_cur.index) {
                Some(SearchRow::Track(i)) => vec![*i],
                Some(SearchRow::Album(i)) => self
                    .albums
                    .get(*i)
                    .map(|g| g.tracks.clone())
                    .unwrap_or_default(),
                Some(SearchRow::Artist(i)) => self
                    .artists
                    .get(*i)
                    .map(|g| g.tracks.clone())
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
            View::Cola => self
                .snapshot
                .queue
                .get(self.queue_cur.index)
                .and_then(|q| match &q.media {
                    MediaSource::File(p) => self.track_by_path.get(p).copied(),
                    _ => None,
                })
                .into_iter()
                .collect(),
            _ => self.contextual_track().into_iter().collect(),
        }
    }

    pub fn queue_track_marked(&self, index: usize) -> bool {
        self.snapshot
            .queue
            .get(index)
            .and_then(|q| match &q.media {
                MediaSource::File(p) => self.track_by_path.get(p).copied(),
                _ => None,
            })
            .map(|i| self.is_marked(i))
            .unwrap_or(false)
    }

    pub fn is_marked(&self, track: usize) -> bool {
        self.tracks
            .get(track)
            .map(|t| self.marked.contains(&t.id))
            .unwrap_or(false)
    }

    pub fn is_group_marked(&self, tracks: &[usize]) -> bool {
        !tracks.is_empty() && tracks.iter().all(|i| self.is_marked(*i))
    }

    pub fn is_episode_marked(&self, ep: &Episode) -> bool {
        self.episode_marks.contains(&ep.url)
    }

    /// Episodios marcados del podcast abierto.
    pub fn marked_episode_count(&self) -> usize {
        self.episodes
            .get(&self.podcast_cur.index)
            .map(|eps| {
                eps.iter()
                    .filter(|ep| self.episode_marks.contains(&ep.url))
                    .count()
            })
            .unwrap_or(0)
    }

    fn toggle_episode_mark(&mut self) {
        let Some(ep) = self
            .episodes
            .get(&self.podcast_cur.index)
            .and_then(|e| e.get(self.episode_cur.index))
            .cloned()
        else {
            self.notify("Selecciona un episodio");
            return;
        };
        let marked = if self.episode_marks.remove(&ep.url) {
            false
        } else {
            self.episode_marks.insert(ep.url.clone());
            true
        };
        let n = self.marked_episode_count();
        self.notify(if marked {
            format!("Episodio marcado · {n} en este podcast · d los descarga")
        } else {
            format!("Episodio desmarcado · {n} marcados")
        });
    }

    fn toggle_mark(&mut self) {
        if self.view == View::Podcasts && !self.podcast_browser && self.focus == 1 {
            self.toggle_episode_mark();
            return;
        }
        let targets = self.target_tracks();
        if targets.is_empty() {
            self.notify("Nada que marcar aquí (las emisoras y podcasts no se marcan)");
            return;
        }
        let all = targets.iter().all(|i| self.is_marked(*i));
        for i in &targets {
            let id = self.tracks[*i].id;
            if all {
                self.marked.remove(&id);
            } else {
                self.marked.insert(id);
            }
        }
        let n = self.marked.len();
        self.notify(if all {
            format!("Desmarcadas · {n} marcadas en total")
        } else {
            format!(
                "Marcadas {} · {n} en total · l las añade a la lista",
                targets.len()
            )
        });
    }

    fn add_to_list(&mut self) {
        let Some(id) = self.selected_list_id() else {
            self.notify("Elige una lista propia en Listas (6) y vuelve a pulsar l");
            return;
        };
        // Con marcas: todas las marcadas (en el orden de la biblioteca); sin marcas: lo seleccionado.
        let ids: Vec<i64> = if self.marked.is_empty() {
            self.target_tracks()
                .into_iter()
                .map(|i| self.tracks[i].id)
                .collect()
        } else {
            self.tracks
                .iter()
                .filter(|t| self.marked.contains(&t.id))
                .map(|t| t.id)
                .collect()
        };
        if ids.is_empty() {
            self.notify("No hay pistas seleccionadas ni marcadas");
            return;
        }
        let (mut ok, mut last_err) = (0usize, None);
        for tid in ids {
            match self.library.add_to_playlist(id, tid) {
                Ok(()) => ok += 1,
                Err(e) => last_err = Some(e),
            }
        }
        self.marked.clear();
        self.refresh_list_tracks();
        match (ok, last_err) {
            (0, Some(e)) => self.notify(e),
            _ => self.notify(format!("{ok} pista(s) añadidas a {}", self.list_name())),
        }
    }

    /// Ajusta y persiste el límite configurable de descargas paralelas.
    fn adjust_parallel_downloads(&mut self, delta: i32) {
        let current = self.dl.parallel_limit() as i32;
        let next = (current + delta).clamp(1, crate::downloads::MAX_PARALLEL as i32) as usize;
        if next == self.dl.parallel_limit() {
            return;
        }
        self.dl.set_parallel_limit(next);
        self.settings.parallel_downloads = next;
        self.save_settings();
        self.pump_downloads();
        self.notify(format!(
            "Descargas paralelas máximas: {next}/{}",
            crate::downloads::MAX_PARALLEL
        ));
    }

    fn set_eq(&mut self, gains: [f32; EQ_BANDS]) -> Vec<Command> {
        self.eq = gains;
        self.eq_edit_tick = self.tick;
        vec![Command::SetEq(gains)]
    }

    pub fn action(&mut self, action: Action) -> Vec<Command> {
        use Action::*;
        if self.help {
            if matches!(action, Help | Cancel | Detach | QuitAll) {
                self.help = false;
            }
            return Vec::new();
        }
        if self.dl_choice.is_some() {
            match action {
                Up | Down => {
                    if let Some(choice) = self.dl_choice.as_mut() {
                        choice.mode = match choice.mode {
                            DownloadMode::Parallel => DownloadMode::Queue,
                            DownloadMode::Queue => DownloadMode::Parallel,
                        };
                    }
                }
                Play => self.confirm_dl_choice(),
                Cancel | Help | Manual | Detach | QuitAll | Downloads => {
                    self.dl_choice = None;
                    self.notify("Descarga cancelada · las marcas se conservan");
                }
                _ => {}
            }
            return Vec::new();
        }
        if self.dl_window {
            match action {
                Downloads | Cancel | Help | Manual | Detach | QuitAll => self.dl_window = false,
                Up => self.dl_scroll = self.dl_scroll.saturating_sub(1),
                Down => {
                    self.dl_scroll =
                        (self.dl_scroll + 1).min(self.download_entry_count().saturating_sub(1))
                }
                SeekBackward => self.adjust_parallel_downloads(-1),
                SeekForward => self.adjust_parallel_downloads(1),
                Delete => self.cancel_download_at_selection(),
                _ => {}
            }
            return Vec::new();
        }
        if self.manual && matches!(action, Manual | Cancel | Help | Detach | QuitAll) {
            self.manual = false;
            return Vec::new();
        }
        if let Some(i) = action.view_index() {
            self.set_view(View::ALL[i]);
            return Vec::new();
        }
        match action {
            Detach => {
                self.quit = true;
                Vec::new()
            }
            QuitAll => {
                self.quit = true;
                self.quit_all = true;
                Vec::new()
            }
            Help => {
                self.help = true;
                self.manual = false;
                Vec::new()
            }
            Downloads => {
                self.dl_window = true;
                self.dl_scroll = 0;
                Vec::new()
            }
            ManageMusicFolders => {
                self.set_view(View::Biblioteca);
                self.open_music_folder_picker();
                Vec::new()
            }
            Manual => {
                self.manual = true;
                self.help = false;
                self.manual_page = 0;
                Vec::new()
            }
            Cancel => {
                if self.view == View::Podcasts && self.podcast_browser {
                    if self.podcast_section == PodcastSection::Categories && self.category_open {
                        self.category_open = false;
                        self.shows.clear();
                    } else {
                        self.podcast_browser = false;
                        self.focus = 0;
                    }
                } else if self.view != View::Inicio {
                    self.set_view(View::Inicio);
                }
                Vec::new()
            }
            Toggle => vec![Command::Toggle],
            Stop => vec![Command::Stop],
            Next => vec![Command::Next],
            Previous => vec![Command::Previous],
            VolumeUp => vec![Command::VolumeDelta(0.05)],
            VolumeDown => vec![Command::VolumeDelta(-0.05)],
            Shuffle => vec![Command::ToggleShuffle],
            Repeat => vec![Command::CycleRepeat],
            ReplayGain => vec![Command::SetReplayGain(!self.snapshot.replay_gain)],
            EqToggle => vec![Command::SetEqEnabled(!self.snapshot.eq_enabled)],
            Crossfade => {
                let pos = CROSSFADE_STEPS
                    .iter()
                    .position(|s| *s == self.snapshot.crossfade_secs)
                    .unwrap_or(0);
                vec![Command::SetCrossfade(
                    CROSSFADE_STEPS[(pos + 1) % CROSSFADE_STEPS.len()],
                )]
            }
            EqPreset => {
                self.eq_preset = (self.eq_preset + 1) % EQ_PRESETS.len();
                let (name, gains) = EQ_PRESETS[self.eq_preset];
                self.notify(format!("Ecualizador: {name}"));
                self.set_eq(gains)
            }
            ClearQueue => vec![Command::Clear],
            CycleViz => {
                self.settings.visualizer =
                    (self.settings.visualizer + 1) % crate::widgets::VIZ_NAMES.len();
                self.notify(format!(
                    "Visualizador: {}",
                    crate::widgets::VIZ_NAMES[self.settings.visualizer]
                ));
                self.save_settings();
                Vec::new()
            }
            CycleTheme => {
                let cur = self.theme_source.trim().to_ascii_lowercase();
                let n = crate::theme::THEMES.len();
                let next = crate::theme::THEMES
                    .iter()
                    .position(|(name, _)| *name == cur)
                    .map_or(0, |i| (i + 1) % n);
                let name = crate::theme::THEMES[next].0;
                match Theme::resolve(name) {
                    Ok(theme) => {
                        self.theme = theme;
                        self.theme_source = name.to_string();
                        self.settings.theme = name.to_string();
                        self.notify(format!("Tema: {}", self.theme.name));
                        self.save_settings();
                    }
                    Err(e) => self.notify(format!("Tema no válido: {e}")),
                }
                Vec::new()
            }
            ReloadTheme => {
                match Theme::resolve(&self.theme_source) {
                    Ok(theme) => {
                        self.theme = theme;
                        self.notify("Tema recargado");
                    }
                    Err(e) => self.notify(format!("Tema no válido: {e}")),
                }
                Vec::new()
            }
            Search => {
                if self.view == View::Podcasts {
                    self.input = Some(Input {
                        mode: InputMode::PodcastSearch,
                        text: String::new(),
                    });
                } else if self.view == View::Radio && self.radio_browser_mode {
                    self.input = Some(Input {
                        mode: InputMode::RadioSearch,
                        text: String::new(),
                    });
                } else {
                    self.set_view(View::Busqueda);
                }
                Vec::new()
            }
            Scan => {
                if self.view == View::Podcasts && self.podcast_browser {
                    self.popular_cache = None;
                    if self.podcast_section == PodcastSection::Popular {
                        self.load_popular();
                    }
                } else if self.view == View::Podcasts {
                    let i = self.podcast_cur.index;
                    self.episodes.remove(&i);
                    self.fetch_feed(i);
                } else {
                    self.start_scan();
                }
                Vec::new()
            }
            LyricsLater => {
                self.lyrics_offset_ms += 500;
                Vec::new()
            }
            LyricsEarlier => {
                self.lyrics_offset_ms -= 500;
                Vec::new()
            }
            Favorite => {
                self.toggle_favorite();
                Vec::new()
            }
            AddToList => {
                self.add_to_list();
                Vec::new()
            }
            Mark => {
                self.toggle_mark();
                Vec::new()
            }
            ClearMarks => {
                self.marked.clear();
                self.episode_marks.clear();
                self.notify("Marcas quitadas");
                Vec::new()
            }
            NewList => {
                self.set_view(View::Listas);
                self.input = Some(Input {
                    mode: InputMode::NewList,
                    text: String::new(),
                });
                Vec::new()
            }
            RenameList => {
                if self.selected_list_id().is_some() {
                    self.input = Some(Input {
                        mode: InputMode::RenameList,
                        text: self.list_name(),
                    });
                } else {
                    self.notify("Selecciona una lista propia en Listas (6)");
                }
                Vec::new()
            }
            ImportM3u => {
                if self.view == View::Radio {
                    self.open_radio_playlist_picker();
                } else {
                    self.set_view(View::Listas);
                    self.open_directory_picker();
                }
                Vec::new()
            }
            ExportM3u => {
                if self.view == View::Radio {
                    self.input = Some(Input {
                        mode: InputMode::ExportRadioName,
                        text: String::new(),
                    });
                } else if self.view == View::Listas {
                    self.input = Some(Input {
                        mode: InputMode::ExportList,
                        text: String::new(),
                    });
                } else {
                    self.notify("Abre Listas (6), elige una lista y pulsa Ctrl+E");
                }
                Vec::new()
            }
            Download => {
                self.download_episode();
                Vec::new()
            }
            DownloadAll => {
                self.download_channel();
                Vec::new()
            }
            Up => self.navigate(-1),
            Down => self.navigate(1),
            SeekForward => {
                if self.view == View::Ajustes && self.settings_cur.index == 4 {
                    self.adjust_parallel_downloads(1);
                    Vec::new()
                } else if self.view == View::Ajustes {
                    self.settings_cur.step(SETTINGS_SECTIONS.len(), 1);
                    if self.settings_cur.index == 6 {
                        self.sync_theme_cursor();
                    }
                    Vec::new()
                } else if self.view == View::Podcasts && self.podcast_browser {
                    self.shift_podcast_browser(1);
                    Vec::new()
                } else if self.horizontal_view() {
                    self.navigate_h(1)
                } else {
                    vec![Command::SeekBy(5)]
                }
            }
            SeekBackward => {
                if self.view == View::Ajustes && self.settings_cur.index == 4 {
                    self.adjust_parallel_downloads(-1);
                    Vec::new()
                } else if self.view == View::Ajustes {
                    self.settings_cur.step(SETTINGS_SECTIONS.len(), -1);
                    if self.settings_cur.index == 6 {
                        self.sync_theme_cursor();
                    }
                    Vec::new()
                } else if self.view == View::Podcasts && self.podcast_browser {
                    self.shift_podcast_browser(-1);
                    Vec::new()
                } else if self.horizontal_view() {
                    self.navigate_h(-1)
                } else {
                    vec![Command::SeekBy(-5)]
                }
            }
            FocusNext => {
                if self.view == View::Radio {
                    self.cycle_radio_section();
                } else if self.view == View::Podcasts && self.podcast_browser { /* ←/→ navega el directorio; Tab conserva el contexto */
                } else if self.view == View::Podcasts {
                    if self.focus == 0 {
                        self.focus_podcast_episodes();
                    } else {
                        self.focus = 0;
                    }
                } else {
                    self.focus_next();
                }
                Vec::new()
            }
            Play => self.activate(),
            Enqueue if matches!(self.view, View::Radio | View::Podcasts) => {
                self.begin_add();
                Vec::new()
            }
            Enqueue => self.enqueue_selected(),
            AddSource => {
                self.begin_add();
                Vec::new()
            }
            BrowsePodcasts if self.view == View::Podcasts => {
                self.open_podcast_browser();
                Vec::new()
            }
            BrowsePodcasts => Vec::new(),
            Delete => self.delete_selected(),
            MoveUp => self.move_queue(true),
            MoveDown => self.move_queue(false),
            _ => Vec::new(),
        }
    }

    // ---------- explorador de podcasts (en español) ----------

    /// Lanza una consulta al directorio en segundo plano; la respuesta llega como `Bg::Shows`.
    fn fetch_shows<F>(&mut self, section: PodcastSection, job: F)
    where
        F: FnOnce(&podcast_directory::Directory, &str) -> Result<Vec<ShowResult>, String>
            + Send
            + 'static,
    {
        self.shows_generation += 1;
        let generation = self.shows_generation;
        self.shows.clear();
        self.show_cur.index = 0;
        self.shows_loading = true;
        let country = self.settings.podcast_country.clone();
        let tx = self.bg_tx.clone();
        thread::spawn(move || {
            let directory = podcast_directory::Directory::default();
            let _ = tx.send(Bg::Shows(generation, section, job(&directory, &country)));
        });
    }

    fn load_popular(&mut self) {
        if let Some(cache) = self.popular_cache.clone() {
            self.shows_generation += 1;
            self.shows_loading = false;
            self.shows = cache;
            self.show_cur.index = 0;
            return;
        }
        self.fetch_shows(PodcastSection::Popular, |directory, country| {
            directory.top(country)
        });
    }

    fn open_podcast_browser(&mut self) {
        self.podcast_browser = true;
        self.podcast_section = PodcastSection::Popular;
        self.category_open = false;
        self.load_popular();
    }

    /// ←/→ cambia entre las áreas del directorio; Tab queda reservado a canales/episodios.
    fn shift_podcast_browser(&mut self, direction: i32) {
        if !self.podcast_browser {
            return;
        }
        if self.category_open {
            self.category_open = false;
            self.shows.clear();
        }
        let next = match (self.podcast_section, direction.signum()) {
            (PodcastSection::Popular, 1) => PodcastSection::Categories,
            (PodcastSection::Categories, -1) => PodcastSection::Popular,
            (PodcastSection::Results, -1) => PodcastSection::Popular,
            (PodcastSection::Results, 1) => PodcastSection::Categories,
            (PodcastSection::Popular, -1) => PodcastSection::Categories,
            (PodcastSection::Categories, 1) => PodcastSection::Popular,
            _ => PodcastSection::Popular,
        };
        if next == self.podcast_section {
            return;
        }
        self.podcast_section = next;
        self.shows_generation += 1;
        self.shows_loading = false;
        self.shows.clear();
        if next == PodcastSection::Popular {
            self.load_popular();
        }
    }

    fn search_podcasts(&mut self, query: &str) {
        let term = query.trim().to_string();
        if term.is_empty() {
            return;
        }
        self.podcast_browser = true;
        self.podcast_section = PodcastSection::Results;
        self.category_open = false;
        self.notify(format!("Buscando «{term}» en español…"));
        self.fetch_shows(PodcastSection::Results, move |directory, country| {
            directory.search(&term, None, country)
        });
    }

    fn podcast_browser_enter(&mut self) {
        if self.podcast_section == PodcastSection::Categories && !self.category_open {
            let Some(category) = CATEGORIES.get(self.category_cur.index) else {
                return;
            };
            self.category_open = true;
            self.notify(format!("Cargando «{}»…", category.name));
            let (id, name) = (category.id, category.name);
            self.fetch_shows(PodcastSection::Categories, move |directory, country| {
                directory.search(name, Some(id), country)
            });
            return;
        }
        let Some(show) = self.shows.get(self.show_cur.index).cloned() else {
            return;
        };
        if let Some(i) = self
            .settings
            .podcasts
            .iter()
            .position(|p| p.url == show.feed_url)
        {
            self.podcast_cur.index = i;
            self.episode_cur.index = 0;
            self.podcast_browser = false;
            self.focus = 1;
            if !self.episodes.contains_key(&i) {
                self.fetch_feed(i);
            }
            return;
        }
        self.notify(format!("Cargando «{}»…", show.title));
        let tx = self.bg_tx.clone();
        thread::spawn(move || {
            let event = match feeds::fetch_feed_full(&show.feed_url) {
                Ok((_, eps)) => Bg::PodcastAdded(
                    PodcastFeed {
                        name: show.title,
                        url: show.feed_url,
                    },
                    eps,
                ),
                Err(e) => Bg::AddFailed(e),
            };
            let _ = tx.send(event);
        });
    }

    fn toggle_show_subscription(&mut self) {
        let Some(show) = self.shows.get(self.show_cur.index).cloned() else {
            self.notify("Elige un podcast de la lista");
            return;
        };
        if let Some(i) = self
            .settings
            .podcasts
            .iter()
            .position(|p| p.url == show.feed_url)
        {
            self.settings.podcasts.remove(i);
            self.episodes.clear();
            self.podcast_cur.index = self
                .podcast_cur
                .index
                .min(self.settings.podcasts.len().saturating_sub(1));
            self.notify(format!("Suscripción a «{}» cancelada", show.title));
        } else {
            self.settings.podcasts.push(PodcastFeed {
                name: show.title.clone(),
                url: show.feed_url,
            });
            self.notify(format!("Suscrito a «{}»", show.title));
        }
        self.save_settings();
    }

    fn search_radio_browser(&mut self, query: &str) {
        if self.radio_searching {
            self.notify("Ya hay una búsqueda en curso");
            return;
        }
        self.radio_searching = true;
        self.notify(format!("Buscando «{query}» en Radio Browser…"));
        let tx = self.bg_tx.clone();
        let query = query.to_string();
        thread::spawn(move || {
            let _ = tx.send(Bg::RadioSearch(radio_browser::search(&query)));
        });
    }

    fn begin_add(&mut self) {
        let mode = match self.view {
            View::Radio => InputMode::AddStation,
            View::Podcasts => InputMode::AddPodcast,
            _ => {
                self.notify("Abre Radio (7) o Podcasts (8) para añadir una URL");
                return;
            }
        };
        self.input = Some(Input {
            mode,
            text: String::new(),
        });
    }

    fn move_queue(&mut self, up: bool) -> Vec<Command> {
        if !matches!(self.view, View::Cola | View::Inicio) {
            return Vec::new();
        }
        let i = self.queue_cur.index;
        let len = self.snapshot.queue.len();
        if up && i > 0 {
            self.queue_cur.index = i - 1;
            return vec![Command::MoveUp(i)];
        }
        if !up && i + 1 < len {
            self.queue_cur.index = i + 1;
            return vec![Command::MoveDown(i)];
        }
        Vec::new()
    }

    fn focus_podcast_episodes(&mut self) {
        self.focus = 1;
        let index = self.podcast_cur.index;
        if !self.episodes.contains_key(&index) && self.feed_loading != Some(index) {
            self.fetch_feed(index);
        }
    }

    fn focus_next(&mut self) {
        match self.view {
            View::Albumes => {
                self.album_tab = (self.album_tab + 1) % ALBUM_TABS.len();
                self.grid_sel = 0;
            }
            View::Biblioteca | View::Listas | View::Podcasts => self.focus = (self.focus + 1) % 2,
            View::Busqueda => {
                self.input = Some(Input {
                    mode: InputMode::Search,
                    text: self.search_text.clone(),
                });
            }
            _ => {}
        }
    }

    fn navigate(&mut self, delta: i32) -> Vec<Command> {
        match self.view {
            View::Inicio | View::Cola => {
                let n = self.snapshot.queue.len();
                self.queue_cur.step(n, delta);
            }
            View::Biblioteca => {
                if self.focus == 0 {
                    let n = self.folders.len();
                    self.folder_cur.step(n, delta);
                    self.refresh_lib_rows();
                    self.lib_cur.index = 0;
                } else {
                    let n = self.lib_rows.len();
                    self.lib_cur.step(n, delta);
                }
            }
            View::Busqueda => self.step_search(delta),
            View::Albumes => {
                let n = self.grid_len();
                if n > 0 {
                    let cols = self.grid_cols.max(1) as i64;
                    let target = self.grid_sel as i64 + delta as i64 * cols;
                    if target >= 0 && target < n as i64 {
                        self.grid_sel = target as usize;
                    }
                }
            }
            View::Listas => {
                if self.focus == 0 {
                    let n = self.list_len();
                    self.list_cur.step(n, delta);
                    self.pending_delete = None;
                    self.refresh_list_tracks();
                } else {
                    self.menu_cur.step(LIST_MENU.len(), delta);
                }
            }
            View::Radio => {
                if self.radio_browser_mode {
                    self.radio_result_cur.step(self.radio_results.len(), delta);
                } else {
                    let n = self.radio_visible().len();
                    self.station_cur.step(n, delta);
                }
            }
            View::Podcasts if self.podcast_browser => {
                if self.podcast_section == PodcastSection::Categories && !self.category_open {
                    self.category_cur.step(CATEGORIES.len(), delta);
                } else {
                    let n = self.shows.len();
                    self.show_cur.step(n, delta);
                }
            }
            View::Podcasts => {
                if self.focus == 0 {
                    let n = self.settings.podcasts.len();
                    self.podcast_cur.step(n, delta);
                    self.episode_cur.index = 0;
                    let i = self.podcast_cur.index;
                    if !self.episodes.contains_key(&i) {
                        self.fetch_feed(i);
                    }
                } else {
                    let n = self
                        .episodes
                        .get(&self.podcast_cur.index)
                        .map(|e| e.len())
                        .unwrap_or(0);
                    self.episode_cur.step(n, delta);
                }
            }
            View::Audio => {
                let mut gains = self.eq;
                let g = &mut gains[self.eq_sel];
                *g = (*g - delta as f32).clamp(-dsp::EQ_RANGE_DB, dsp::EQ_RANGE_DB);
                return self.set_eq(gains);
            }
            View::Ajustes => {
                if self.settings_cur.index == 6 {
                    self.theme_cur.step(crate::theme::THEMES.len(), delta);
                } else {
                    self.settings_cur.step(SETTINGS_SECTIONS.len(), delta);
                    if self.settings_cur.index == 6 {
                        self.sync_theme_cursor();
                    }
                }
            }
            View::Letras | View::Completa => {}
        }
        Vec::new()
    }

    fn navigate_h(&mut self, delta: i32) -> Vec<Command> {
        match self.view {
            View::Albumes => {
                let n = self.grid_len() as i64;
                let target = self.grid_sel as i64 + delta as i64;
                if n > 0 && target >= 0 && target < n {
                    self.grid_sel = target as usize;
                }
            }
            View::Audio => {
                self.eq_sel =
                    (self.eq_sel as i64 + delta as i64).rem_euclid(EQ_BANDS as i64) as usize
            }
            View::Biblioteca => self.folder_expand(delta > 0),
            _ => {}
        }
        Vec::new()
    }

    fn activate(&mut self) -> Vec<Command> {
        match self.view {
            View::Inicio | View::Cola => {
                if self.snapshot.queue.is_empty() {
                    Vec::new()
                } else {
                    vec![Command::PlayIndex(self.queue_cur.index)]
                }
            }
            View::Biblioteca => {
                if self.focus == 0 {
                    let has = self
                        .folders
                        .get(self.folder_cur.index)
                        .map(|f| f.has_children)
                        .unwrap_or(false);
                    if has {
                        let open = !self.folders[self.folder_cur.index].expanded;
                        self.folder_expand(open);
                    } else {
                        self.focus = 1;
                    }
                    return Vec::new();
                }
                let rows = self.lib_rows.clone();
                self.replace_with(&rows, self.lib_cur.index)
            }
            View::Busqueda => match self.search_rows.get(self.search_cur.index).cloned() {
                Some(SearchRow::Track(i)) => vec![Command::Play(self.tracks[i].to_item())],
                Some(SearchRow::Album(i)) => {
                    let t = self.albums[i].tracks.clone();
                    self.replace_with(&t, 0)
                }
                Some(SearchRow::Artist(i)) => {
                    let t = self.artists[i].tracks.clone();
                    self.replace_with(&t, 0)
                }
                _ => Vec::new(),
            },
            View::Albumes => match self.grid().get(self.grid_sel).map(|g| g.tracks.clone()) {
                Some(t) => self.replace_with(&t, 0),
                None => Vec::new(),
            },
            View::Listas => {
                if self.focus == 0 {
                    let t = self.list_tracks.clone();
                    self.replace_with(&t, 0)
                } else {
                    self.run_list_menu()
                }
            }
            View::Radio => {
                if self.radio_browser_mode {
                    let Some(found) = self.radio_results.get(self.radio_result_cur.index).cloned()
                    else {
                        return Vec::new();
                    };
                    let index = self.station_from_result(&found, false);
                    self.notify(format!("«{}» lista y añadida a Mis emisoras", found.name));
                    self.play_station(index)
                } else {
                    match self.selected_station() {
                        Some(i) => self.play_station(i),
                        None => Vec::new(),
                    }
                }
            }
            View::Podcasts if self.podcast_browser => {
                self.podcast_browser_enter();
                Vec::new()
            }
            View::Podcasts => {
                if self.focus == 0 {
                    self.focus_podcast_episodes();
                    return Vec::new();
                }
                match self.selected_episode() {
                    Some((item, _)) => vec![Command::Play(item)],
                    None => Vec::new(),
                }
            }
            View::Audio => self.set_eq(EQ_PRESETS[0].1),
            View::Letras | View::Completa => vec![Command::Toggle],
            View::Ajustes => {
                if self.settings_cur.index == 2 {
                    self.start_scan();
                } else if self.settings_cur.index == 6 {
                    self.select_theme(self.theme_cur.index);
                }
                Vec::new()
            }
        }
    }

    fn run_list_menu(&mut self) -> Vec<Command> {
        match self.menu_cur.index {
            0 => {
                self.input = Some(Input {
                    mode: InputMode::NewList,
                    text: String::new(),
                })
            }
            1 => {
                if self.selected_list_id().is_some() {
                    self.input = Some(Input {
                        mode: InputMode::RenameList,
                        text: self.list_name(),
                    });
                } else {
                    self.notify("«Favoritas» no se puede renombrar");
                }
            }
            2 => return self.delete_selected(),
            3 => self.open_directory_picker(),
            _ => {
                self.input = Some(Input {
                    mode: InputMode::ExportList,
                    text: String::new(),
                })
            }
        }
        Vec::new()
    }

    fn enqueue_selected(&mut self) -> Vec<Command> {
        let indices: Vec<usize> = match self.view {
            View::Biblioteca if self.focus == 1 => self
                .lib_rows
                .get(self.lib_cur.index)
                .copied()
                .into_iter()
                .collect(),
            View::Biblioteca => self.folder_tracks(),
            View::Busqueda => match self.search_rows.get(self.search_cur.index) {
                Some(SearchRow::Track(i)) => vec![*i],
                Some(SearchRow::Album(i)) => self.albums[*i].tracks.clone(),
                Some(SearchRow::Artist(i)) => self.artists[*i].tracks.clone(),
                _ => Vec::new(),
            },
            View::Albumes => self
                .grid()
                .get(self.grid_sel)
                .map(|g| g.tracks.clone())
                .unwrap_or_default(),
            View::Listas if self.focus == 0 => self.list_tracks.clone(),
            _ => Vec::new(),
        };
        if indices.is_empty() {
            return Vec::new();
        }
        let capped = &indices[..indices.len().min(300)];
        let items: Vec<QueueItem> = capped.iter().map(|i| self.tracks[*i].to_item()).collect();
        self.notify(format!("{} pistas añadidas a la cola", items.len()));
        vec![Command::EnqueueMany(items)]
    }

    fn delete_selected(&mut self) -> Vec<Command> {
        match self.view {
            View::Cola | View::Inicio => {
                if self.snapshot.queue.is_empty() {
                    Vec::new()
                } else {
                    vec![Command::Remove(self.queue_cur.index)]
                }
            }
            View::Listas => {
                let Some(id) = self.selected_list_id() else {
                    self.notify("«Favoritas» no se puede eliminar; usa f para quitar pistas");
                    return Vec::new();
                };
                if self.pending_delete == Some(id) {
                    match self.library.delete_playlist(id) {
                        Ok(()) => {
                            self.pending_delete = None;
                            self.list_cur.index = 0;
                            self.reload_library();
                            self.notify("Lista eliminada");
                        }
                        Err(e) => self.notify(e),
                    }
                } else {
                    self.pending_delete = Some(id);
                    self.notify(format!(
                        "Pulsa x otra vez para eliminar «{}»",
                        self.list_name()
                    ));
                }
                Vec::new()
            }
            View::Radio => {
                let Some(st) = self
                    .selected_station()
                    .and_then(|i| self.settings.stations.get(i))
                    .cloned()
                else {
                    return Vec::new();
                };
                if self.radio_filter == RadioFilter::Recent && !self.radio_browser_mode {
                    self.settings.recent_stations.retain(|u| u != &st.url);
                    self.save_settings();
                    self.notify(format!("«{}» quitada del historial", st.name));
                } else if self.pending_source.as_deref() == Some(st.url.as_str()) {
                    self.pending_source = None;
                    self.settings.stations.retain(|s| s.url != st.url);
                    self.settings.recent_stations.retain(|u| u != &st.url);
                    self.save_settings();
                    self.notify(format!("Emisora «{}» eliminada", st.name));
                } else {
                    self.notify(format!("Pulsa x otra vez para quitar «{}»", st.name));
                    self.pending_source = Some(st.url);
                }
                Vec::new()
            }
            View::Podcasts if self.focus == 0 && !self.podcast_browser => {
                let Some(p) = self.settings.podcasts.get(self.podcast_cur.index).cloned() else {
                    return Vec::new();
                };
                if self.pending_source.as_deref() == Some(p.url.as_str()) {
                    self.pending_source = None;
                    self.settings.podcasts.retain(|q| q.url != p.url);
                    self.episodes.clear();
                    self.save_settings();
                    self.notify(format!("Podcast «{}» eliminado", p.name));
                } else {
                    self.notify(format!("Pulsa x otra vez para quitar «{}»", p.name));
                    self.pending_source = Some(p.url);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Episodio seleccionado como elemento de cola; usa el archivo local si ya se descargó.
    pub fn selected_episode(&self) -> Option<(QueueItem, PathBuf)> {
        let feed = self.settings.podcasts.get(self.podcast_cur.index)?;
        let ep = self
            .episodes
            .get(&self.podcast_cur.index)?
            .get(self.episode_cur.index)?;
        let local = feeds::find_episode_path(
            &config::data_dir().join("podcasts"),
            &feed.name,
            &ep.url,
            &ep.title,
        );
        let item = if local.exists() {
            let mut item = QueueItem::from_path(&local);
            item.title = ep.title.clone();
            item.artist = feed.name.clone();
            item
        } else {
            let mut item = QueueItem::stream(&ep.title, &feed.name, &ep.url, false);
            // La duración viene del RSS (itunes:duration). El decodificador de
            // un flujo HTTP no siempre puede deducirla por sí mismo.
            item.duration = feeds::parse_episode_duration(&ep.duration);
            item
        };
        Some((item, local))
    }

    pub fn is_downloaded(&self, ep: &Episode) -> bool {
        let name = self
            .settings
            .podcasts
            .get(self.podcast_cur.index)
            .map(|p| p.name.as_str())
            .unwrap_or("Podcast");
        feeds::find_episode_path(
            &config::data_dir().join("podcasts"),
            name,
            &ep.url,
            &ep.title,
        )
        .exists()
    }

    fn download_episode(&mut self) {
        if self.view != View::Podcasts || self.podcast_browser {
            self.notify("Abre un podcast en Mis podcasts (8) para descargar episodios");
            return;
        }
        if self.marked_episode_count() > 0 {
            self.download_marked();
            return;
        }
        let Some(ep) = self
            .episodes
            .get(&self.podcast_cur.index)
            .and_then(|e| e.get(self.episode_cur.index))
            .cloned()
        else {
            self.notify("Selecciona un episodio");
            return;
        };
        if self.is_downloaded(&ep) {
            self.notify(format!("«{}» ya está descargado", ep.title));
            return;
        }
        if self.dl.jobs().iter().any(|job| job.url == ep.url) {
            self.notify(format!("«{}» ya está en la cola de descargas", ep.title));
            return;
        }
        let waiting = self.dl.is_full();
        let podcast_name = self
            .settings
            .podcasts
            .get(self.podcast_cur.index)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Podcast".into());
        self.dl
            .add_for_podcast(ep.title.clone(), ep.url.clone(), podcast_name);
        if waiting {
            let active = self.dl.active_count();
            self.notify(format!(
                "«{}» en cola ({} activas, máximo {})",
                ep.title,
                active,
                self.dl.effective_limit()
            ));
        } else {
            self.notify(format!("Descargando «{}»…", ep.title));
        }
        self.pump_downloads();
    }

    /// Descarga los episodios marcados del podcast abierto (los ya descargados o en curso se omiten).
    /// Con dos o más pide elegir entre descarga en paralelo o en cola; con uno la añade directamente.
    fn download_marked(&mut self) {
        let eps: Vec<Episode> = self
            .episodes
            .get(&self.podcast_cur.index)
            .map(|e| {
                e.iter()
                    .filter(|ep| self.episode_marks.contains(&ep.url))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let total = eps.len();
        let urls: Vec<String> = eps.iter().map(|ep| ep.url.clone()).collect();
        let episodes: Vec<Episode> = eps
            .into_iter()
            .filter(|ep| !self.is_downloaded(ep) && !self.dl.jobs().iter().any(|j| j.url == ep.url))
            .collect();
        let skipped = total - episodes.len();
        match episodes.len() {
            0 => {
                for url in &urls {
                    self.episode_marks.remove(url);
                }
                self.notify(format!(
                    "Nada que descargar: {skipped} marcado(s) ya descargados o en curso"
                ));
            }
            1 => {
                let mode = self.dl.mode();
                self.dl_choice = Some(DlChoice {
                    episodes,
                    mode,
                    skipped,
                });
                self.confirm_dl_choice();
            }
            _ => {
                // Se propone el modo actual del gestor (por defecto, en paralelo).
                let mode = self.dl.mode();
                self.dl_choice = Some(DlChoice {
                    episodes,
                    mode,
                    skipped,
                });
            }
        }
    }

    /// Aplica la opción elegida en el diálogo: añade los episodios y arranca lo que quepa.
    fn confirm_dl_choice(&mut self) {
        let Some(choice) = self.dl_choice.take() else {
            return;
        };
        self.dl.set_mode(choice.mode);
        for ep in &choice.episodes {
            self.episode_marks.remove(&ep.url);
            let podcast_name = self
                .settings
                .podcasts
                .get(self.podcast_cur.index)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Podcast".into());
            self.dl
                .add_for_podcast(ep.title.clone(), ep.url.clone(), podcast_name);
        }
        self.pump_downloads();
        let (active, queued) = (self.dl.active_count(), self.dl.queued_count());
        let mut text = format!(
            "{} episodio(s) {} · {active} activas · {queued} en cola",
            choice.episodes.len(),
            choice.mode.label()
        );
        if choice.skipped > 0 {
            text.push_str(&format!(
                " · {} omitidos (ya descargados o en curso)",
                choice.skipped
            ));
        }
        self.notify(text);
    }

    /// Cancela la entrada seleccionada en Ctrl+D. La entrada 0 es el canal completo;
    /// las restantes corresponden a trabajos individuales.
    fn cancel_download_at_selection(&mut self) {
        let index = self.dl_scroll;
        if self.bulk.is_some() && index == 0 {
            if let Some(bulk) = self.bulk.as_mut() {
                bulk.cancelled = true;
            }
            let ids: Vec<u64> = self.bulk_job_ids.iter().copied().collect();
            for id in ids {
                let active = self
                    .dl
                    .jobs()
                    .iter()
                    .any(|job| job.id == id && job.state == crate::downloads::JobState::Active);
                if active {
                    if let Some(token) = self.dl_cancellations.get(&id) {
                        token.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                } else {
                    self.dl.remove(id);
                    self.bulk_job_ids.remove(&id);
                    if let Some(bulk) = self.bulk.as_mut() {
                        bulk.done += 1;
                        bulk.cancelled_jobs += 1;
                    }
                }
            }
            if self.bulk_job_ids.is_empty() {
                if let Some(bulk) = self.bulk.take() {
                    self.notify(format!("Descarga del canal «{}» cancelada", bulk.name));
                    self.toast(format!("Descarga cancelada · «{}»", bulk.name), false);
                }
            } else {
                self.notify("Cancelación del canal solicitada; deteniendo trabajos activos…");
            }
            self.dl_scroll = self
                .dl_scroll
                .min(self.download_entry_count().saturating_sub(1));
            return;
        }
        let offset = usize::from(self.bulk.is_some());
        let Some(job) = self.dl.jobs().get(index.saturating_sub(offset)).cloned() else {
            self.notify("No hay una descarga seleccionada para cancelar");
            return;
        };
        if job.state == crate::downloads::JobState::Active {
            if let Some(token) = self.dl_cancellations.get(&job.id) {
                token.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            // No liberamos el hueco hasta que el hilo confirme que se detuvo.
        } else {
            self.dl.remove(job.id);
            if self.bulk_job_ids.remove(&job.id) {
                if let Some(bulk) = self.bulk.as_mut() {
                    bulk.done += 1;
                    bulk.cancelled_jobs += 1;
                }
                if self.bulk_job_ids.is_empty() {
                    if let Some(bulk) = self.bulk.take() {
                        let downloaded =
                            bulk.done.saturating_sub(bulk.failed + bulk.cancelled_jobs);
                        self.notify(format!(
                            "Canal «{}»: {} descargados, {} fallidos, {} cancelados",
                            bulk.name, downloaded, bulk.failed, bulk.cancelled_jobs
                        ));
                    }
                }
            }
            self.pump_downloads();
        }
        self.dl_scroll = self
            .dl_scroll
            .min(self.download_entry_count().saturating_sub(1));
        self.notify(format!("Cancelación solicitada · {}", job.title));
    }

    /// Lanza los hilos de las descargas en cola mientras haya huecos (máximo 8 activas).
    fn pump_downloads(&mut self) {
        let folder = config::data_dir().join("podcasts");
        while let Some(job) = self.dl.next_to_start() {
            let tx = self.bg_tx.clone();
            let folder = folder.clone();
            let id = job.id;
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.dl_cancellations.insert(id, cancel.clone());
            thread::spawn(move || {
                let progress_tx = tx.clone();
                let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
                let result = feeds::download_episode_cancellable_for_podcast(
                    &job.url,
                    &folder,
                    &job.podcast,
                    &job.title,
                    &cancel,
                    |got, total| {
                        if last.elapsed() >= std::time::Duration::from_millis(100) {
                            last = std::time::Instant::now();
                            let _ = progress_tx.send(Bg::DlBytes(id, got, total));
                        }
                    },
                );
                let _ = tx.send(Bg::Downloaded(id, job.title, result));
            });
        }
    }

    /// Añade todos los episodios pendientes del podcast al gestor unificado.
    /// Comparte el límite, modo, cola, progreso y cancelación de las descargas individuales.
    fn download_channel(&mut self) {
        if self.view != View::Podcasts || self.podcast_browser {
            self.notify("Abre un podcast en Mis podcasts (8) para descargar el canal");
            return;
        }
        if self.bulk.is_some() {
            self.dl_scroll = 0;
            self.cancel_download_at_selection();
            return;
        }
        let index = self.podcast_cur.index;
        let Some(feed) = self.settings.podcasts.get(index).cloned() else {
            self.notify("Selecciona un podcast");
            return;
        };
        let Some(episodes) = self.episodes.get(&index).cloned() else {
            self.notify("Espera a que carguen los episodios (u recarga el feed)");
            return;
        };
        let folder = config::data_dir().join("podcasts");
        let pending: Vec<Episode> =
            feeds::pending_downloads_for_podcast(&episodes, &folder, &feed.name)
                .into_iter()
                .filter(|episode| !self.dl.jobs().iter().any(|job| job.url == episode.url))
                .cloned()
                .collect();
        if pending.is_empty() {
            self.notify(format!(
                "Todos los episodios de «{}» ya están descargados",
                feed.name
            ));
            return;
        }
        let count = pending.len();
        let confirmed = matches!(self.bulk_confirm, Some((i, at))
            if i == index && at.elapsed() < std::time::Duration::from_secs(8));
        if !confirmed {
            self.bulk_confirm = Some((index, std::time::Instant::now()));
            self.notify(format!(
                "Pulsa D otra vez para añadir {} episodios de «{}» a la cola compartida",
                count, feed.name
            ));
            return;
        }
        self.bulk_confirm = None;
        let mode = self.dl.mode();
        for episode in pending {
            let id = self
                .dl
                .add_for_podcast(episode.title, episode.url, feed.name.clone());
            self.bulk_job_ids.insert(id);
        }
        self.bulk = Some(BulkDownload {
            name: feed.name.clone(),
            done: 0,
            total: count,
            failed: 0,
            current: String::new(),
            received: 0,
            size: None,
            cancelled: false,
            cancelled_jobs: 0,
        });
        self.notify(format!(
            "Canal «{}»: {} episodios añadidos {} · límite {} activo(s)",
            feed.name,
            count,
            mode.label(),
            if mode == DownloadMode::Parallel {
                self.dl.parallel_limit()
            } else {
                1
            }
        ));
        self.pump_downloads();
    }

    /// Copia el estado actual a los ajustes y los guarda.
    pub fn save_settings(&mut self) {
        let s = &self.snapshot;
        self.settings.volume = s.volume;
        self.settings.eq = s.eq.to_vec();
        self.settings.eq_enabled = s.eq_enabled;
        self.settings.replay_gain = s.replay_gain;
        self.settings.crossfade_secs = s.crossfade_secs;
        self.settings.shuffle = s.shuffle;
        self.settings.repeat = s.repeat.as_str().to_string();
        self.settings.parallel_downloads = self.dl.parallel_limit();
        self.settings.last_view = self.view.index();
        self.settings.last_focus = self.focus;
        if let Some(path) = self.settings_path.clone() {
            if let Err(e) = self.settings.save(&path) {
                eprintln!("MUZORYN: no se pudieron guardar los ajustes: {e}");
            }
        }
    }

    pub fn playing(&self) -> bool {
        self.snapshot.playback == Playback::Playing
    }
}

#[cfg(test)]
mod directory_path_tests {
    use super::*;

    #[test]
    fn same_directory_accepts_normalized_equivalent_paths() {
        assert!(same_directory(
            Path::new("/tmp/music/../music"),
            Path::new("/tmp/music")
        ));
        assert!(!same_directory(
            Path::new("/tmp/music"),
            Path::new("/tmp/music-old")
        ));
    }
}
