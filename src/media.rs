//! Elementos de cola y acceso a flujos HTTP (radio y podcasts).
use std::{io::{self, Read, Seek, SeekFrom}, path::{Path, PathBuf}, sync::{mpsc, Arc, Mutex}, thread, time::Duration};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaSource {
    File(PathBuf),
    Url(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub media: MediaSource,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
    /// Ganancia ReplayGain de pista en dB, si el archivo la trae.
    pub rg_db: Option<f32>,
    /// Flujo en directo (radio): sin duración ni búsqueda.
    pub live: bool,
}

impl QueueItem {
    pub fn from_path(path: &Path) -> Self {
        let title = path.file_stem().map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Self {
            media: MediaSource::File(path.to_path_buf()),
            title, artist: String::new(), album: String::new(),
            duration: None, rg_db: None, live: false,
        }
    }

    pub fn stream(title: &str, artist: &str, url: &str, live: bool) -> Self {
        Self {
            media: MediaSource::Url(url.to_string()),
            title: title.to_string(), artist: artist.to_string(), album: String::new(),
            duration: None, rg_db: None, live,
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        match &self.media { MediaSource::File(p) => Some(p), MediaSource::Url(_) => None }
    }
}

/// Lector de red que cumple `Read + Seek` para el decodificador, pero solo
/// admite consultas de posición: los flujos en vivo no se pueden buscar.
pub struct StreamReader {
    inner: Box<dyn Read + Send + Sync + 'static>,
    position: u64,
}

impl StreamReader {
    pub fn new(inner: Box<dyn Read + Send + Sync + 'static>) -> Self {
        Self { inner, position: 0 }
    }
}

impl Read for StreamReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.position += n as u64;
        Ok(n)
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match pos {
            SeekFrom::Current(0) => Ok(self.position),
            SeekFrom::Start(p) if p == self.position => Ok(self.position),
            _ => Err(io::Error::new(io::ErrorKind::Unsupported, "flujo sin búsqueda")),
        }
    }
}

pub fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(20))
        .user_agent("muzoryn/0.5")
        .build()
}

/// Nombre legible a partir de una URL (host sin `www.`), por si no hay nada mejor.
pub fn host_name(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() { "Sin nombre".into() } else { host.to_string() }
}

/// Consulta las cabeceras ICY de una emisora: (nombre, bitrate, género). No lee el audio.
pub fn station_info(url: &str) -> Result<(Option<String>, u32, Option<String>), String> {
    let response = http_agent().get(url).call().map_err(|e| format!("Emisora no disponible: {e}"))?;
    let text = |h: &str| response.header(h).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let bitrate = response.header("icy-br")
        .and_then(|s| s.split(',').next().and_then(|n| n.trim().parse::<u32>().ok())).unwrap_or(0);
    Ok((text("icy-name"), bitrate, text("icy-genre")))
}

/// Trozos de 8 KiB que se leen por adelantado (~1 MiB): absorben los microcortes de la red
/// y evitan que el hilo de audio se quede esperando datos.
const PREFETCH_CHUNKS: usize = 128;

/// Lee el flujo de red en un hilo propio y lo entrega por una cola acotada.
struct Prefetch {
    rx: Mutex<mpsc::Receiver<io::Result<Vec<u8>>>>,
    chunk: Vec<u8>,
    pos: usize,
}

impl Prefetch {
    fn new(mut inner: Box<dyn Read + Send + Sync + 'static>) -> Self {
        let (tx, rx) = mpsc::sync_channel(PREFETCH_CHUNKS);
        thread::spawn(move || loop {
            let mut buf = vec![0u8; 8192];
            match inner.read(&mut buf) {
                Ok(0) => { let _ = tx.send(Ok(Vec::new())); break; }
                Ok(n) => { buf.truncate(n); if tx.send(Ok(buf)).is_err() { break; } }
                Err(e) => { let _ = tx.send(Err(e)); break; }
            }
        });
        Self { rx: Mutex::new(rx), chunk: Vec::new(), pos: 0 }
    }
}

impl Read for Prefetch {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() { return Ok(0); }
        while self.pos >= self.chunk.len() {
            let next = self.rx.get_mut().unwrap_or_else(|e| e.into_inner()).recv();
            match next {
                Ok(Ok(chunk)) if chunk.is_empty() => return Ok(0),
                Ok(Ok(chunk)) => { self.chunk = chunk; self.pos = 0; }
                Ok(Err(e)) => return Err(e),
                Err(_) => return Ok(0),
            }
        }
        let n = buf.len().min(self.chunk.len() - self.pos);
        buf[..n].copy_from_slice(&self.chunk[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Título de la canción en vivo (ICY `StreamTitle`), compartido con el motor de audio.
/// Vacío mientras la emisora no envíe metadatos.
pub type StreamTitle = Arc<Mutex<String>>;

/// Quita del flujo los bloques de metadatos ICY (cada `metaint` bytes de audio) y
/// publica el último `StreamTitle`, de modo que el decodificador solo ve audio.
struct IcyReader {
    inner: Box<dyn Read + Send + Sync + 'static>,
    metaint: usize,
    left: usize,
    title: StreamTitle,
}

impl IcyReader {
    fn read_metadata(&mut self) -> io::Result<()> {
        let mut len = [0u8; 1];
        self.inner.read_exact(&mut len)?;
        let size = len[0] as usize * 16;
        if size == 0 { return Ok(()); }
        let mut block = vec![0u8; size];
        self.inner.read_exact(&mut block)?;
        if let Some(title) = parse_stream_title(&block) {
            *self.title.lock().unwrap_or_else(|e| e.into_inner()) = title;
        }
        Ok(())
    }
}

impl Read for IcyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() { return Ok(0); }
        if self.left == 0 {
            match self.read_metadata() {
                Ok(()) => self.left = self.metaint,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(0),
                Err(e) => return Err(e),
            }
        }
        let max = buf.len().min(self.left);
        let n = self.inner.read(&mut buf[..max])?;
        self.left -= n;
        Ok(n)
    }
}

/// Extrae `StreamTitle='…';` de un bloque ICY (UTF-8 o Latin-1). `None` si el bloque no lo trae.
fn parse_stream_title(block: &[u8]) -> Option<String> {
    let end = block.iter().rposition(|b| *b != 0).map(|i| i + 1).unwrap_or(0);
    let block = &block[..end];
    let text = match std::str::from_utf8(block) {
        Ok(t) => t.to_string(),
        Err(_) => block.iter().map(|b| *b as char).collect(),
    };
    let rest = &text[text.find("StreamTitle='")? + "StreamTitle='".len()..];
    let value = match rest.find("';") {
        Some(i) => &rest[..i],
        None => rest.strip_suffix('\'').unwrap_or(rest),
    };
    let clean: String = value.chars().filter(|c| !c.is_control()).collect();
    Some(clean.trim().to_string())
}

/// Abre un flujo HTTP. Con `icy` pide metadatos de canción y devuelve donde se publican.
pub fn open_url(url: &str, icy: bool) -> Result<(StreamReader, StreamTitle), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("URL no admitida: {url}"));
    }
    let mut request = http_agent().get(url);
    if icy { request = request.set("Icy-MetaData", "1"); }
    let response = request.call().map_err(|e| format!("No se pudo abrir el flujo: {e}"))?;
    let title: StreamTitle = Arc::new(Mutex::new(String::new()));
    let metaint = if icy {
        response.header("icy-metaint").and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0)
    } else { 0 };
    let reader: Box<dyn Read + Send + Sync + 'static> = Box::new(Prefetch::new(response.into_reader()));
    let inner: Box<dyn Read + Send + Sync + 'static> = if metaint > 0 {
        Box::new(IcyReader { inner: reader, metaint, left: metaint, title: title.clone() })
    } else { reader };
    Ok((StreamReader::new(inner), title))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn icy_stream(metaint: usize, blocks: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, meta) in blocks.iter().enumerate() {
            out.extend(std::iter::repeat(b'A' + i as u8).take(metaint));
            let mut bytes = meta.as_bytes().to_vec();
            let units = (bytes.len() + 15) / 16;
            bytes.resize(units * 16, 0);
            out.push(units as u8);
            out.extend(bytes);
        }
        out.extend(std::iter::repeat(b'Z').take(3));
        out
    }

    #[test]
    fn icy_reader_strips_metadata_and_publishes_title() {
        let data = icy_stream(4, &["StreamTitle='Artista - Canción';", "", "StreamTitle='Don't Stop';StreamUrl='';"]);
        let title: StreamTitle = Arc::new(Mutex::new(String::new()));
        let mut reader = IcyReader { inner: Box::new(io::Cursor::new(data)), metaint: 4, left: 4, title: title.clone() };
        let mut audio = Vec::new();
        let mut buf = [0u8; 3]; // lecturas pequeñas: cruzan los límites de bloque
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 { break; }
            audio.extend_from_slice(&buf[..n]);
        }
        assert_eq!(String::from_utf8(audio).unwrap(), "AAAABBBBCCCCZZZ");
        assert_eq!(*title.lock().unwrap(), "Don't Stop");
    }

    #[test]
    fn stream_title_parsing_handles_latin1_empty_and_missing() {
        assert_eq!(parse_stream_title(b"StreamTitle='Caf\xe9 del Mar';\0\0").as_deref(), Some("Café del Mar"));
        assert_eq!(parse_stream_title(b"StreamTitle='';\0").as_deref(), Some(""));
        assert_eq!(parse_stream_title(b"StreamUrl='x';\0\0"), None);
        assert_eq!(parse_stream_title(b"StreamTitle='Sin cierre").as_deref(), Some("Sin cierre"));
    }

    struct Flaky(Vec<u8>, bool);
    impl Read for Flaky {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                if self.1 { return Err(io::Error::new(io::ErrorKind::TimedOut, "sin datos")); }
                return Ok(0);
            }
            let n = buf.len().min(self.0.len()).min(5);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0.drain(..n);
            Ok(n)
        }
    }

    #[test]
    fn prefetch_delivers_all_bytes_in_order_then_eof() {
        let data: Vec<u8> = (0..30000u32).map(|i| (i % 251) as u8).collect();
        let mut reader = Prefetch::new(Box::new(Flaky(data.clone(), false)));
        let mut out = Vec::new();
        reader.read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn prefetch_passes_bytes_then_the_error() {
        let mut reader = Prefetch::new(Box::new(Flaky(vec![1, 2, 3], true)));
        let mut buf = [0u8; 3];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3]);
        assert_eq!(reader.read(&mut buf).unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn title_comes_from_file_name() {
        let item = QueueItem::from_path(Path::new("/m/Pink Floyd - Time.flac"));
        assert_eq!(item.title, "Pink Floyd - Time");
        assert!(item.file_path().is_some());
    }
    #[test]
    fn stream_reader_rejects_real_seeks_only() {
        let mut r = StreamReader::new(Box::new(io::Cursor::new(vec![1u8, 2, 3, 4])));
        let mut buf = [0u8; 2];
        r.read_exact(&mut buf).unwrap();
        assert_eq!(r.seek(SeekFrom::Current(0)).unwrap(), 2);
        assert_eq!(r.seek(SeekFrom::Start(2)).unwrap(), 2);
        assert!(r.seek(SeekFrom::Start(0)).is_err());
        assert!(r.seek(SeekFrom::End(0)).is_err());
    }
    #[test]
    fn host_name_strips_scheme_and_www() {
        assert_eq!(host_name("https://www.radio.example.com:8000/live?x=1"), "radio.example.com");
        assert_eq!(host_name("http://u:p@ice1.somafm.com/a"), "ice1.somafm.com");
    }
    #[test]
    fn rejects_non_http_urls() {
        assert!(open_url("file:///etc/passwd", false).is_err());
    }
}
