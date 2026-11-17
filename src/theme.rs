use std::{collections::BTreeMap, fs, path::Path};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

const BUILTIN: &str = include_str!("../themes/nocturne.toml");
const DAYLIGHT: &str = include_str!("../themes/daylight.toml");

/// Temas incluidos, en el orden en que los recorre la tecla `T`.
pub const THEMES: [(&str, &str); 12] = [
    ("nocturne", BUILTIN),
    ("daylight", DAYLIGHT),
    ("matrix", include_str!("../themes/matrix.toml")),
    ("cyberpunk", include_str!("../themes/cyberpunk.toml")),
    ("dracula", include_str!("../themes/dracula.toml")),
    ("nord", include_str!("../themes/nord.toml")),
    ("gruvbox", include_str!("../themes/gruvbox.toml")),
    ("tokyo-night", include_str!("../themes/tokyo-night.toml")),
    ("catppuccin", include_str!("../themes/catppuccin.toml")),
    ("synthwave", include_str!("../themes/synthwave.toml")),
    ("oceano", include_str!("../themes/oceano.toml")),
    ("atardecer", include_str!("../themes/atardecer.toml")),
];

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub base: Style,
    pub panel: Style,
    pub muted: Style,
    pub border: Style,
    pub focus: Style,
    pub cursor: Style,
    pub inactive_cursor: Style,
    pub playback: Style,
    pub error: Style,
    pub progress: Style,
    pub ok: Style,
    pub text: Color,
    pub bg: Color,
    pub surface: Color,
    pub accent: Color,
    pub accent2: Color,
    pub accent3: Color,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    name: String,
    #[serde(default)]
    palette: BTreeMap<String, String>,
    semantic: BTreeMap<String, String>,
}

impl Default for Theme {
    fn default() -> Self { Self::parse(BUILTIN).expect("invalid built-in theme") }
}

impl Theme {
    /// Nombre de tema incluido (`nocturne`, `daylight`) o ruta a un archivo.
    pub fn resolve(source: &str) -> Result<Self, String> {
        let key = source.trim().to_ascii_lowercase();
        if key.is_empty() { return Self::parse(BUILTIN); }
        match THEMES.iter().find(|(name, _)| *name == key) {
            Some((_, src)) => Self::parse(src),
            None => Self::load(Path::new(source)),
        }
    }

    /// Degradado de tres colores (cian, violeta, rosa) para t en 0..1.
    pub fn gradient(&self, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        if t < 0.5 { mix(self.accent, self.accent2, t * 2.0) } else { mix(self.accent2, self.accent3, (t - 0.5) * 2.0) }
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        Self::parse(&fs::read_to_string(path).map_err(|e| e.to_string())?)
    }

    pub fn parse(source: &str) -> Result<Self, String> {
        let doc: Document = toml::from_str(source).map_err(|e| e.to_string())?;
        if doc.schema_version != 1 { return Err("schema_version debe ser 1".into()); }
        if doc.name.trim().is_empty() || doc.name.chars().any(char::is_control) {
            return Err("Nombre de tema no válido".into());
        }
        // Validate unused entries too: no dormant invalid references.
        for name in doc.palette.keys() {
            resolve(&format!("${name}"), &doc, &mut Vec::new())?;
        }
        for name in doc.semantic.keys() {
            resolve(&format!("@{name}"), &doc, &mut Vec::new())?;
        }
        let color = |name: &str| resolve(&format!("@{name}"), &doc, &mut Vec::new());
        let bg = color("surface.base")?;
        let surface = color("surface.panel")?;
        let text = color("text.primary")?;
        let secondary = color("text.secondary")?;
        let accent = color("border.focused")?;
        let cursor_fg = color("selection.foreground")?;
        let cursor_bg = color("selection.background")?;
        if cursor_fg == cursor_bg { return Err("Selección invisible: fg y bg iguales".into()); }
        Ok(Self {
            name: doc.name.clone(),
            base: Style::default().fg(text).bg(bg),
            // Un solo tono de fondo: paneles y base coinciden, sin "papel" ni esquinas cuadradas.
            panel: Style::default().fg(text).bg(bg),
            muted: Style::default().fg(secondary),
            border: Style::default().fg(color("border.normal")?),
            focus: Style::default().fg(accent).add_modifier(Modifier::BOLD),
            cursor: Style::default().fg(cursor_fg).bg(cursor_bg)
                .add_modifier(Modifier::BOLD),
            inactive_cursor: Style::default().fg(text).bg(bg)
                .add_modifier(Modifier::UNDERLINED),
            playback: Style::default().fg(color("playback.active")?),
            error: Style::default().fg(color("status.error")?),
            progress: Style::default().fg(color("progress.elapsed")?).bg(bg),
            ok: Style::default().fg(color("status.ok")?),
            text,
            bg,
            surface,
            accent,
            accent2: color("accent.secondary")?,
            accent3: color("accent.tertiary")?,
        })
    }
}

pub fn rgb_of(color: Color) -> (u8, u8, u8) {
    match color { Color::Rgb(r, g, b) => (r, g, b), _ => (160, 170, 190) }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = rgb_of(a);
    let (br, bg, bb) = rgb_of(b);
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color::Rgb(f(ar, br), f(ag, bg), f(ab, bb))
}

fn resolve(value: &str, doc: &Document, stack: &mut Vec<String>) -> Result<Color, String> {
    if stack.len() >= 32 { return Err("Demasiados niveles de referencias".into()); }
    let reference = if let Some(key) = value.strip_prefix('$') {
        Some((&doc.palette, key))
    } else {
        value.strip_prefix('@').map(|key| (&doc.semantic, key))
    };
    if let Some((map, key)) = reference {
        if stack.iter().any(|entry| entry == value) {
            return Err(format!("Referencia circular: {value}"));
        }
        let target = map.get(key).ok_or_else(|| format!("Referencia ausente: {value}"))?;
        stack.push(value.into());
        let result = resolve(target, doc, stack);
        stack.pop();
        return result;
    }
    if value == "default" { return Ok(Color::Reset); }
    if let Some(index) = value.strip_prefix("ansi:") {
        return index.parse::<u8>().map(Color::Indexed).map_err(|e| e.to_string());
    }
    let hex = value.strip_prefix('#').ok_or_else(|| format!("Color no válido: {value}"))?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("Usa #RRGGBB: {value}"));
    }
    let rgb = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
    Ok(Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_semantic_reference() {
        let theme = Theme::parse(BUILTIN).unwrap();
        assert_eq!(theme.focus.fg, Some(Color::Rgb(0x27, 0xD7, 0xF2)));
    }
    #[test]
    fn rejects_cycles() {
        let bad = BUILTIN.replace("cyan = \"#27D7F2\"", "cyan = \"@border.focused\"");
        assert!(Theme::parse(&bad).unwrap_err().contains("circular"));
    }
    #[test]
    fn rejects_missing_token() {
        let bad = BUILTIN.replace("\"status.error\" = \"$red\"", "");
        assert!(Theme::parse(&bad).is_err());
    }
    #[test]
    fn bundled_themes_load() {
        for (name, _) in THEMES { Theme::resolve(name).unwrap(); }
        assert_eq!(Theme::resolve("daylight").unwrap().name, "Daylight");
        assert!(Theme::resolve("/no/existe.toml").is_err());
    }
    #[test]
    fn gradient_hits_endpoints() {
        let t = Theme::default();
        assert_eq!(t.gradient(0.0), t.accent);
        assert_eq!(t.gradient(1.0), t.accent3);
    }
}
