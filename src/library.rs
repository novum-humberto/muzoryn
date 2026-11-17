//! Biblioteca SQLite: escaneo incremental, favoritos, listas y M3U.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    tag::{Accessor, ItemKey},
};
use rusqlite::{params, Connection};
use crate::media::{MediaSource, QueueItem};

pub const AUDIO_EXTENSIONS: [&str; 10] =
    ["mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "mp4", "aiff"];

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
            duration: if self.duration > 0 { Some(Duration::from_secs(self.duration as u64)) } else { None },
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
        for row in rows { out.push(db(row)?); }
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
                path.to_string_lossy(), meta.title, meta.artist, meta.album, meta.genre,
                meta.duration as i64, meta.track_no as i64, mtime, meta.rg_db.map(|v| v as f64)
            ],
        ))?;
        Ok(())
    }

    fn stored_mtime(&self, path: &Path) -> Option<i64> {
        self.conn.query_row(
            "SELECT mtime FROM tracks WHERE path = ?1",
            params![path.to_string_lossy()],
            |r| r.get::<_, i64>(0),
        ).ok()
    }

    /// Inserta o actualiza una pista sin leer etiquetas (usado por pruebas).
    pub fn insert_simple(&self, path: &str, title: &str, artist: &str, album: &str,
                         genre: &str, duration: u32) -> Result<i64, String> {
        let meta = Meta {
            title: title.into(), artist: artist.into(), album: album.into(),
            genre: genre.into(), duration, track_no: 0, rg_db: None,
        };
        self.upsert(Path::new(path), 0, &meta)?;
        db(self.conn.query_row("SELECT id FROM tracks WHERE path = ?1", params![path], |r| r.get(0)))
    }

    /// Elimina las pistas cuyos archivos ya no existen.
    pub fn prune_missing(&self) -> Result<usize, String> {
        let mut stmt = db(self.conn.prepare("SELECT id, path FROM tracks"))?;
        let rows = db(stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))))?;
        let mut gone = Vec::new();
        for row in rows {
            let (id, path) = db(row)?;
            if !Path::new(&path).exists() { gone.push(id); }
        }
        drop(stmt);
        for id in &gone {
            db(self.conn.execute("DELETE FROM tracks WHERE id = ?1", params![id]))?;
            db(self.conn.execute("DELETE FROM favorites WHERE track_id = ?1", params![id]))?;
            db(self.conn.execute("DELETE FROM playlist_items WHERE track_id = ?1", params![id]))?;
        }
        Ok(gone.len())
    }

    // ---- favoritos ----
    pub fn favorites(&self) -> Result<HashSet<i64>, String> {
        let mut stmt = db(self.conn.prepare("SELECT track_id FROM favorites"))?;
        let rows = db(stmt.query_map([], |r| r.get::<_, i64>(0)))?;
        let mut set = HashSet::new();
        for row in rows { set.insert(db(row)?); }
        Ok(set)
    }

    pub fn set_favorite(&self, track_id: i64, on: bool) -> Result<(), String> {
        if on {
            db(self.conn.execute("INSERT OR IGNORE INTO favorites (track_id) VALUES (?1)", params![track_id]))?;
        } else {
            db(self.conn.execute("DELETE FROM favorites WHERE track_id = ?1", params![track_id]))?;
        }
        Ok(())
    }

    // ---- listas ----
    pub fn playlists(&self) -> Result<Vec<(i64, String)>, String> {
        let mut stmt = db(self.conn.prepare("SELECT id, name FROM playlists ORDER BY name COLLATE NOCASE"))?;
        let rows = db(stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))))?;
        let mut out = Vec::new();
        for row in rows { out.push(db(row)?); }
        Ok(out)
    }

    pub fn create_playlist(&self, name: &str) -> Result<i64, String> {
        let name = name.trim();
        if name.is_empty() { return Err("El nombre de la lista no puede estar vacío".into()); }
        db(self.conn.execute("INSERT INTO playlists (name) VALUES (?1)", params![name]))
            .map_err(|_| format!("Ya existe una lista llamada «{name}»"))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() { return Err("El nombre de la lista no puede estar vacío".into()); }
        db(self.conn.execute("UPDATE playlists SET name = ?1 WHERE id = ?2", params![name, id]))
            .map_err(|_| format!("Ya existe una lista llamada «{name}»"))?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<(), String> {
        db(self.conn.execute("DELETE FROM playlist_items WHERE playlist_id = ?1", params![id]))?;
        db(self.conn.execute("DELETE FROM playlists WHERE id = ?1", params![id]))?;
        Ok(())
    }

    pub fn playlist_track_ids(&self, id: i64) -> Result<Vec<i64>, String> {
        let mut stmt = db(self.conn.prepare(
            "SELECT track_id FROM playlist_items WHERE playlist_id = ?1 ORDER BY pos"))?;
        let rows = db(stmt.query_map(params![id], |r| r.get::<_, i64>(0)))?;
        let mut out = Vec::new();
        for row in rows { out.push(db(row)?); }
        Ok(out)
    }

    pub fn add_to_playlist(&self, id: i64, track_id: i64) -> Result<(), String> {
        let next: i64 = db(self.conn.query_row(
            "SELECT COALESCE(MAX(pos) + 1, 0) FROM playlist_items WHERE playlist_id = ?1",
            params![id], |r| r.get(0)))?;
        db(self.conn.execute(
            "INSERT INTO playlist_items (playlist_id, pos, track_id) VALUES (?1, ?2, ?3)",
            params![id, next, track_id]))?;
        Ok(())
    }

    pub fn remove_from_playlist(&self, id: i64, track_id: i64) -> Result<(), String> {
        db(self.conn.execute(
            "DELETE FROM playlist_items WHERE playlist_id = ?1 AND track_id = ?2",
            params![id, track_id]))?;
        Ok(())
    }

    pub fn track_id_by_path(&self, path: &Path) -> Option<i64> {
        self.conn.query_row("SELECT id FROM tracks WHERE path = ?1",
            params![path.to_string_lossy()], |r| r.get(0)).ok()
    }

    /// Como `track_id_by_path`, pero tolera rutas movidas: prueba la ruta canónica y,
    /// si no, el nombre de archivo (sólo si es único en la biblioteca).
    pub fn track_id_lenient(&self, path: &Path) -> Option<i64> {
        if let Some(id) = self.track_id_by_path(path) { return Some(id); }
        if let Ok(c) = path.canonicalize() { if let Some(id) = self.track_id_by_path(&c) { return Some(id); } }
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let mut stmt = self.conn.prepare("SELECT id, path FROM tracks").ok()?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).ok()?;
        let mut found = None;
        for (id, p) in rows.flatten() {
            let same = Path::new(&p).file_name().map(|n| n.to_string_lossy().to_lowercase() == name).unwrap_or(false);
            if same { if found.is_some() { return None; } found = Some(id); }
        }
        found
    }
}

// ---- escaneo ----

fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

/// Recorre carpetas sin seguir enlaces simbólicos.
pub fn collect_audio_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            if is_audio(&dir) && dir.is_file() { files.push(dir); }
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() { stack.push(path); }
            else if kind.is_file() && is_audio(&path) { files.push(path); }
        }
    }
    files.sort();
    files
}

fn parse_gain(text: &str) -> Option<f32> {
    let cleaned = text.trim().trim_end_matches("dB").trim_end_matches("db").trim();
    cleaned.parse::<f32>().ok().filter(|v| v.is_finite() && v.abs() <= 40.0)
}

fn read_meta(path: &Path) -> Meta {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut meta = Meta {
        title: stem, artist: "Artista desconocido".into(), album: "Álbum desconocido".into(),
        genre: String::new(), duration: 0, track_no: 0, rg_db: None,
    };
    if let Ok(tagged) = lofty::read_from_path(path) {
        meta.duration = tagged.properties().duration().as_secs() as u32;
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(v) = tag.title() { if !v.trim().is_empty() { meta.title = v.trim().to_string(); } }
            if let Some(v) = tag.artist() { if !v.trim().is_empty() { meta.artist = v.trim().to_string(); } }
            if let Some(v) = tag.album() { if !v.trim().is_empty() { meta.album = v.trim().to_string(); } }
            if let Some(v) = tag.genre() { meta.genre = v.trim().to_string(); }
            meta.track_no = tag.track().unwrap_or(0);
            meta.rg_db = tag.get_string(&ItemKey::ReplayGainTrackGain).and_then(parse_gain);
        }
    }
    meta
}

/// Escaneo incremental: solo relee los archivos cuya fecha de modificación cambió.
pub fn scan(library: &Library, roots: &[PathBuf], mut progress: impl FnMut(usize)) -> Result<ScanStats, String> {
    let files = collect_audio_files(roots);
    let mut stats = ScanStats::default();
    db(library.conn.execute_batch("BEGIN"))?;
    for path in &files {
        stats.seen += 1;
        let mtime = fs::metadata(path).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(1);
        if library.stored_mtime(path) == Some(mtime) {
            stats.unchanged += 1;
        } else {
            let meta = read_meta(path);
            match library.upsert(path, mtime, &meta) {
                Ok(()) => stats.added += 1,
                Err(_) => stats.failed += 1,
            }
        }
        if stats.seen % 25 == 0 { progress(stats.seen); }
    }
    db(library.conn.execute_batch("COMMIT"))?;
    stats.removed = library.prune_missing()?;
    progress(stats.seen);
    Ok(stats)
}

// ---- M3U ----

pub fn export_m3u(tracks: &[&Track], target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let mut text = String::from("#EXTM3U\n");
    for t in tracks {
        text.push_str(&format!("#EXTINF:{},{} - {}\n{}\n", t.duration, t.artist, t.title, t.path.display()));
    }
    fs::write(target, text).map_err(|e| e.to_string())
}

/// Devuelve las rutas de un M3U/M3U8; las relativas se resuelven contra `base`.
pub fn parse_m3u(text: &str, base: &Path) -> Vec<PathBuf> {
    text.lines().map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| { let p = PathBuf::from(l); if p.is_absolute() { p } else { base.join(p) } })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorites_and_playlists() {
        let lib = Library::open_memory().unwrap();
        let a = lib.insert_simple("/m/a.flac", "A", "X", "Alb", "Rock", 100).unwrap();
        let b = lib.insert_simple("/m/b.flac", "B", "X", "Alb", "Rock", 200).unwrap();
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
    fn tracks_are_sorted_and_pruned() {
        let lib = Library::open_memory().unwrap();
        lib.insert_simple("/no/existe/z.mp3", "Z", "B", "Alb", "", 1).unwrap();
        lib.insert_simple("/no/existe/a.mp3", "A", "A", "Alb", "", 1).unwrap();
        let tracks = lib.all_tracks().unwrap();
        assert_eq!(tracks[0].artist, "A");
        assert_eq!(lib.prune_missing().unwrap(), 2);
        assert!(lib.all_tracks().unwrap().is_empty());
    }

    #[test]
    fn m3u_roundtrip_paths() {
        let t = Track { id: 1, path: "/m/a.flac".into(), title: "A".into(), artist: "X".into(),
            album: "Y".into(), genre: String::new(), duration: 5, track_no: 1, rg_db: None };
        let dir = std::env::temp_dir().join("muzoryn-test-m3u");
        let file = dir.join("l.m3u");
        export_m3u(&[&t], &file).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        let paths = parse_m3u(&text, Path::new("/base"));
        assert_eq!(paths, vec![PathBuf::from("/m/a.flac")]);
        assert_eq!(parse_m3u("rel/b.mp3\n#c", Path::new("/base")), vec![PathBuf::from("/base/rel/b.mp3")]);
    }

    #[test]
    fn gain_parsing() {
        assert_eq!(parse_gain("-6.50 dB"), Some(-6.5));
        assert_eq!(parse_gain("abc"), None);
    }
}
