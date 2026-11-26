//! Importar/exportar listas: M3U, M3U8, TOML, XSPF (XML) y HTML.
use crate::library::{parse_m3u, Track};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const FORMATS: [&str; 6] = ["m3u", "m3u8", "pls", "toml", "xspf", "html"];

/// Normaliza lo que escribe el usuario: «.XSPF» → «xspf»; «htm» → «html».
pub fn format_of(text: &str) -> Option<&'static str> {
    let t = text.trim().trim_start_matches('.').to_lowercase();
    let t = if t == "htm" { "html".to_string() } else { t };
    FORMATS.iter().copied().find(|f| *f == t)
}

#[derive(Serialize, Deserialize, Default)]
struct TomlList {
    #[serde(default)]
    name: String,
    #[serde(default)]
    tracks: Vec<TomlTrack>,
}
#[derive(Serialize, Deserialize, Default)]
struct TomlTrack {
    path: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
    #[serde(default)]
    duration: u32,
}

pub fn render(tracks: &[&Track], name: &str, format: &str) -> Result<String, String> {
    Ok(match format {
        "m3u" | "m3u8" => {
            let mut s = String::from("#EXTM3U\n");
            for t in tracks {
                s.push_str(&format!(
                    "#EXTINF:{},{} - {}\n{}\n",
                    t.duration,
                    t.artist,
                    t.title,
                    t.path.display()
                ));
            }
            s
        }
        "pls" => {
            let safe_name = name.replace(['\n', '\r'], " ");
            let mut s = format!(
                "[playlist]\nNumberOfEntries={}\nPlaylistName={}\nVersion=2\n",
                tracks.len(),
                safe_name
            );
            for (i, t) in tracks.iter().enumerate() {
                s.push_str(&format!(
                    "File{}={}\nTitle{}={} - {}\nLength{}={}\n",
                    i + 1,
                    t.path.display(),
                    i + 1,
                    t.artist,
                    t.title,
                    i + 1,
                    t.duration
                ));
            }
            s
        }
        "toml" => toml::to_string(&TomlList {
            name: name.into(),
            tracks: tracks
                .iter()
                .map(|t| TomlTrack {
                    path: t.path.to_string_lossy().into_owned(),
                    title: t.title.clone(),
                    artist: t.artist.clone(),
                    album: t.album.clone(),
                    duration: t.duration,
                })
                .collect(),
        })
        .map_err(|e| e.to_string())?,
        "xspf" => {
            let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n  <title>{}</title>\n  <trackList>\n", esc(name));
            for t in tracks {
                s.push_str(&format!(
                    "    <track>\n      <location>{}</location>\n      <title>{}</title>\n      <creator>{}</creator>\n      <album>{}</album>\n      <duration>{}</duration>\n    </track>\n",
                    file_uri(&t.path), esc(&t.title), esc(&t.artist), esc(&t.album), t.duration as u64 * 1000));
            }
            s.push_str("  </trackList>\n</playlist>\n");
            s
        }
        "html" => {
            let mut s = format!("<!DOCTYPE html>\n<html lang=\"es\"><head><meta charset=\"utf-8\"><title>{n}</title>\n<style>body{{font-family:sans-serif;margin:2rem}}table{{border-collapse:collapse}}td,th{{padding:.3rem .8rem;border-bottom:1px solid #ccc;text-align:left}}</style></head>\n<body><h1>{n}</h1><p>{c} canciones · exportada por MUZORYN</p>\n<table><thead><tr><th>#</th><th>Título</th><th>Artista</th><th>Álbum</th><th>Duración</th></tr></thead><tbody>\n",
                n = esc(name), c = tracks.len());
            for (i, t) in tracks.iter().enumerate() {
                s.push_str(&format!("<tr data-path=\"{}\"><td>{}</td><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td><td>{}:{:02}</td></tr>\n",
                    esc(&t.path.to_string_lossy()), i + 1, esc(&file_uri(&t.path)), esc(&t.title),
                    esc(&t.artist), esc(&t.album), t.duration / 60, t.duration % 60));
            }
            s.push_str("</tbody></table></body></html>\n");
            s
        }
        other => {
            return Err(format!(
                "Formato no admitido: {other} (usa {})",
                FORMATS.join(", ")
            ))
        }
    })
}

pub fn export(tracks: &[&Track], name: &str, format: &str, target: &Path) -> Result<(), String> {
    let text = render(tracks, name, format)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(target, text).map_err(|e| e.to_string())
}

/// Lee una lista según su extensión. Devuelve (nombre si el formato lo guarda, rutas).
pub fn import(
    text: &str,
    ext: &str,
    base: &Path,
) -> Result<(Option<String>, Vec<PathBuf>), String> {
    let normalized = normalize_escaped_newlines(text);
    let text = normalized.as_str();
    let abs = |p: String| {
        let p = PathBuf::from(p);
        if p.is_absolute() {
            p
        } else {
            base.join(p)
        }
    };
    match ext.to_lowercase().as_str() {
        "m3u" | "m3u8" => Ok((None, parse_m3u(text.trim_start_matches('\u{feff}'), base))),
        "toml" => {
            let (name, paths) = toml_paths(text)?;
            Ok((
                name,
                paths.into_iter().map(|p| abs(expand_home(p))).collect(),
            ))
        }
        "pls" => {
            let (name, paths) = pls_paths(text);
            Ok((name, paths.into_iter().map(abs).collect()))
        }
        "xspf" => {
            let head = text.split("<trackList").next().unwrap_or("");
            let name = tag_values(head, "title")
                .into_iter()
                .next()
                .filter(|n| !n.is_empty());
            Ok((
                name,
                tag_values(text, "location")
                    .into_iter()
                    .map(|l| abs(from_uri(&l)))
                    .collect(),
            ))
        }
        "html" | "htm" => {
            let mut paths: Vec<String> = attr_values(text, "data-path");
            if paths.is_empty() {
                paths = attr_values(text, "href")
                    .into_iter()
                    .filter(|h| h.starts_with("file:"))
                    .map(|h| from_uri(&h))
                    .collect();
            }
            let name = tag_values(text, "title")
                .into_iter()
                .next()
                .filter(|n| !n.is_empty());
            Ok((name, paths.into_iter().map(abs).collect()))
        }
        other => Err(format!(
            "Extensión no admitida: «{other}» (usa {})",
            FORMATS.join(", ")
        )),
    }
}

/// Metadatos EXTINF de M3U en el mismo orden que sus rutas locales.
pub fn m3u_metadata(text: &str) -> Vec<(String, String, u32)> {
    let mut pending: Option<(u32, String)> = None;
    let mut out = Vec::new();
    for raw in normalize_escaped_newlines(text)
        .trim_start_matches('\u{feff}')
        .lines()
    {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let (duration, label) = rest.split_once(',').unwrap_or((rest, ""));
            let secs = duration
                .split(',')
                .next()
                .unwrap_or("")
                .parse::<i32>()
                .unwrap_or(0)
                .max(0) as u32;
            pending = Some((secs, label.trim().to_string()));
        } else if line.is_empty() || line.starts_with('#') {
            continue;
        } else if line.starts_with("http://") || line.starts_with("https://") {
            pending = None;
            continue;
        } else {
            let (duration, label) = pending.take().unwrap_or((0, String::new()));
            let (artist, title) = label
                .split_once(" - ")
                .map(|(a, t)| (a.trim(), t.trim()))
                .unwrap_or(("", label.trim()));
            out.push((title.to_string(), artist.to_string(), duration));
        }
    }
    out
}

/// Metadatos de listas TOML habituales, respetando el orden de `track(s)`.
pub fn toml_metadata(text: &str) -> Vec<(String, String, u32)> {
    let Ok(doc) = toml::from_str::<toml::Value>(
        normalize_escaped_newlines(text).trim_start_matches('\u{feff}'),
    ) else {
        return Vec::new();
    };
    let Some(root) = doc.as_table() else {
        return Vec::new();
    };
    for key in [
        "tracks",
        "track",
        "songs",
        "items",
        "pistas",
        "canciones",
        "playlist",
        "files",
    ] {
        let Some(items) = root.get(key).and_then(toml::Value::as_array) else {
            continue;
        };
        let mut out = Vec::new();
        for item in items {
            let Some(table) = item.as_table() else {
                continue;
            };
            let get = |keys: &[&str]| {
                keys.iter()
                    .find_map(|k| table.get(*k).and_then(toml::Value::as_str))
            };
            let title = get(&["title", "name", "titulo"]).unwrap_or("").to_string();
            let artist = get(&["artist", "creator", "artista"])
                .unwrap_or("")
                .to_string();
            let duration = ["duration", "duration_secs", "length"]
                .iter()
                .find_map(|k| table.get(*k).and_then(toml::Value::as_integer))
                .unwrap_or(0)
                .max(0) as u32;
            out.push((title, artist, duration));
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

fn normalize_escaped_newlines(text: &str) -> String {
    // Some exporters and shell-generated fixtures store literal \\n instead of line breaks.
    if text.contains("\\n") && !text.contains('\n') {
        text.replace("\\r\\n", "\n")
            .replace("\\n", "\n")
            .replace("\\r", "\n")
    } else {
        text.to_string()
    }
}

fn pls_paths(text: &str) -> (Option<String>, Vec<String>) {
    let mut indexed = std::collections::BTreeMap::<usize, String>::new();
    let mut playlist_name = None;
    for line in text.lines().map(str::trim) {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key.eq_ignore_ascii_case("PlaylistName") {
                let value = value.trim();
                if !value.is_empty() {
                    playlist_name = Some(value.to_string());
                }
            } else if let Some(n) = key
                .strip_prefix("File")
                .and_then(|v| v.parse::<usize>().ok())
            {
                indexed.insert(n, value.trim().to_string());
            }
        }
    }
    (playlist_name, indexed.into_values().collect())
}

// ---- TOML tolerante ----

fn expand_home(p: String) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join(rest).to_string_lossy().into_owned();
        }
    }
    if p.starts_with("file://") {
        return from_uri(&p);
    }
    p
}

/// Acepta `[[tracks]]` (formato propio) y variantes habituales: `track`, `songs`, `items`, `pistas`,
/// `canciones`, listas de cadenas, y las claves `path`, `file`, `location`, `ruta`, `archivo`, `uri`.
fn toml_paths(text: &str) -> Result<(Option<String>, Vec<String>), String> {
    let doc: toml::Value = toml::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("TOML no válido: {e}"))?;
    let root = doc
        .as_table()
        .ok_or("TOML no válido: se esperaba una tabla")?;
    let pick = |t: &toml::value::Table, keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| t.get(*k).and_then(|v| v.as_str()).map(str::to_string))
    };
    let name = pick(root, &["name", "title", "nombre", "titulo"]).filter(|n| !n.is_empty());
    let mut out = Vec::new();
    for key in [
        "tracks",
        "track",
        "songs",
        "items",
        "pistas",
        "canciones",
        "playlist",
        "files",
    ] {
        let Some(v) = root.get(key) else { continue };
        let Some(arr) = v.as_array() else { continue };
        for e in arr {
            match e {
                toml::Value::String(s) => out.push(s.clone()),
                toml::Value::Table(t) => {
                    if let Some(p) = pick(
                        t,
                        &[
                            "path", "file", "location", "ruta", "archivo", "uri", "filename",
                        ],
                    ) {
                        out.push(p);
                    }
                }
                _ => {}
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    Ok((name, out))
}

// ---- utilidades XML/HTML y URI ----

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn tag_values(text: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find(&open) {
        let after = &rest[a + open.len()..];
        let Some(b) = after.find(&close) else { break };
        out.push(unesc(after[..b].trim()));
        rest = &after[b + close.len()..];
    }
    out
}

fn attr_values(text: &str, attr: &str) -> Vec<String> {
    let key = format!("{attr}=\"");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find(&key) {
        let after = &rest[a + key.len()..];
        let Some(b) = after.find('"') else { break };
        out.push(unesc(&after[..b]));
        rest = &after[b..];
    }
    out
}

fn file_uri(path: &Path) -> String {
    let mut s = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

fn from_uri(uri: &str) -> String {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let raw = raw.strip_prefix("localhost").unwrap_or(raw);
    let bytes = raw.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(path: &str, title: &str) -> Track {
        Track {
            id: 1,
            path: PathBuf::from(path),
            title: title.into(),
            artist: "Ella & «Él»".into(),
            album: "A<B>".into(),
            genre: String::new(),
            duration: 185,
            track_no: 1,
            rg_db: None,
        }
    }

    #[test]
    fn m3u_extinf_metadata_is_preserved_in_order() {
        let metadata =
            m3u_metadata("#EXTM3U\\n#EXTINF:239,Miki Martz - Humano\\nArtist/Humano.mp3\\n");
        assert_eq!(
            metadata,
            vec![(String::from("Humano"), String::from("Miki Martz"), 239)]
        );
    }

    #[test]
    fn toml_track_metadata_is_preserved() {
        let metadata = toml_metadata("[[track]]\\npath = \"/music/a.mp3\"\\ntitle = \"Canción\"\\nartist = \"Artista\"\\nduration_secs = 210\\n");
        assert_eq!(
            metadata,
            vec![(String::from("Canción"), String::from("Artista"), 210)]
        );
    }

    #[test]
    fn every_format_roundtrips_awkward_paths() {
        let t1 = track(
            "/m/Jazz/Nina Simone/Sinnerman & más #1.flac",
            "Sinnerman \"live\"",
        );
        let t2 = track("/m/Ñandú/canción 100%.mp3", "Canción");
        for f in FORMATS {
            let text = render(&[&t1, &t2], "Mi lista <1>", f).unwrap();
            let (name, paths) = import(&text, f, Path::new("/base")).unwrap();
            assert_eq!(paths, vec![t1.path.clone(), t2.path.clone()], "formato {f}");
            if !f.starts_with("m3u") {
                assert_eq!(name.as_deref(), Some("Mi lista <1>"), "nombre en {f}");
            }
        }
    }

    #[test]
    fn extensions_are_recognized() {
        assert_eq!(format_of(".XSPF"), Some("xspf"));
        assert_eq!(format_of("htm"), Some("html"));
        assert_eq!(format_of("mp3"), None);
        assert!(import("x", "pdf", Path::new("/")).is_err());
    }

    #[test]
    fn xspf_from_other_players_and_relative_toml() {
        let x = "<playlist><title>VLC</title><trackList><track><location>file:///a/b%20c.mp3</location></track></trackList></playlist>";
        assert_eq!(
            import(x, "xspf", Path::new("/")).unwrap().1,
            vec![PathBuf::from("/a/b c.mp3")]
        );
        let t = "[[tracks]]\npath = \"rel/x.mp3\"\n";
        assert_eq!(
            import(t, "toml", Path::new("/base")).unwrap().1,
            vec![PathBuf::from("/base/rel/x.mp3")]
        );
    }

    #[test]
    fn every_listed_format_renders_and_is_recognised() {
        let t = track("/música/a.mp3", "Tema");
        for f in FORMATS {
            assert_eq!(format_of(f), Some(f));
            assert_eq!(format_of(&format!(".{}", f.to_uppercase())), Some(f));
            let out = render(&[&t], "Mi lista", f).unwrap_or_else(|e| panic!("{f}: {e}"));
            assert!(!out.is_empty(), "{f} vacío");
        }
        assert_eq!(format_of("htm"), Some("html"));
        assert!(format_of("exe").is_none());
    }
}
