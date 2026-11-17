//! Daemon e IPC por socket Unix: el audio sigue sonando al cerrar la interfaz.
//! Protocolo: una línea JSON por mensaje. Cliente → daemon: `Command`.
//! Daemon → cliente: `Snapshot` cada 100 ms.
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::Duration,
};
use crate::core::{AudioHandle, Command, LocalService, Playback, Snapshot};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct RemoteService {
    writer: Mutex<UnixStream>,
    latest: Arc<Mutex<Snapshot>>,
}

pub fn connect(path: &Path) -> Result<RemoteService, String> {
    let stream = UnixStream::connect(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let reader_stream = stream.try_clone().map_err(|e| e.to_string())?;
    let latest = Arc::new(Mutex::new(Snapshot::new(Vec::new())));
    let output = latest.clone();
    thread::spawn(move || {
        for line in BufReader::new(reader_stream).lines() {
            let Ok(line) = line else { break };
            if let Ok(snapshot) = serde_json::from_str::<Snapshot>(&line) {
                *lock(&output) = snapshot;
            }
        }
        lock(&output).error = Some("Se perdió la conexión con el daemon de MUZORYN".into());
    });
    Ok(RemoteService { writer: Mutex::new(stream), latest })
}

impl AudioHandle for RemoteService {
    fn send(&self, command: Command) -> Result<(), String> {
        let mut line = serde_json::to_string(&command).map_err(|e| e.to_string())?;
        line.push('\n');
        lock(&self.writer).write_all(line.as_bytes()).map_err(|e| format!("Daemon no disponible: {e}"))
    }
    fn snapshot(&self) -> Snapshot { lock(&self.latest).clone() }
    fn is_remote(&self) -> bool { true }
}

fn serve_client(service: Arc<LocalService>, stream: UnixStream, path: PathBuf) {
    let Ok(reader_stream) = stream.try_clone() else { return };
    let alive = Arc::new(AtomicBool::new(true));
    let alive_reader = alive.clone();
    let commands = service.clone();
    thread::spawn(move || {
        for line in BufReader::new(reader_stream).lines() {
            let Ok(line) = line else { break };
            if let Ok(command) = serde_json::from_str::<Command>(&line) {
                // Cerrar la interfaz no apaga el daemon; solo `Shutdown` explícito (W / ctl quit) lo hace.
                if matches!(command, Command::Shutdown) {
                    let _ = commands.send(Command::Stop);
                    thread::sleep(Duration::from_millis(150));
                    let _ = fs::remove_file(&path);
                    std::process::exit(0);
                }
                let _ = commands.send(command);
            }
        }
        alive_reader.store(false, Ordering::Relaxed);
    });
    let mut writer = stream;
    while alive.load(Ordering::Relaxed) {
        let Ok(mut line) = serde_json::to_string(&service.snapshot()) else { break };
        line.push('\n');
        if writer.write_all(line.as_bytes()).is_err() { break; }
        thread::sleep(Duration::from_millis(100));
    }
}

/// Arranca `muzoryn --daemon` en segundo plano (grupo de procesos propio) y espera al socket.
pub fn ensure_daemon(path: &Path) -> bool {
    use std::{os::unix::process::CommandExt, process::{Command as Proc, Stdio}};
    if UnixStream::connect(path).is_ok() { return true; }
    let Ok(exe) = std::env::current_exe() else { return false };
    let spawned = Proc::new(exe).arg("--daemon")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .process_group(0).spawn();
    if spawned.is_err() { return false; }
    for _ in 0..60 {
        if UnixStream::connect(path).is_ok() { return true; }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Bloquea atendiendo clientes. Devuelve error si ya hay un daemon activo.
pub fn serve(service: LocalService, path: &Path) -> Result<(), String> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() { return Err("Ya hay un daemon de MUZORYN activo".into()); }
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let service = Arc::new(service);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let service = service.clone();
        let socket = path.to_path_buf();
        thread::spawn(move || serve_client(service, stream, socket));
    }
    Ok(())
}

pub fn parse_ctl(word: &str) -> Option<Command> {
    Some(match word {
        "toggle" | "play-pause" | "pause" | "play" => Command::Toggle,
        "next" => Command::Next,
        "prev" | "previous" => Command::Previous,
        "stop" => Command::Stop,
        "vol+" | "volume-up" => Command::VolumeDelta(0.05),
        "vol-" | "volume-down" => Command::VolumeDelta(-0.05),
        "quit" | "shutdown" | "kill" => Command::Shutdown,
        "shuffle" => Command::ToggleShuffle,
        "repeat" => Command::CycleRepeat,
        "forward" => Command::SeekBy(10),
        "back" => Command::SeekBy(-10),
        _ => return None,
    })
}

/// `muzoryn ctl <orden>`: envía una orden al daemon o muestra su estado.
pub fn run_ctl(path: &Path, word: &str) -> Result<String, String> {
    if word == "status" {
        let stream = UnixStream::connect(path).map_err(|e| format!("No hay daemon activo ({e})"))?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
        let s: Snapshot = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let state = match s.playback { Playback::Playing => "▶", Playback::Paused => "❚❚", Playback::Stopped => "■" };
        return Ok(match s.current_item() {
            Some(i) => format!("{state} {} — {} [{}s]", i.title, i.artist, s.position.as_secs()),
            None => format!("{state} sin pista"),
        });
    }
    let command = parse_ctl(word).ok_or_else(|| format!("Orden desconocida: {word}"))?;
    let remote = connect(path)?;
    remote.send(command)?;
    // Deja que el hilo de escritura vacíe el socket antes de cerrar.
    thread::sleep(Duration::from_millis(50));
    Ok("ok".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Settings, media::QueueItem};

    #[test]
    fn protocol_messages_roundtrip_as_json() {
        let command = Command::Replace { items: vec![QueueItem::from_path(Path::new("/m/a.flac"))], start: 0 };
        let json = serde_json::to_string(&command).unwrap();
        assert!(matches!(serde_json::from_str::<Command>(&json).unwrap(), Command::Replace { .. }));
        let snapshot = Snapshot::new(vec![QueueItem::stream("Radio", "Vivo", "https://x.test/s", true)]);
        let back: Snapshot = serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
        assert_eq!(back.queue.len(), 1);
        assert_eq!(back.spectrum.len(), snapshot.spectrum.len());
    }

    #[test]
    fn ctl_words_map_to_commands() {
        assert!(matches!(parse_ctl("toggle"), Some(Command::Toggle)));
        assert!(matches!(parse_ctl("vol-"), Some(Command::VolumeDelta(v)) if v < 0.0));
        assert!(parse_ctl("nada").is_none());
    }

    #[test]
    fn client_controls_daemon_over_socket() {
        let path = std::env::temp_dir().join(format!("muzoryn-test-{}.sock", std::process::id()));
        let _ = fs::remove_file(&path);
        let server_path = path.clone();
        thread::spawn(move || {
            let service = LocalService::spawn(Settings::default(), Vec::new());
            let _ = serve(service, &server_path);
        });
        let mut remote = None;
        for _ in 0..50 {
            if let Ok(r) = connect(&path) { remote = Some(r); break; }
            thread::sleep(Duration::from_millis(100));
        }
        let remote = remote.expect("el daemon no arrancó");
        remote.send(Command::VolumeDelta(-0.5)).unwrap();
        let mut ok = false;
        for _ in 0..50 {
            if (remote.snapshot().volume - 0.2).abs() < 0.01 { ok = true; break; }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = fs::remove_file(&path);
        assert!(ok, "el volumen no cambió");
    }
}
