use std::{
    env, io,
    path::PathBuf,
    time::{Duration, Instant},
};
use crossterm::event::{self, Event, KeyEventKind};
use muzoryn::{
    app::App,
    config::{self, Settings},
    core::{AudioHandle, Command, LocalService},
    keys::{Action, KeyMap},
    library::{self, Library},
    media::QueueItem,
    theme::Theme,
    ui,
};

const HELP: &str = "MUZORYN: reproductor musical en terminal

USO
  muzoryn [archivos o carpetas...]   Abre la interfaz (y reproduce lo indicado)
  muzoryn --daemon [rutas...]        Inicia el daemon de audio sin interfaz
                                    (la interfaz lo inicia sola si no hay uno activo)
  muzoryn ctl <orden>                Controla un daemon activo
                                    (toggle, next, prev, stop, vol+, vol-,
                                     shuffle, repeat, forward, back, quit, status)
  muzoryn --help | -h                Muestra esta ayuda breve
  muzoryn --man                      Muestra el manual completo
  muzoryn --version                  Muestra la versión instalada

TECLAS DE SALIDA
  w   cierra la interfaz; el daemon sigue reproduciendo (al volver, mismo panel)
  W   cierra la interfaz y detiene el daemon

VARIABLES
  MUZORYN_THEME  tema (nocturne, daylight, matrix, cyberpunk, dracula, nord, gruvbox, tokyo-night, catppuccin, synthwave, oceano, atardecer o ruta a un .toml)
  MUZORYN_KEYS   archivo de atajos
  MUZORYN_SOCKET ruta del socket del daemon
";

const MANUAL: &str = r#"MUZORYN · MANUAL COMPLETO

NAVEGACIÓN
1 Inicio · 2 Biblioteca · 3 Búsqueda · 4 Cola · 5 Álbumes/artistas · 6 Listas
7 Radio · 8 Podcasts · 9 Letras · 0 Audio/EQ · v Vista completa · , Ajustes
↑/↓ selecciona · Enter abre/reproduce · Tab cambia de panel · Esc vuelve.

REPRODUCCIÓN
Espacio pausa/reanuda · n/p siguiente/anterior · ←/→ busca ±5 s
=/+ sube volumen · - baja volumen · z aleatorio · o repetir
c crossfade · g ReplayGain · e preset EQ · t EQ on/off · f favorito.

BIBLIOTECA Y LISTAS
La biblioteca indexa las carpetas music_dirs de settings.toml.
Pulsa u para actualizar. Ctrl+N crea lista · F2 renombra · i importa
playlist · Ctrl+E exporta. Formatos: M3U, M3U8, PLS, XSPF, TOML y HTML.

RADIO Y PODCASTS
Radio: Enter reproduce · a añade URL · x elimina · f marca favorita ★.
Tab recorre Mis emisoras, Favoritas, Recientes y Explorar Radio Browser; / busca por nombre o con
country:, language: y genre:; Enter escucha y añade el resultado.
Muestra la canción en vivo si la emisora la envía, el estado (Conectando, En vivo, Reconectando,
No disponible) y reconecta sola si el flujo se corta.
Podcasts: Enter muestra episodios · a añade feed RSS · d descarga un episodio · D descarga el canal completo
(pide confirmar; D otra vez durante la descarga la cancela) · u actualiza.
/ busca podcasts en español (directorio de Apple Podcasts) y Tab (desde Episodios) abre Populares,
Categorías y vuelve a Mis podcasts. Enter en un podcast lo suscribe y abre sus episodios; f suscribe o
cancela; Esc vuelve atrás. La tienda se elige con podcast_country en settings.toml (por defecto mx).
Letras: usa .lrc junto a la canción; [ y ] ajustan el desfase.

TEMAS Y AYUDA
Ajustes → Temas: ↑/↓ selecciona tema y Enter lo aplica directamente.
T avanza al siguiente tema. ? abre la ayuda original de atajos.
H abre el manual integrado. Esc cierra las ventanas flotantes.

COMANDOS
muzoryn [rutas...]       Abre la app y reproduce archivos/carpetas indicados.
muzoryn --help | -h      Ayuda breve.
muzoryn --man            Este manual completo.
muzoryn --version        Versión instalada.
muzoryn --daemon         Inicia el daemon sin interfaz.
muzoryn ctl <orden>      toggle, next, prev, stop, vol+, vol-, shuffle,
                         repeat, forward, back, quit, status.
w cierra la interfaz y conserva el daemon; W cierra también el daemon.

CONFIGURACIÓN
MUZORYN_THEME selecciona el tema; MUZORYN_KEYS define un archivo de atajos;
MUZORYN_SOCKET define la ruta del socket del daemon.
"#;

fn config_error(error: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn initial_queue(paths: &[PathBuf]) -> Vec<QueueItem> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() { files.extend(library::collect_audio_files(&[path.clone()])); }
        else { files.push(path.clone()); }
    }
    files.iter().map(|p| QueueItem::from_path(p)).collect()
}

fn load_settings() -> io::Result<Settings> {
    let path = config::settings_path();
    let settings = Settings::load(&path).map_err(config_error)?;
    if !path.exists() { let _ = settings.save(&path); }
    Ok(settings)
}

#[cfg(unix)]
fn run_daemon(paths: Vec<PathBuf>) -> io::Result<()> {
    let settings = load_settings()?;
    let service = LocalService::spawn(settings, initial_queue(&paths));
    let socket = config::socket_path();
    eprintln!("MUZORYN daemon escuchando en {}", socket.display());
    muzoryn::ipc::serve(service, &socket).map_err(config_error)
}

fn run(paths: Vec<PathBuf>) -> io::Result<()> {
    let settings = load_settings()?;
    let theme_source = env::var("MUZORYN_THEME").unwrap_or_else(|_| settings.theme.clone());
    // La configuración inválida se informa antes de entrar en modo raw.
    let theme = Theme::resolve(&theme_source).map_err(config_error)?;
    let keys_path = env::var_os("MUZORYN_KEYS").map(PathBuf::from)
        .unwrap_or_else(|| config::config_dir().join("keys.toml"));
    let keys = if keys_path.exists() || env::var_os("MUZORYN_KEYS").is_some() {
        KeyMap::load(&keys_path).map_err(config_error)?
    } else { KeyMap::default() };
    let db_path = config::database_path();
    let library = Library::open(&db_path).map_err(config_error)?;

    let queue = initial_queue(&paths);
    let has_paths = !queue.is_empty();
    // El daemon arranca solo con la app; si no se puede, se usa audio integrado.
    #[cfg(unix)]
    let _ = muzoryn::ipc::ensure_daemon(&config::socket_path());
    #[cfg(unix)]
    let remote: Option<Box<dyn AudioHandle>> = muzoryn::ipc::connect(&config::socket_path())
        .ok().map(|r| Box::new(r) as Box<dyn AudioHandle>);
    #[cfg(not(unix))]
    let remote: Option<Box<dyn AudioHandle>> = None;
    let is_remote = remote.is_some();
    let handle: Box<dyn AudioHandle> = match remote {
        Some(handle) => {
            if has_paths { let _ = handle.send(Command::Replace { items: queue, start: 0 }); }
            handle
        }
        None => {
            let local = LocalService::spawn(settings.clone(), queue);
            if has_paths { let _ = local.send(Command::PlayIndex(0)); }
            Box::new(local)
        }
    };

    let mut app = App::new(settings, library, Some(db_path), theme_source, theme, is_remote);
    app.keys_help = keys.describe();
    app.settings_path = Some(config::settings_path());
    if is_remote && !has_paths { app.restore_view(); }
    if app.tracks.is_empty() { app.action(Action::Scan); }

    let mut terminal = ratatui::init();
    let result = (|| -> io::Result<()> {
        let interval = Duration::from_millis(66);
        let mut last_update = Instant::now();
        let mut dirty = true;
        let mut osc_bg: Option<ratatui::style::Color> = None;
        let osc_on = std::env::var_os("MUZORYN_NO_OSC").is_none();
        while !app.quit {
            if last_update.elapsed() >= interval {
                app.update(handle.snapshot());
                last_update = Instant::now();
                dirty = true;
            }
            if dirty {
                // Sincroniza el fondo por defecto del terminal (márgenes/padding) con el tema.
                if osc_on && osc_bg != Some(app.theme.bg) {
                    if let ratatui::style::Color::Rgb(r, g, b) = app.theme.bg {
                        use std::io::Write;
                        let mut o = io::stdout();
                        let _ = write!(o, "\x1b]11;#{r:02x}{g:02x}{b:02x}\x1b\\");
                        let _ = o.flush();
                    }
                    osc_bg = Some(app.theme.bg);
                }
                muzoryn::gfx::begin_frame();
                terminal.draw(|frame| ui::render(frame, &mut app))?;
                dirty = false;
                if muzoryn::gfx::flush(&mut io::stdout())? { terminal.clear()?; dirty = true; }
            }
            let timeout = interval.saturating_sub(last_update.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if app.help {
                            if let Some(action) = keys.action(key) { app.action(action); }
                            dirty = true;
                            continue;
                        }
                        let commands = if app.manual {
                            app.handle_manual_key(key); Vec::new()
                        } else if app.input_active() {
                            app.handle_text(key)
                        } else if let Some(action) = keys.action(key) {
                            app.action(action)
                        } else { Vec::new() };
                        for command in commands {
                            if let Err(error) = handle.send(command) { app.notify(error); }
                        }
                        dirty = true;
                    }
                    Event::Resize(_, _) => { muzoryn::gfx::invalidate(); dirty = true; }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    let _ = muzoryn::gfx::cleanup(&mut io::stdout());
    if std::env::var_os("MUZORYN_NO_OSC").is_none() {
        use std::io::Write;
        let _ = write!(io::stdout(), "\x1b]111\x1b\\");
        let _ = io::stdout().flush();
    }
    ratatui::restore();
    // `W` apaga también el daemon y la próxima vez se abre en Inicio; `w` conserva el panel.
    if app.quit_all {
        app.view = muzoryn::app::View::Inicio;
        app.focus = 0;
    }
    app.save_settings();
    if app.quit_all { let _ = handle.send(Command::Shutdown); std::thread::sleep(Duration::from_millis(250)); }
    // Se restaura la terminal antes de esperar al hilo de audio.
    drop(handle);
    result
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => { print!("{HELP}"); Ok(()) }
        Some("--man") => { print!("{MANUAL}"); Ok(()) }
        Some("--version") | Some("-V") => { println!("muzoryn {}", env!("CARGO_PKG_VERSION")); Ok(()) }
        #[cfg(unix)]
        Some("--daemon") => run_daemon(args[1..].iter().map(PathBuf::from).collect()),
        #[cfg(unix)]
        Some("ctl") => match args.get(1) {
            Some(word) => muzoryn::ipc::run_ctl(&config::socket_path(), word)
                .map(|text| println!("{text}")).map_err(config_error),
            None => Err(config_error("Uso: muzoryn ctl <orden>".into())),
        },
        #[cfg(not(unix))]
        Some("--daemon") | Some("ctl") => Err(config_error("El daemon requiere Linux o macOS".into())),
        _ => run(args.iter().map(PathBuf::from).collect()),
    };
    if let Err(error) = outcome {
        eprintln!("MUZORYN: {error}");
        std::process::exit(1);
    }
}
