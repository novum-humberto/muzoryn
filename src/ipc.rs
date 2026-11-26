//! Daemon e IPC por socket Unix: el audio sigue sonando al cerrar la interfaz.
//! Protocolo: una línea JSON por mensaje. Cliente → daemon: `Command`.
//! Daemon → cliente: `Snapshot` cada 100 ms al sonar y solo si cambia cuando está parado.
use crate::core::{AudioHandle, Command, LocalService, Playback, Snapshot};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::atomic::AtomicUsize,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::Duration,
};

const MAX_IPC_LINE: usize = 1024 * 1024;
const MAX_IPC_CLIENTS: usize = 16;
static IPC_CLIENTS: AtomicUsize = AtomicUsize::new(0);
/// Se activa con `Shutdown`: el bucle de `serve` termina y el daemon sale con normalidad.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Intervalo entre snapshots mientras suena algo (el espectro cambia continuamente).
const SNAPSHOT_ACTIVE: Duration = Duration::from_millis(100);
/// Intervalo de comprobación cuando no suena nada: solo se envía si el estado cambió.
const SNAPSHOT_IDLE: Duration = Duration::from_millis(250);

/// Decide qué enviar a un cliente y cuándo volver a mirar: evita reenviar un estado idéntico
/// (cola completa incluida) cada 100 ms cuando el reproductor está parado o en pausa.
#[derive(Default)]
pub struct SnapshotPacer {
    queue: Option<Vec<crate::media::QueueItem>>,
    last: Option<String>,
}

impl SnapshotPacer {
    /// Devuelve la línea JSON a enviar (si algo cambió) y la espera hasta la siguiente
    /// comprobación. La cola solo viaja cuando cambia; si no, se marca `queue_same`.
    pub fn next(&mut self, mut snapshot: Snapshot) -> Option<(Option<String>, Duration)> {
        let wait = if snapshot.playback == Playback::Playing {
            SNAPSHOT_ACTIVE
        } else {
            SNAPSHOT_IDLE
        };
        let queue_changed = self.queue.as_ref() != Some(&snapshot.queue);
        let full = if queue_changed {
            let line = serde_json::to_string(&snapshot).ok()?;
            self.queue = Some(snapshot.queue.clone());
            Some(line)
        } else {
            None
        };
        // La versión sin cola sirve de clave para detectar «nada cambió».
        snapshot.queue = Vec::new();
        snapshot.queue_same = true;
        let light = serde_json::to_string(&snapshot).ok()?;
        let unchanged = self.last.as_deref() == Some(light.as_str());
        self.last = Some(light.clone());
        if let Some(line) = full {
            return Some((Some(line), wait));
        }
        if unchanged {
            return Some((None, wait));
        }
        Some((Some(light), wait))
    }
}

/// Lee una línea JSON con límite duro para evitar que un cliente local agote memoria.
fn read_bounded_line<R: BufRead>(reader: &mut R, line: &mut Vec<u8>) -> std::io::Result<usize> {
    line.clear();
    let mut byte = [0u8; 1];
    while line.len() <= MAX_IPC_LINE {
        match reader.read(&mut byte)? {
            0 => return Ok(line.len()),
            _ if byte[0] == b'\n' => return Ok(line.len()),
            _ => line.push(byte[0]),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "mensaje IPC demasiado grande",
    ))
}

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
                let mut current = lock(&output);
                let previous_queue = std::mem::take(&mut current.queue);
                *current = snapshot;
                if current.queue_same {
                    current.queue = previous_queue;
                    current.queue_same = false;
                }
            }
        }
        lock(&output).error = Some("Se perdió la conexión con el daemon de MUZORYN".into());
    });
    Ok(RemoteService {
        writer: Mutex::new(stream),
        latest,
    })
}

impl AudioHandle for RemoteService {
    fn send(&self, command: Command) -> Result<(), String> {
        let mut line = serde_json::to_string(&command).map_err(|e| e.to_string())?;
        line.push('\n');
        lock(&self.writer)
            .write_all(line.as_bytes())
            .map_err(|e| format!("Daemon no disponible: {e}"))
    }
    fn snapshot(&self) -> Snapshot {
        lock(&self.latest).clone()
    }
    fn is_remote(&self) -> bool {
        true
    }
}

fn serve_client(service: Arc<LocalService>, stream: UnixStream, path: PathBuf) {
    let Ok(reader_stream) = stream.try_clone() else {
        return;
    };
    let alive = Arc::new(AtomicBool::new(true));
    let alive_reader = alive.clone();
    let commands = service.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(reader_stream);
        let mut bytes = Vec::with_capacity(4096);
        loop {
            if read_bounded_line(&mut reader, &mut bytes).is_err() {
                break;
            }
            if bytes.is_empty() {
                break;
            }
            let Ok(line) = std::str::from_utf8(&bytes) else {
                break;
            };
            if let Ok(command) = serde_json::from_str::<Command>(line) {
                // Cerrar la interfaz no apaga el daemon; solo `Shutdown` explícito (W / ctl quit) lo hace.
                if matches!(command, Command::Shutdown) {
                    // Cierre ordenado: se detiene el audio, se espera al hilo y se despierta
                    // el `accept` de `serve` para que devuelva el control a `main`.
                    SHUTDOWN.store(true, Ordering::SeqCst);
                    commands.shutdown_and_wait();
                    let _ = UnixStream::connect(&path);
                    break;
                }
                let _ = commands.send(command);
            }
        }
        alive_reader.store(false, Ordering::Relaxed);
    });
    let mut writer = stream;
    let mut pacer = SnapshotPacer::default();
    while alive.load(Ordering::Relaxed) {
        let Some((line, wait)) = pacer.next(service.snapshot()) else {
            break;
        };
        if let Some(mut line) = line {
            line.push('\n');
            if writer.write_all(line.as_bytes()).is_err() {
                break;
            }
        }
        thread::sleep(wait);
    }
}

/// Arranca `muzoryn --daemon` en segundo plano (grupo de procesos propio) y espera al socket.
pub fn ensure_daemon(path: &Path) -> bool {
    use std::{
        os::unix::process::CommandExt,
        process::{Command as Proc, Stdio},
    };
    if UnixStream::connect(path).is_ok() {
        return true;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let spawned = Proc::new(exe)
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    if spawned.is_err() {
        return false;
    }
    for _ in 0..60 {
        if UnixStream::connect(path).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Bloquea atendiendo clientes. Devuelve error si ya hay un daemon activo.
pub fn serve(service: LocalService, path: &Path) -> Result<(), String> {
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err("Ya hay un daemon de MUZORYN activo".into());
        }
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let listener = UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // El socket solo puede ser usado por el propietario (incluye rutas XDG y /tmp).
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("No se pudieron proteger los permisos del socket: {e}"))?;
    let service = Arc::new(service);
    #[cfg(feature = "mpris")]
    crate::mpris::spawn(service.clone());
    for stream in listener.incoming() {
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        let Ok(stream) = stream else { continue };
        if IPC_CLIENTS.fetch_add(1, Ordering::AcqRel) >= MAX_IPC_CLIENTS {
            IPC_CLIENTS.fetch_sub(1, Ordering::AcqRel);
            drop(stream);
            continue;
        }
        let service = service.clone();
        let socket = path.to_path_buf();
        thread::spawn(move || {
            serve_client(service, stream, socket);
            IPC_CLIENTS.fetch_sub(1, Ordering::AcqRel);
        });
    }
    let _ = fs::remove_file(path);
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
        let stream =
            UnixStream::connect(path).map_err(|e| format!("No hay daemon activo ({e})"))?;
        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        let s: Snapshot = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let state = match s.playback {
            Playback::Playing => "▶",
            Playback::Paused => "❚❚",
            Playback::Stopped => "■",
        };
        return Ok(match s.current_item() {
            Some(i) => format!(
                "{state} {} — {} [{}s]",
                i.title,
                i.artist,
                s.position.as_secs()
            ),
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
    fn ipc_rejects_oversized_command_lines() {
        let payload = vec![b'x'; MAX_IPC_LINE + 1];
        let mut reader = std::io::Cursor::new(payload);
        let mut line = Vec::new();
        assert!(read_bounded_line(&mut reader, &mut line).is_err());
    }

    #[test]
    fn ipc_accepts_a_normal_line() {
        let mut reader = std::io::Cursor::new(b"{}\n".to_vec());
        let mut line = Vec::new();
        assert_eq!(read_bounded_line(&mut reader, &mut line).unwrap(), 2);
        assert_eq!(line, b"{}");
    }

    #[test]
    fn protocol_messages_roundtrip_as_json() {
        let command = Command::Replace {
            items: vec![QueueItem::from_path(Path::new("/m/a.flac"))],
            start: 0,
        };
        let json = serde_json::to_string(&command).unwrap();
        assert!(matches!(
            serde_json::from_str::<Command>(&json).unwrap(),
            Command::Replace { .. }
        ));
        let snapshot = Snapshot::new(vec![QueueItem::stream(
            "Radio",
            "Vivo",
            "https://x.test/s",
            true,
        )]);
        let back: Snapshot =
            serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
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
    fn pacer_skips_identical_snapshots_when_idle() {
        let snapshot = Snapshot::new(vec![QueueItem::from_path(Path::new("/m/a.flac"))]);
        let mut pacer = SnapshotPacer::default();
        let (first, wait) = pacer.next(snapshot.clone()).unwrap();
        assert!(first.is_some(), "el primer estado siempre se envía");
        assert_eq!(wait, SNAPSHOT_IDLE);
        for _ in 0..20 {
            assert!(pacer.next(snapshot.clone()).unwrap().0.is_none());
        }
        let mut changed = snapshot.clone();
        changed.volume = 0.1;
        assert!(pacer.next(changed).unwrap().0.is_some());
    }

    #[test]
    fn pacer_uses_fast_cadence_only_while_playing() {
        let mut snapshot = Snapshot::new(Vec::new());
        snapshot.playback = Playback::Playing;
        let mut pacer = SnapshotPacer::default();
        assert_eq!(pacer.next(snapshot.clone()).unwrap().1, SNAPSHOT_ACTIVE);
        snapshot.playback = Playback::Paused;
        assert_eq!(pacer.next(snapshot.clone()).unwrap().1, SNAPSHOT_IDLE);
    }

    #[test]
    fn pacer_omits_unchanged_queue_while_playing() {
        let queue: Vec<QueueItem> = (0..500)
            .map(|i| QueueItem::from_path(Path::new(&format!("/m/{i}.flac"))))
            .collect();
        let mut snapshot = Snapshot::new(queue);
        snapshot.playback = Playback::Playing;
        let mut pacer = SnapshotPacer::default();
        let full = pacer.next(snapshot.clone()).unwrap().0.unwrap();
        snapshot.position = Duration::from_secs(1);
        let light = pacer.next(snapshot.clone()).unwrap().0.unwrap();
        assert!(
            light.len() * 10 < full.len(),
            "{} vs {}",
            light.len(),
            full.len()
        );
        let parsed: Snapshot = serde_json::from_str(&light).unwrap();
        assert!(parsed.queue_same && parsed.queue.is_empty());
        // Si la cola cambia, vuelve a viajar completa.
        snapshot.queue.pop();
        let again: Snapshot =
            serde_json::from_str(&pacer.next(snapshot).unwrap().0.unwrap()).unwrap();
        assert!(!again.queue_same);
        assert_eq!(again.queue.len(), 499);
    }

    /// Mide cuánto cuesta un snapshot con colas grandes (se ve con `--nocapture`).
    #[test]
    fn snapshot_cost_scales_with_queue_only_when_sent() {
        for n in [100usize, 1_000, 10_000] {
            let queue: Vec<QueueItem> = (0..n)
                .map(|i| QueueItem::from_path(Path::new(&format!("/musica/album/pista-{i}.flac"))))
                .collect();
            let snapshot = Snapshot::new(queue);
            let start = std::time::Instant::now();
            let line = serde_json::to_string(&snapshot).unwrap();
            let t = start.elapsed();
            eprintln!(
                "snapshot con {n} pistas: {} KiB, {:?} por serialización",
                line.len() / 1024,
                t
            );
            assert!(!line.is_empty());
        }
    }

    #[test]
    fn shutdown_closes_daemon_cleanly_without_exit() {
        let path = std::env::temp_dir().join(format!("muzoryn-shut-{}.sock", std::process::id()));
        let _ = fs::remove_file(&path);
        let server_path = path.clone();
        let server = thread::spawn(move || {
            let service = LocalService::spawn(Settings::default(), Vec::new());
            serve(service, &server_path)
        });
        let mut remote = None;
        for _ in 0..50 {
            if let Ok(r) = connect(&path) {
                remote = Some(r);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let remote = remote.expect("el daemon no arrancó");
        remote.send(Command::Shutdown).unwrap();
        for _ in 0..50 {
            if server.is_finished() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(server.is_finished(), "serve no terminó tras Shutdown");
        assert!(server.join().unwrap().is_ok());
        assert!(!path.exists(), "el socket debe borrarse");
        SHUTDOWN.store(false, Ordering::SeqCst);
    }

    #[test]
    fn client_keeps_queue_when_daemon_omits_it() {
        let path = std::env::temp_dir().join(format!("muzoryn-queue-{}.sock", std::process::id()));
        let _ = fs::remove_file(&path);
        let server_path = path.clone();
        thread::spawn(move || {
            let queue = (0..3)
                .map(|i| QueueItem::from_path(Path::new(&format!("/m/{i}.flac"))))
                .collect();
            let service = LocalService::spawn(Settings::default(), queue);
            let _ = serve(service, &server_path);
        });
        let mut remote = None;
        for _ in 0..50 {
            if let Ok(r) = connect(&path) {
                remote = Some(r);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let remote = remote.expect("el daemon no arrancó");
        // Un cambio de volumen fuerza un snapshot sin cola (queue_same).
        remote.send(Command::VolumeDelta(-0.1)).unwrap();
        let mut ok = false;
        for _ in 0..50 {
            let s = remote.snapshot();
            if (s.volume - 0.6).abs() < 0.01 && s.queue.len() == 3 {
                ok = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = fs::remove_file(&path);
        assert!(ok, "la cola se perdió o el volumen no cambió");
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
            if let Ok(r) = connect(&path) {
                remote = Some(r);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let remote = remote.expect("el daemon no arrancó");
        remote.send(Command::VolumeDelta(-0.5)).unwrap();
        let mut ok = false;
        for _ in 0..50 {
            if (remote.snapshot().volume - 0.2).abs() < 0.01 {
                ok = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = fs::remove_file(&path);
        assert!(ok, "el volumen no cambió");
    }
}
