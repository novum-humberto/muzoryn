use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    #[serde(alias = "quit")]
    Detach,
    QuitAll,
    Up,
    Down,
    SeekForward,
    SeekBackward,
    FocusNext,
    Play,
    Toggle,
    Stop,
    Next,
    Previous,
    Enqueue,
    VolumeUp,
    VolumeDown,
    ReloadTheme,
    CycleViz,
    CycleTheme,
    Help,
    Search,
    Cancel,
    Delete,
    Favorite,
    Shuffle,
    Repeat,
    Crossfade,
    ReplayGain,
    EqPreset,
    EqToggle,
    Scan,
    LyricsLater,
    LyricsEarlier,
    Download,
    DownloadAll,
    NewList,
    RenameList,
    ImportM3u,
    ExportM3u,
    AddToList,
    MoveUp,
    MoveDown,
    ClearQueue,
    AddSource,
    BrowsePodcasts,
    ManageMusicFolders,
    Manual,
    Mark,
    ClearMarks,
    Downloads,
    #[serde(rename = "view_1")]
    View1,
    #[serde(rename = "view_2")]
    View2,
    #[serde(rename = "view_3")]
    View3,
    #[serde(rename = "view_4")]
    View4,
    #[serde(rename = "view_5")]
    View5,
    #[serde(rename = "view_6")]
    View6,
    #[serde(rename = "view_7")]
    View7,
    #[serde(rename = "view_8")]
    View8,
    #[serde(rename = "view_9")]
    View9,
    #[serde(rename = "view_10")]
    View10,
    #[serde(rename = "view_11")]
    View11,
    #[serde(rename = "view_12")]
    View12,
}

impl Action {
    /// Índice de vista (0..12) si la acción cambia de sección.
    pub fn view_index(self) -> Option<usize> {
        use Action::*;
        Some(match self {
            View1 => 0,
            View2 => 1,
            View3 => 2,
            View4 => 3,
            View5 => 4,
            View6 => 5,
            View7 => 6,
            View8 => 7,
            View9 => 8,
            View10 => 9,
            View11 => 10,
            View12 => 11,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        use Action::*;
        match self {
            Detach => "Cerrar la interfaz (el daemon sigue sonando)",
            QuitAll => "Salir y detener el daemon",
            Up => "Subir",
            Down => "Bajar",
            SeekForward => "Adelantar / derecha",
            SeekBackward => "Retroceder / izquierda",
            FocusNext => "Cambiar panel / pestaña",
            Play => "Reproducir / abrir",
            Toggle => "Pausa / reanudar",
            Stop => "Detener",
            Next => "Siguiente pista",
            Previous => "Pista anterior",
            Enqueue => "Añadir a la cola",
            VolumeUp => "Subir volumen",
            VolumeDown => "Bajar volumen",
            ReloadTheme => "Recargar tema",
            CycleViz => "Cambiar estilo del visualizador",
            CycleTheme => "Siguiente tema de color",
            Help => "Ayuda · atajos",
            Manual => "Manual completo",
            Search => "Buscar",
            Cancel => "Cancelar / cerrar",
            Delete => "Quitar / eliminar",
            Favorite => "Favorito",
            Shuffle => "Aleatorio",
            Repeat => "Repetir (no/todo/una)",
            Crossfade => "Crossfade (0/3/6/9 s)",
            ReplayGain => "ReplayGain",
            EqPreset => "Preset del ecualizador",
            EqToggle => "Ecualizador on/off",
            Scan => "Actualizar biblioteca / feed seleccionado / populares",
            LyricsLater => "Letras: +0.5 s",
            LyricsEarlier => "Letras: -0.5 s",
            Download => "Descargar episodio",
            DownloadAll => "Descargar canal completo",
            NewList => "Nueva lista",
            RenameList => "Renombrar lista",
            ImportM3u => "Importar lista",
            ExportM3u => "Exportar lista",
            AddToList => "Añadir (marcadas o actual) a la lista elegida",
            Mark => "Marcar / desmarcar (pistas para una lista, episodios para descargar)",
            ClearMarks => "Quitar todas las marcas",
            Downloads => "Ventana de descargas",
            MoveUp => "Subir en la cola",
            MoveDown => "Bajar en la cola",
            ClearQueue => "Vaciar la cola",
            AddSource => "Añadir emisora / podcast (URL)",
            BrowsePodcasts => "Explorar directorio de podcasts",
            ManageMusicFolders => "Añadir o quitar carpetas de música",
            View1 => "Ir a Inicio",
            View2 => "Ir a Biblioteca",
            View3 => "Ir a Búsqueda",
            View4 => "Ir a Cola",
            View5 => "Ir a Álbumes y artistas",
            View6 => "Ir a Listas",
            View7 => "Ir a Radio",
            View8 => "Ir a Podcasts",
            View9 => "Ir a Letras",
            View10 => "Ir a Audio / EQ",
            View11 => "Ir a Vista completa",
            View12 => "Ir a Ajustes",
        }
    }
}

#[derive(Clone)]
pub struct KeyMap(HashMap<(KeyCode, KeyModifiers), Action>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    keys: HashMap<Action, Vec<String>>,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::parse(include_str!("../config/keys.toml")).expect("invalid bundled keys")
    }
}

fn chord_name(code: KeyCode, modifiers: KeyModifiers) -> String {
    let mut out = String::new();
    if modifiers.contains(KeyModifiers::CONTROL) {
        out.push_str("Ctrl+");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        out.push_str("Alt+");
    }
    out.push_str(&match code {
        KeyCode::Char(' ') => "Espacio".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Up => "↑".into(),
        KeyCode::Down => "↓".into(),
        KeyCode::Left => "←".into(),
        KeyCode::Right => "→".into(),
        KeyCode::Delete => "Supr".into(),
        KeyCode::Backspace => "Retroceso".into(),
        KeyCode::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    });
    out
}

impl KeyMap {
    // Custom bindings replace all bindings for each mentioned action.
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::parse(&fs::read_to_string(path).map_err(|e| e.to_string())?)
    }
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut base: Config =
            toml::from_str(include_str!("../config/keys.toml")).map_err(|e| e.to_string())?;
        let custom: Config = toml::from_str(source).map_err(|e| e.to_string())?;
        base.keys.extend(custom.keys);
        let mut map = HashMap::new();
        for (action, chords) in base.keys {
            for chord in chords {
                let key = parse_chord(&chord)?;
                if let Some(other) = map.insert(key, action) {
                    return Err(format!("Atajo duplicado {chord}: {other:?}/{action:?}"));
                }
            }
        }
        Ok(Self(map))
    }

    /// Líneas «teclas  acción» agrupadas por acción y ordenadas por etiqueta.
    pub fn describe(&self) -> String {
        let mut by_action: HashMap<Action, Vec<String>> = HashMap::new();
        for ((code, modifiers), action) in &self.0 {
            by_action
                .entry(*action)
                .or_default()
                .push(chord_name(*code, *modifiers));
        }
        let mut rows: Vec<(String, String)> = by_action
            .into_iter()
            .map(|(action, mut keys)| {
                keys.sort();
                (action.label().to_string(), keys.join(" / "))
            })
            .collect();
        rows.sort();
        rows.into_iter()
            .map(|(label, keys)| format!("{keys:<18}{label}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn action(&self, key: KeyEvent) -> Option<Action> {
        if key.kind != KeyEventKind::Press {
            return None;
        }
        // Algunos terminales marcan SHIFT en símbolos como «+» o «?»: se ignora para teclas de texto.
        let mut modifiers = key.modifiers;
        if matches!(key.code, KeyCode::Char(_)) && modifiers.contains(KeyModifiers::SHIFT) {
            modifiers ^= KeyModifiers::SHIFT;
        }
        self.0.get(&(key.code, modifiers)).copied()
    }
}

fn parse_chord(chord: &str) -> Result<(KeyCode, KeyModifiers), String> {
    // Una tecla «+» sola no se puede separar por «+».
    if chord == "+" {
        return Ok((KeyCode::Char('+'), KeyModifiers::NONE));
    }
    let parts: Vec<_> = chord.split('+').collect();
    let mut modifiers = KeyModifiers::NONE;
    for part in &parts[..parts.len().saturating_sub(1)] {
        modifiers |= match *part {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            _ => return Err(format!("Modificador no admitido: {part}")),
        };
    }
    let code = match parts.last().copied().unwrap_or("") {
        "space" => KeyCode::Char(' '),
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "esc" => KeyCode::Esc,
        "delete" => KeyCode::Delete,
        "backspace" => KeyCode::Backspace,
        value
            if value.len() >= 2
                && value.starts_with('f')
                && value[1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            let n: u8 = value[1..]
                .parse()
                .map_err(|_| format!("Tecla no admitida: {value}"))?;
            if !(1..=12).contains(&n) {
                return Err(format!("Tecla no admitida: {value}"));
            }
            KeyCode::F(n)
        }
        value => {
            let mut chars = value.chars();
            let c = chars.next().ok_or("Atajo vacío")?;
            if chars.next().is_some() {
                return Err(format!("Tecla no admitida: {value}"));
            }
            KeyCode::Char(c)
        }
    };
    Ok((code, modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_binding_replaces_action() {
        let map = KeyMap::parse("[keys]\ntoggle = ['ctrl+p']").unwrap();
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            Some(Action::Toggle)
        );
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            None
        );
    }
    #[test]
    fn w_detaches_and_shift_w_quits_all() {
        let map = KeyMap::default();
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE)),
            Some(Action::Detach)
        );
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char('W'), KeyModifiers::SHIFT)),
            Some(Action::QuitAll)
        );
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)),
            Some(Action::ExportM3u)
        );
    }
    #[test]
    fn conflicts_are_errors() {
        assert!(KeyMap::parse("[keys]\ntoggle = ['w']").is_err());
    }
    #[test]
    fn default_map_has_every_view_and_function_keys() {
        let map = KeyMap::default();
        for (key, view) in [('1', 0), ('9', 8), ('0', 9), ('v', 10), (',', 11)] {
            let a = map
                .action(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
                .unwrap();
            assert_eq!(a.view_index(), Some(view));
        }
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)),
            Some(Action::Help)
        );
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE)),
            Some(Action::VolumeUp)
        );
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
            Some(Action::MoveUp)
        );
        assert!(map.describe().contains("Pausa"));
        assert_eq!(
            map.action(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE)),
            Some(Action::Scan)
        );
    }
}
