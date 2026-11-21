//! Biblioteca SQLite: escaneo incremental, favoritos, listas y M3U.
use crate::media::{MediaSource, QueueItem};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::{Accessor, ItemKey},
};
use rusqlite::{params, Connection};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

pub const AUDIO_EXTENSIONS: [&str; 10] = [
    "mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "mp4", "aiff",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration: u32,
    pub track_no: u32,
    pub rg_db: Option<f32>,
}

impl Track {
    pub fn to_item(&self) -> QueueItem {
        QueueItem {
            media: MediaSource::File(self.path.clone()),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration: if self.duration > 0 {
                Some(Duration::from_secs(self.duration as u64))
            } else {
                None
            },
            rg_db: self.rg_db,
            live: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScanStats {
    pub seen: usize,
    pub added: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub failed: usize,
}

struct Meta {
    title: String,
    artist: String,
    album: String,
    genre: String,
    duration: u32,
    track_no: u32,
    rg_db: Option<f32>,
}

pub struct Library {
    conn: Connection,
}

fn normalized_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn path_is_within(candidate: &Path, root: &Path) -> bool {
    normalized_path(candidate).starts_with(normalized_path(root))
}

fn db<T>(result: rusqlite::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

impl Library {
    pub fn open(path: &Path) -> Result<Library, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = db(Connection::open(path))?;
        Library::init(conn)
    }

    pub fn open_memory() -> Result<Library, String> {
        Library::init(db(Connection::open_in_memory())?)
    }

    fn init(conn: Connection) -> Result<Library, String> {
        db(conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS tracks (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                album TEXT NOT NULL,
                genre TEXT NOT NULL DEFAULT '',
                duration INTEGER NOT NULL DEFAULT 0,
                track_no INTEGER NOT NULL DEFAULT 0,
                mtime INTEGER NOT NULL DEFAULT 0,
                rg REAL
             );
             CREATE TABLE IF NOT EXISTS favorites (track_id INTEGER PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS playlists (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
             );
             CREATE TABLE IF NOT EXISTS playlist_items (
                playlist_id INTEGER NOT NULL,
                pos INTEGER NOT NULL,
                track_id INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_items ON playlist_items(playlist_id, pos);",
        ))?;
        Ok(Library { conn })
    }

    pub fn all_tracks(&self) -> Result<Vec<Track>, String> {
        let mut stmt = db(self.conn.prepare(
            "SELECT id, path, title, artist, album, genre, duration, track_no, rg
             FROM tracks
             ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, track_no, title COLLATE NOCASE",
        ))?;
        let rows = db(stmt.query_map([], |r| {
            let path: String = r.get(1)?;
            let rg: Option<f64> = r.get(8)?;
            Ok(Track {
                id: r.get(0)?,
                path: PathBuf::from(path),
                title: r.get(2)?,
                artist: r.get(3)?,
                album: r.get(4)?,
                genre: r.get(5)?,
                duration: r.get::<_, i64>(6)?.max(0) as u32,
                track_no: r.get::<_, i64>(7)?.max(0) as u32,
                rg_db: rg.map(|v| v as f32),
            })
        }))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(db(row)?);
        }
        Ok(out)
    }

    fn upsert(&self, path: &Path, mtime: i64, meta: &Meta) -> Result<(), String> {
        db(self.conn.execute(
            "INSERT INTO tracks (path, title, artist, album, genre, duration, track_no, mtime, rg)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
                title = excluded.title, artist = excluded.artist, album = excluded.album,
                genre = excluded.genre, duration = excluded.duration,
                track_no = excluded.track_no, mtime = excluded.mtime, rg = excluded.rg",
            params![
                path.to_string_lossy(),
                meta.title,
                meta.artist,
                meta.album,
                meta.genre,
                meta.duration as i64,
                meta.track_no as i64,
                mtime,
                meta.rg_db.map(|v| v as f64)
            ],
        ))?;
        Ok(())
    }

    fn stored_mtime(&self, path: &Path) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT mtime FROM tracks WHERE path = ?1",
                params![path.to_string_lossy()],
                |r| r.get::<_, i64>(0),
            )
            .ok()
    }

    /// Inserta o actualiza una pista sin leer etiquetas (usado por pruebas).
    pub fn insert_simple(
        &self,
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        genre: &str,
        duration: u32,
    ) -> Result<i64, String> {
        let meta = Meta {
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            genre: genre.into(),
            duration,
            track_no: 0,
            rg_db: None,
        };
        self.upsert(Path::new(path), 0, &meta)?;
        db(self.conn.query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            params![path],
            |r| r.get(0),
        ))
    }

    /// Elimina pistas ausentes, salvo las pertenecientes a raíces que no devolvieron
    /// archivos en este escaneo. Esto conserva la biblioteca cuando una memoria USB
    /// o un disco externo está desconectado o montado todavía vacío.
    /// Retira de la biblioteca las pistas ubicadas dentro de una raíz, sin borrar
    /// los archivos del disco. También limpia favoritos y referencias de listas.
    pub fn remove_tracks_under_root(&self, root: &Path) -> Result<usize, String> {
        let mut stmt = db(self.conn.prepare("SELECT id, path FROM tracks"))?;
        let rows = db(stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))))?;
        let mut ids = Vec::new();
        for row in rows {
            let (id, path) = db(row)?;
            if path_is_within(Path::new(&path), root) {
                ids.push(id);
            }
        }
        drop(stmt);
        for id in &ids {
            db(self.conn.execute(
                "DELETE FROM playlist_items WHERE track_id = ?1",
                params![id],
            ))?;
            db(self
                .conn
                .execute("DELETE FROM favorites WHERE track_id = ?1", params![id]))?;
            db(self
                .conn
                .execute("DELETE FROM tracks WHERE id = ?1", params![id]))?;
        }
        Ok(ids.len())
    }

    pub fn prune_missing_except(&self, protected_roots: &[PathBuf]) -> Result<usize, String> {
        let mut stmt = db(self.conn.prepare("SELECT id, path FROM tracks"))?;
        let rows = db(stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))))?;
        let mut gone = Vec::new();
        for row in rows {
            let (id, path) = db(row)?;
            let file = Path::new(&path);
            let protected = protected_roots.iter().any(|root| file.starts_with(root));
            let referenced_by_playlist = self
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM playlist_items WHERE track_id = ?1)",
                    params![id],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false);
            if !protected && !referenced_by_playlist && !file.exists() {
                gone.push(id);
            }
        }
        drop(stmt);
        for id in &gone {
            db(self
                .conn
                .execute("DELETE FROM tracks WHERE id = ?1", params![id]))?;
            db(self
                .conn
                .execute("DELETE FROM favorites WHERE track_id = ?1", params![id]))?;
            db(self.conn.execute(
                "DELETE FROM playlist_items WHERE track_id = ?1",
                params![id],
            ))?;
        }
        Ok(gone.len())
    }

    /// Elimina todas las pistas ausentes (útil para mantenimiento explícito y pruebas).
    pub fn prune_missing(&self) -> Result<usize, String> {
        self.prune_missing_except(&[])
    }

    // ---- favoritos ----
    pub fn favorites(&self) -> Result<HashSet<i64>, String> {
        let mut stmt = db(self.conn.prepare("SELECT track_id FROM favorites"))?;
        let rows = db(stmt.query_map([], |r| r.get::<_, i64>(0)))?;
        let mut set = HashSet::new();
        for row in rows {
            set.insert(db(row)?);
        }
        Ok(set)
    }

    pub fn set_favorite(&self, track_id: i64, on: bool) -> Result<(), String> {
        if on {
            db(self.conn.execute(
                "INSERT OR IGNORE INTO favorites (track_id) VALUES (?1)",
                params![track_id],
            ))?;
        } else {
            db(self.conn.execute(
                "DELETE FROM favorites WHERE track_id = ?1",
                params![track_id],
            ))?;
        }
        Ok(())
    }

    // ---- listas ----
    pub fn playlists(&self) -> Result<Vec<(i64, String)>, String> {
        let mut stmt = db(self
            .conn
            .prepare("SELECT id, name FROM playlists ORDER BY name COLLATE NOCASE"))?;
        let rows = db(stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(db(row)?);
        }
        Ok(out)
    }

    pub fn create_playlist(&self, name: &str) -> Result<i64, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("El nombre de la lista no puede estar vacío".into());
        }
        db(self
            .conn
            .execute("INSERT INTO playlists (name) VALUES (?1)", params![name]))
        .map_err(|_| format!("Ya existe una lista llamada «{name}»"))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("El nombre de la lista no puede estar vacío".into());
        }
        db(self.conn.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            params![name, id],
        ))
        .map_err(|_| format!("Ya existe una lista llamada «{name}»"))?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<(), String> {
        db(self.conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1",
            params![id],
        ))?;
        db(self
            .conn
            .execute("DELETE FROM playlists WHERE id = ?1", params![id]))?;
        Ok(())
    }

    pub fn playlist_track_ids(&self, id: i64) -> Result<Vec<i64>, String> {
        let mut stmt = db(self
            .conn
            .prepare("SELECT track_id FROM playlist_items WHERE playlist_id = ?1 ORDER BY pos"))?;
        let rows = db(stmt.query_map(params![id], |r| r.get::<_, i64>(0)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(db(row)?);
        }
        Ok(out)
    }

    pub fn add_to_playlist(&self, id: i64, track_id: i64) -> Result<(), String> {
        let next: i64 = db(self.conn.query_row(
            "SELECT COALESCE(MAX(pos) + 1, 0) FROM playlist_items WHERE playlist_id = ?1",
            params![id],
            |r| r.get(0),
        ))?;
        db(self.conn.execute(
            "INSERT INTO playlist_items (playlist_id, pos, track_id) VALUES (?1, ?2, ?3)",
            params![id, next, track_id],
        ))?;
        Ok(())
    }

    pub fn remove_from_playlist(&self, id: i64, track_id: i64) -> Result<(), String> {
        db(self.conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1 AND track_id = ?2",
            params![id, track_id],
        ))?;
        Ok(())
    }

    pub fn track_id_by_path(&self, path: &Path) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT id FROM tracks WHERE path = ?1",
                params![path.to_string_lossy()],
                |r| r.get(0),
            )
            .ok()
    }

    /// Como `track_id_by_path`, pero tolera rutas movidas: prueba la ruta canónica y,
    /// si no, el nombre de archivo (sólo si es único en la biblioteca).
    pub fn track_id_lenient(&self, path: &Path) -> Option<i64> {
        if let Some(id) = self.track_id_by_path(path) {
            return Some(id);
        }
        if let Ok(c) = path.canonicalize() {
            if let Some(id) = self.track_id_by_path(&c) {
                return Some(id);
            }
        }
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let mut stmt = self.conn.prepare("SELECT id, path FROM tracks").ok()?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .ok()?;
        let mut found = None;
        for (id, p) in rows.flatten() {
            let same = Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase() == name)
                .unwrap_or(false);
            if same {
                if found.is_some() {
                    return None;
                }
                found = Some(id);
            }
        }
        found
    }
}

// ---- escaneo ----

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Recorre carpetas sin seguir enlaces simbólicos.
pub fn collect_audio_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            if is_audio(&dir) && dir.is_file() {
                files.push(dir);
            }
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() && is_audio(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn parse_gain(text: &str) -> Option<f32> {
    let cleaned = text
        .trim()
        .trim_end_matches("dB")
        .trim_end_matches("db")
        .trim();
    cleaned
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= 40.0)
}

fn read_meta(path: &Path) -> Meta {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut meta = Meta {
        title: stem,
        artist: "Artista desconocido".into(),
        album: "Álbum desconocido".into(),
        genre: String::new(),
        duration: 0,
        track_no: 0,
        rg_db: None,
    };
    if let Ok(tagged) = lofty::read_from_path(path) {
        meta.duration = tagged.properties().duration().as_secs() as u32;
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(v) = tag.title() {
                if !v.trim().is_empty() {
                    meta.title = v.trim().to_string();
                }
            }
            if let Some(v) = tag.artist() {
                if !v.trim().is_empty() {
                    meta.artist = v.trim().to_string();
                }
            }
            if let Some(v) = tag.album() {
                if !v.trim().is_empty() {
                    meta.album = v.trim().to_string();
                }
            }
            if let Some(v) = tag.genre() {
                meta.genre = v.trim().to_string();
            }
            meta.track_no = tag.track().unwrap_or(0);
            meta.rg_db = tag
                .get_string(&ItemKey::ReplayGainTrackGain)
                .and_then(parse_gain);
        }
    }
    meta
}

/// Escaneo incremental: solo relee los archivos cuya fecha de modificación cambió.
pub fn scan(
    library: &Library,
    roots: &[PathBuf],
    mut progress: impl FnMut(usize),
) -> Result<ScanStats, String> {
    let files = collect_audio_files(roots);
    let mut stats = ScanStats::default();
    db(library.conn.execute_batch("BEGIN"))?;
    for path in &files {
        stats.seen += 1;
        let mtime = fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(1);
        if library.stored_mtime(path) == Some(mtime) {
            stats.unchanged += 1;
        } else {
            let meta = read_meta(path);
            match library.upsert(path, mtime, &meta) {
                Ok(()) => stats.added += 1,
                Err(_) => stats.failed += 1,
            }
        }
        if stats.seen % 25 == 0 {
            progress(stats.seen);
        }
    }
    db(library.conn.execute_batch("COMMIT"))?;
    let empty_roots: Vec<PathBuf> = roots
        .iter()
        .filter(|root| !files.iter().any(|file| file.starts_with(root)))
        .cloned()
        .collect();
    stats.removed = library.prune_missing_except(&empty_roots)?;
    progress(stats.seen);
    Ok(stats)
}

// ---- M3U ----

pub fn export_m3u(tracks: &[&Track], target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut text = String::from("#EXTM3U\n");
    for t in tracks {
        text.push_str(&format!(
            "#EXTINF:{},{} - {}\n{}\n",
            t.duration,
            t.artist,
            t.title,
            t.path.display()
        ));
    }
    fs::write(target, text).map_err(|e| e.to_string())
}

/// Devuelve las rutas de un M3U/M3U8; las relativas se resuelven contra `base`.
pub fn parse_m3u(text: &str, base: &Path) -> Vec<PathBuf> {
    let normalized = if text.contains("\\n") && !text.contains('\n') {
        text.replace("\\r\\n", "\n")
            .replace("\\n", "\n")
            .replace("\\r", "\n")
    } else {
        text.to_string()
    };
    normalized
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        // Las listas de radio pueden contener URLs HTTP; no son archivos locales.
        .filter(|l| !l.starts_with("http://") && !l.starts_with("https://"))
        .map(|line| {
            let decoded = decode_m3u_path(line);
            let local = decoded.strip_prefix("file://").unwrap_or(&decoded);
            let local = local.strip_prefix("localhost").unwrap_or(local);
            let p = PathBuf::from(local);
            if p.is_absolute() {
                p
            } else {
                base.join(p)
            }
        })
        .collect()
}

/// Decodifica escapes porcentuales UTF-8 usados por exportadores M3U (espacios,
/// acentos, paréntesis, etc.). Los '+' se conservan porque en rutas no son espacios.
fn decode_m3u_path(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_within_normalizes_equivalent_roots() {
        assert!(path_is_within(
            Path::new("/tmp/music/album/song.mp3"),
            Path::new("/tmp/music/../music")
        ));
        assert!(!path_is_within(
            Path::new("/tmp/music-old/song.mp3"),
            Path::new("/tmp/music")
        ));
    }

    #[test]
    fn favorites_and_playlists() {
        let lib = Library::open_memory().unwrap();
        let a = lib
            .insert_simple("/m/a.flac", "A", "X", "Alb", "Rock", 100)
            .unwrap();
        let b = lib
            .insert_simple("/m/b.flac", "B", "X", "Alb", "Rock", 200)
            .unwrap();
        lib.set_favorite(a, true).unwrap();
        assert!(lib.favorites().unwrap().contains(&a));
        lib.set_favorite(a, false).unwrap();
        assert!(lib.favorites().unwrap().is_empty());
        let list = lib.create_playlist("Viajes").unwrap();
        assert!(lib.create_playlist("Viajes").is_err());
        lib.add_to_playlist(list, b).unwrap();
        lib.add_to_playlist(list, a).unwrap();
        assert_eq!(lib.playlist_track_ids(list).unwrap(), vec![b, a]);
        lib.remove_from_playlist(list, b).unwrap();
        assert_eq!(lib.playlist_track_ids(list).unwrap(), vec![a]);
        lib.rename_playlist(list, "Relax").unwrap();
        assert_eq!(lib.playlists().unwrap()[0].1, "Relax");
        lib.delete_playlist(list).unwrap();
        assert!(lib.playlists().unwrap().is_empty());
    }

    #[test]
    fn removing_library_root_removes_only_its_indexed_tracks_not_files() {
        let lib = Library::open_memory().unwrap();
        let inside = lib
            .insert_simple("/music/album/song.mp3", "Song", "Artist", "Album", "", 10)
            .unwrap();
        let outside = lib
            .insert_simple("/music-old/song.mp3", "Other", "Artist", "Album", "", 10)
            .unwrap();
        lib.set_favorite(inside, true).unwrap();
        let list = lib.create_playlist("Test").unwrap();
        lib.add_to_playlist(list, inside).unwrap();
        lib.add_to_playlist(list, outside).unwrap();

        assert_eq!(
            lib.remove_tracks_under_root(Path::new("/music")).unwrap(),
            1
        );
        let remaining = lib.all_tracks().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, outside);
        assert!(lib.favorites().unwrap().is_empty());
        assert_eq!(lib.playlist_track_ids(list).unwrap(), vec![outside]);
    }

    #[test]
    fn tracks_are_sorted_and_pruned() {
        let lib = Library::open_memory().unwrap();
        lib.insert_simple("/no/existe/z.mp3", "Z", "B", "Alb", "", 1)
            .unwrap();
        lib.insert_simple("/no/existe/a.mp3", "A", "A", "Alb", "", 1)
            .unwrap();
        let tracks = lib.all_tracks().unwrap();
        assert_eq!(tracks[0].artist, "A");
        assert_eq!(lib.prune_missing().unwrap(), 2);
        assert!(lib.all_tracks().unwrap().is_empty());
    }

    #[test]
    fn scan_preserves_tracks_under_empty_or_disconnected_library_roots() {
        let lib = Library::open_memory().unwrap();
        let root = std::env::temp_dir().join("muzoryn-empty-external-library");
        fs::create_dir_all(&root).unwrap();
        let missing = root.join("album/track.flac");
        lib.insert_simple(missing.to_str().unwrap(), "Track", "Artist", "Album", "", 1)
            .unwrap();
        let stats = scan(&lib, std::slice::from_ref(&root), |_| {}).unwrap();
        assert_eq!(stats.removed, 0);
        assert_eq!(lib.all_tracks().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn m3u_roundtrip_paths() {
        let t = Track {
            id: 1,
            path: "/m/a.flac".into(),
            title: "A".into(),
            artist: "X".into(),
            album: "Y".into(),
            genre: String::new(),
            duration: 5,
            track_no: 1,
            rg_db: None,
        };
        let dir = std::env::temp_dir().join("muzoryn-test-m3u");
        let file = dir.join("l.m3u");
        export_m3u(&[&t], &file).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        let paths = parse_m3u(&text, Path::new("/base"));
        assert_eq!(paths, vec![PathBuf::from("/m/a.flac")]);
        assert_eq!(
            parse_m3u("rel/b.mp3\n#c", Path::new("/base")),
            vec![PathBuf::from("/base/rel/b.mp3")]
        );
    }

    #[test]
    fn m3u_decodes_percent_encoded_utf8_and_ignores_remote_streams() {
        let parsed = parse_m3u(
            "#EXTM3U\\n#EXTINF:120,Demo\\nArtist%20Name/Canci%C3%B3n%20%281%29.mp3\\nhttps://radio.example/live\\n",
            Path::new("/lists"),
        );
        assert_eq!(
            parsed,
            vec![PathBuf::from("/lists/Artist Name/Canción (1).mp3")]
        );
    }

    #[test]
    fn playlist_references_survive_scan_when_audio_file_is_missing() {
        let lib = Library::open_memory().unwrap();
        let list = lib.create_playlist("Importada").unwrap();
        let id = lib
            .insert_simple("/no-existe/tema.mp3", "Tema", "Artista", "", "", 0)
            .unwrap();
        lib.add_to_playlist(list, id).unwrap();
        assert_eq!(lib.prune_missing_except(&[]).unwrap(), 0);
        assert_eq!(lib.playlist_track_ids(list).unwrap(), vec![id]);
    }

    #[test]
    fn gain_parsing() {
        assert_eq!(parse_gain("-6.50 dB"), Some(-6.5));
        assert_eq!(parse_gain("abc"), None);
    }
}
