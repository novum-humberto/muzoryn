//! Hooks («plugins»): comandos de shell que se ejecutan ante eventos del reproductor.
//!
//! Como ejecutan código, se tratan con cuidado:
//! - **Confianza:** solo se activan si `settings.toml` es del usuario, no es escribible por
//!   grupo/otros y su directorio tampoco lo es. Si no, se desactivan y se guardan en un archivo
//!   de cuarentena (así no se pierden, y guardar la configuración no «blanquea» hooks ajenos).
//! - **Validación:** solo eventos conocidos, comandos acotados y un máximo de hooks.
//! - **Ejecución:** grupo de procesos propio, tiempo límite (se mata todo el grupo) y máximo de
//!   hooks simultáneos.
//! - **Edición:** `muzoryn hooks list|add|remove|restore`.
use crate::config::{Hook, Settings};
use std::{
    fs,
    os::unix::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

pub const EVENTS: [&str; 2] = ["track_changed", "playback_stopped"];
pub const MAX_HOOKS: usize = 16;
pub const MAX_COMMAND_LEN: usize = 1024;
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RUNNING: usize = 8;
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// Resultado de ejecutar un hook.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Finished(Option<i32>),
    TimedOut,
    NotStarted,
}

/// Comprueba que `settings.toml` merece confianza para ejecutar comandos.
pub fn check_trust(path: &Path) -> Result<(), String> {
    let real = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = fs::metadata(&real).map_err(|e| e.to_string())?;
    let euid = unsafe { libc::geteuid() };
    if meta.uid() != euid {
        return Err(format!("{} no pertenece a tu usuario", real.display()));
    }
    if meta.mode() & 0o022 != 0 {
        return Err(format!(
            "{} es escribible por grupo u otros (arréglalo con: chmod 600 {})",
            real.display(),
            real.display()
        ));
    }
    if let Some(dir) = real.parent() {
        let dm = fs::metadata(dir).map_err(|e| e.to_string())?;
        if dm.uid() != euid && dm.uid() != 0 {
            return Err(format!("{} pertenece a otro usuario", dir.display()));
        }
        if dm.mode() & 0o022 != 0 && dm.mode() & 0o1000 == 0 {
            return Err(format!(
                "{} es escribible por grupo u otros (arréglalo con: chmod 700 {})",
                dir.display(),
                dir.display()
            ));
        }
    }
    Ok(())
}

/// Valida un hook: evento conocido y comando razonable.
pub fn validate(hook: &Hook) -> Result<(), String> {
    if !EVENTS.contains(&hook.event.as_str()) {
        return Err(format!(
            "evento desconocido «{}» (válidos: {})",
            hook.event,
            EVENTS.join(", ")
        ));
    }
    let command = hook.command.trim();
    if command.is_empty() {
        return Err("comando vacío".into());
    }
    if command.len() > MAX_COMMAND_LEN {
        return Err(format!("comando de más de {MAX_COMMAND_LEN} caracteres"));
    }
    if command.contains('\0') {
        return Err("el comando contiene un byte nulo".into());
    }
    Ok(())
}

/// Ruta del archivo donde se guardan los hooks desactivados por falta de confianza.
pub fn quarantine_path(settings: &Path) -> PathBuf {
    let mut name = settings
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".hooks-en-cuarentena");
    settings.with_file_name(name)
}

/// Guarda (modo 0600) los hooks desactivados para que el usuario los revise.
pub fn quarantine(settings: &Path, hooks: &[Hook]) -> Result<PathBuf, String> {
    #[derive(serde::Serialize)]
    struct File<'a> {
        hooks: &'a [Hook],
    }
    let path = quarantine_path(settings);
    let text = toml::to_string_pretty(&File { hooks }).map_err(|e| e.to_string())?;
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Aplica confianza y validación a los hooks recién cargados de `settings`.
/// Devuelve el aviso a mostrar al usuario, si lo hay.
pub fn vet_loaded(settings: &mut Settings, path: &Path) -> Option<String> {
    if settings.hooks.is_empty() {
        return None;
    }
    if let Err(reason) = check_trust(path) {
        let hooks = std::mem::take(&mut settings.hooks);
        let saved = quarantine(path, &hooks)
            .map(|p| format!(" Copia en {}.", p.display()))
            .unwrap_or_default();
        return Some(format!(
            "{} hook(s) desactivados: {reason}.{saved}",
            hooks.len()
        ));
    }
    let mut notes = Vec::new();
    let mut kept = Vec::new();
    for hook in std::mem::take(&mut settings.hooks) {
        match validate(&hook) {
            Ok(()) if kept.len() < MAX_HOOKS => kept.push(hook),
            Ok(()) => notes.push(format!("más de {MAX_HOOKS} hooks; se ignora uno")),
            Err(e) => notes.push(format!("hook ignorado: {e}")),
        }
    }
    settings.hooks = kept;
    (!notes.is_empty()).then(|| notes.join("; "))
}

/// Ejecuta un comando con `sh -c`, en su propio grupo de procesos y con tiempo límite.
pub fn run_with_timeout(command: &str, env: &[(&str, &str)], timeout: Duration) -> Outcome {
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let Ok(mut child) = cmd.spawn() else {
        return Outcome::NotStarted;
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Outcome::Finished(status.code()),
            Ok(None) if start.elapsed() >= timeout => {
                // Mata todo el grupo (el shell y lo que haya lanzado).
                unsafe { libc::killpg(child.id() as i32, libc::SIGKILL) };
                let _ = child.wait();
                return Outcome::TimedOut;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return Outcome::NotStarted,
        }
    }
}

/// Lanza en segundo plano los hooks del evento (sin bloquear el hilo de audio).
pub fn fire(hooks: &[Hook], event: &str, title: &str, artist: &str, album: &str) {
    for hook in hooks.iter().filter(|h| h.event == event) {
        if validate(hook).is_err() {
            continue;
        }
        if RUNNING.fetch_add(1, Ordering::AcqRel) >= MAX_RUNNING {
            RUNNING.fetch_sub(1, Ordering::AcqRel);
            continue; // demasiados hooks a la vez: se omite este
        }
        let command = hook.command.clone();
        let vars = [
            ("MUZORYN_EVENT".to_string(), event.to_string()),
            ("MUZORYN_TITLE".to_string(), title.to_string()),
            ("MUZORYN_ARTIST".to_string(), artist.to_string()),
            ("MUZORYN_ALBUM".to_string(), album.to_string()),
        ];
        thread::spawn(move || {
            let env: Vec<(&str, &str)> =
                vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            run_with_timeout(&command, &env, HOOK_TIMEOUT);
            RUNNING.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

// ---- edición: `muzoryn hooks ...` ----

fn load_trusted(path: &Path) -> Result<Settings, String> {
    let settings = Settings::load(path)?;
    match &settings.hooks_notice {
        Some(notice) => Err(format!(
            "{notice}\nNo se edita nada hasta resolverlo; revisa el archivo y vuelve a intentarlo."
        )),
        None => Ok(settings),
    }
}

pub fn list(path: &Path) -> Result<String, String> {
    let settings = Settings::load(path)?;
    let mut out = format!("Configuración: {}\n", path.display());
    match &settings.hooks_notice {
        Some(n) => out.push_str(&format!("AVISO: {n}\n")),
        None => out.push_str("Estado: hooks activos (el archivo es de confianza)\n"),
    }
    if settings.hooks.is_empty() {
        out.push_str("Sin hooks activos.\n");
    }
    for (i, h) in settings.hooks.iter().enumerate() {
        out.push_str(&format!("{:>2}. [{}] {}\n", i + 1, h.event, h.command));
    }
    let q = quarantine_path(path);
    if q.exists() {
        out.push_str(&format!(
            "Hooks en cuarentena: {} (se recuperan con `muzoryn hooks restore`)\n",
            q.display()
        ));
    }
    Ok(out)
}

pub fn add(path: &Path, event: &str, command: &str) -> Result<String, String> {
    let mut settings = load_trusted(path)?;
    let hook = Hook {
        event: event.into(),
        command: command.trim().into(),
    };
    validate(&hook)?;
    if settings.hooks.len() >= MAX_HOOKS {
        return Err(format!("ya hay {MAX_HOOKS} hooks"));
    }
    settings.hooks.push(hook);
    settings.save(path)?;
    Ok(format!(
        "Hook añadido ({} en total). Se aplica al reiniciar el daemon (muzoryn ctl quit).",
        settings.hooks.len()
    ))
}

pub fn remove(path: &Path, number: usize) -> Result<String, String> {
    let mut settings = load_trusted(path)?;
    if number == 0 || number > settings.hooks.len() {
        return Err(format!(
            "no existe el hook {number} (hay {})",
            settings.hooks.len()
        ));
    }
    let gone = settings.hooks.remove(number - 1);
    settings.save(path)?;
    Ok(format!(
        "Hook eliminado: [{}] {}. Se aplica al reiniciar el daemon.",
        gone.event, gone.command
    ))
}

/// Recupera los hooks en cuarentena. Exige confianza en el archivo y confirmación explícita.
pub fn restore(path: &Path, confirmed: bool) -> Result<String, String> {
    let q = quarantine_path(path);
    let text = fs::read_to_string(&q).map_err(|_| "no hay hooks en cuarentena".to_string())?;
    #[derive(serde::Deserialize)]
    struct File {
        hooks: Vec<Hook>,
    }
    let file: File = toml::from_str(&text).map_err(|e| e.to_string())?;
    let mut settings = load_trusted(path)?;
    let mut listing = String::new();
    for h in &file.hooks {
        validate(h)?;
        listing.push_str(&format!("  [{}] {}\n", h.event, h.command));
    }
    if !confirmed {
        return Ok(format!(
            "Hooks en cuarentena (revísalos, ejecutan comandos con tus permisos):\n{listing}\
             Para activarlos: muzoryn hooks restore --yes"
        ));
    }
    if settings.hooks.len() + file.hooks.len() > MAX_HOOKS {
        return Err(format!("se superaría el máximo de {MAX_HOOKS} hooks"));
    }
    settings.hooks.extend(file.hooks);
    settings.save(path)?;
    let _ = fs::remove_file(&q);
    Ok(format!(
        "Hooks restaurados ({} en total).",
        settings.hooks.len()
    ))
}

pub const USAGE: &str = "Uso:
  muzoryn hooks [list]                  Muestra los hooks y si son de confianza
  muzoryn hooks add <evento> <comando>  Añade un hook (eventos: track_changed, playback_stopped)
  muzoryn hooks remove <número>         Elimina un hook
  muzoryn hooks restore [--yes]         Recupera los hooks desactivados por permisos inseguros";

/// Punto de entrada de `muzoryn hooks ...` (`args` va tras la palabra `hooks`).
pub fn run(path: &Path, args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        None | Some("list") => list(path),
        Some("add") => match (args.get(1), args.get(2)) {
            (Some(event), Some(_)) => add(path, event, &args[2..].join(" ")),
            _ => Err(USAGE.into()),
        },
        Some("remove") => match args.get(1).and_then(|n| n.parse().ok()) {
            Some(n) => remove(path, n),
            None => Err(USAGE.into()),
        },
        Some("restore") => restore(path, args.get(1).map(String::as_str) == Some("--yes")),
        _ => Err(USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muzoryn-hooks-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    fn write_settings(dir: &Path, mode: u32, hooks: &[(&str, &str)]) -> PathBuf {
        let path = dir.join("settings.toml");
        let mut text = String::new();
        for (event, command) in hooks {
            text.push_str(&format!(
                "[[hooks]]\nevent = \"{event}\"\ncommand = \"{command}\"\n"
            ));
        }
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn trusts_private_file_in_private_dir() {
        let dir = temp_dir("ok");
        let path = write_settings(&dir, 0o600, &[]);
        assert!(check_trust(&path).is_ok());
    }

    #[test]
    fn rejects_group_or_world_writable_file() {
        let dir = temp_dir("fw");
        for mode in [0o620, 0o602, 0o666] {
            let path = write_settings(&dir, mode, &[]);
            let e = check_trust(&path).unwrap_err();
            assert!(e.contains("chmod 600"), "{e}");
        }
    }

    #[test]
    fn rejects_writable_directory_unless_sticky() {
        let dir = temp_dir("dw");
        let path = write_settings(&dir, 0o600, &[]);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o770)).unwrap();
        assert!(check_trust(&path).unwrap_err().contains("chmod 700"));
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(check_trust(&path).is_ok());
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn symlink_to_unsafe_target_is_rejected() {
        let safe = temp_dir("sl-safe");
        let other = temp_dir("sl-other");
        let target = write_settings(&other, 0o666, &[]);
        let link = safe.join("settings.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(check_trust(&link).is_err());
    }

    #[test]
    fn validates_events_and_commands() {
        let h = |e: &str, c: &str| Hook {
            event: e.into(),
            command: c.into(),
        };
        assert!(validate(&h("track_changed", "notify-send hola")).is_ok());
        assert!(validate(&h("nada", "true")).is_err());
        assert!(validate(&h("track_changed", "   ")).is_err());
        assert!(validate(&h("track_changed", &"x".repeat(MAX_COMMAND_LEN + 1))).is_err());
        assert!(validate(&h("track_changed", "a\0b")).is_err());
    }

    #[test]
    fn untrusted_settings_quarantine_hooks_and_stay_untouched() {
        let dir = temp_dir("q");
        let path = write_settings(&dir, 0o666, &[("track_changed", "touch /tmp/x")]);
        let before = fs::read_to_string(&path).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert!(settings.hooks.is_empty());
        assert!(settings
            .hooks_notice
            .as_deref()
            .unwrap()
            .contains("desactivados"));
        let q = quarantine_path(&path);
        assert!(fs::read_to_string(&q).unwrap().contains("touch /tmp/x"));
        assert_eq!(
            fs::metadata(&q).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // La carga no reescribe el archivo ni le cambia los permisos.
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o666
        );
    }

    #[test]
    fn trusted_settings_drop_invalid_hooks_with_notice() {
        let dir = temp_dir("inv");
        let path = write_settings(
            &dir,
            0o600,
            &[("track_changed", "true"), ("evento_raro", "true")],
        );
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.hooks.len(), 1);
        assert!(settings.hooks_notice.unwrap().contains("evento_raro"));
    }

    #[test]
    fn hook_receives_environment_and_finishes() {
        let dir = temp_dir("env");
        let out = dir.join("out.txt");
        let cmd = format!(
            "printf '%s|%s' \"$MUZORYN_EVENT\" \"$MUZORYN_TITLE\" > {}",
            out.display()
        );
        let r = run_with_timeout(
            &cmd,
            &[
                ("MUZORYN_EVENT", "track_changed"),
                ("MUZORYN_TITLE", "$(touch pwn)"),
            ],
            Duration::from_secs(5),
        );
        assert_eq!(r, Outcome::Finished(Some(0)));
        // El título viaja como dato: no se interpreta como comando.
        assert_eq!(
            fs::read_to_string(&out).unwrap(),
            "track_changed|$(touch pwn)"
        );
        assert!(!Path::new("pwn").exists());
    }

    #[test]
    fn hanging_hook_is_killed_with_its_children() {
        let dir = temp_dir("to");
        let marker = dir.join("vivo");
        let cmd = format!("(sleep 3; touch {}) & sleep 30", marker.display());
        let start = Instant::now();
        let r = run_with_timeout(&cmd, &[], Duration::from_millis(400));
        assert_eq!(r, Outcome::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(3));
        thread::sleep(Duration::from_millis(3300));
        assert!(
            !marker.exists(),
            "el subproceso sobrevivió al tiempo límite"
        );
    }

    #[test]
    fn cli_add_list_remove_roundtrip() {
        let dir = temp_dir("cli");
        let path = write_settings(&dir, 0o600, &[]);
        add(&path, "track_changed", "echo hola").unwrap();
        assert!(add(&path, "inventado", "true").is_err());
        assert!(list(&path).unwrap().contains("echo hola"));
        assert!(remove(&path, 5).is_err());
        remove(&path, 1).unwrap();
        assert!(list(&path).unwrap().contains("Sin hooks activos"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn cli_refuses_to_edit_untrusted_file() {
        let dir = temp_dir("cli-bad");
        let path = write_settings(&dir, 0o666, &[("track_changed", "true")]);
        let e = add(&path, "track_changed", "echo nuevo").unwrap_err();
        assert!(e.contains("desactivados"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o666
        );
    }

    #[test]
    fn restore_requires_confirmation_and_trusted_file() {
        let dir = temp_dir("rest");
        let path = write_settings(&dir, 0o666, &[("playback_stopped", "echo adios")]);
        let loaded = Settings::load(&path).unwrap(); // crea la cuarentena
        assert!(
            restore(&path, true).is_err(),
            "con archivo inseguro no restaura"
        );
        // La app guarda su configuración (sin hooks, modo 0600): los hooks solo viven en cuarentena.
        loaded.save(&path).unwrap();
        let preview = restore(&path, false).unwrap();
        assert!(preview.contains("echo adios") && preview.contains("--yes"));
        assert!(list(&path).unwrap().contains("Sin hooks activos"));
        restore(&path, true).unwrap();
        assert!(list(&path).unwrap().contains("echo adios"));
        assert!(!quarantine_path(&path).exists());
    }
}
