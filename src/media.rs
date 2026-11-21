//! Elementos de cola y acceso a flujos HTTP (radio y podcasts).
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaSource {
    File(PathBuf),
    Url(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Self {
            media: MediaSource::File(path.to_path_buf()),
            title,
            artist: String::new(),
            album: String::new(),
            duration: None,
            rg_db: None,
            live: false,
        }
    }

    pub fn stream(title: &str, artist: &str, url: &str, live: bool) -> Self {
        Self {
            media: MediaSource::Url(url.to_string()),
            title: title.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            duration: None,
            rg_db: None,
            live,
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        match &self.media {
            MediaSource::File(p) => Some(p),
            MediaSource::Url(_) => None,
        }
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
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "flujo sin búsqueda",
            )),
        }
    }
}

pub fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(20))
        .user_agent("muzoryn/7.0.1")
        .redirects(0) // las redirecciones las sigue `get_checked`
        .build()
}

/// Máximo de redirecciones que sigue `get_checked`.
pub const MAX_REDIRECTS: usize = 5;

/// Fallo de `get_checked`.
#[derive(Debug)]
pub enum FetchError {
    /// El servidor respondió con un estado de error (>= 400).
    Status(u16),
    /// Error de red o de transporte.
    Transport(String),
    /// Redirección rechazada (de HTTPS a HTTP, demasiadas, o destino no válido).
    Redirect(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Status(code) => write!(f, "HTTP {code}"),
            FetchError::Transport(e) => write!(f, "{e}"),
            FetchError::Redirect(e) => write!(f, "{e}"),
        }
    }
}

/// Resuelve el `Location` de una redirección respecto a la URL actual.
pub fn resolve_location(base: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.is_empty() || location.chars().any(|c| c.is_control()) {
        return None;
    }
    let (scheme, rest) = base.split_once("://")?;
    if location.starts_with("http://") || location.starts_with("https://") {
        return Some(location.to_string());
    }
    if let Some(r) = location.strip_prefix("//") {
        return Some(format!("{scheme}://{r}"));
    }
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, path) = rest.split_at(host_end);
    if location.starts_with('/') {
        return Some(format!("{scheme}://{host}{location}"));
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Some(format!("{scheme}://{host}{dir}/{location}"))
}

/// Política de redirecciones: nunca de HTTPS a HTTP y solo a http(s).
pub fn check_redirect(current: &str, next: &str) -> Result<(), FetchError> {
    if current.starts_with("https://") && !next.starts_with("https://") {
        return Err(FetchError::Redirect(format!(
            "Redirección bloqueada: {} intenta bajar de HTTPS a HTTP",
            host_name(current)
        )));
    }
    if !next.starts_with("http://") && !next.starts_with("https://") {
        return Err(FetchError::Redirect(
            "Redirección a un esquema no admitido".into(),
        ));
    }
    Ok(())
}

/// GET que sigue las redirecciones a mano (hasta `MAX_REDIRECTS`) y **rechaza** las que
/// bajan de HTTPS a HTTP, para que una respuesta HTTPS no degrade la conexión a texto plano.
/// El agente debe haberse creado con `.redirects(0)`.
pub fn get_checked(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<ureq::Response, FetchError> {
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let mut request = agent.get(&current);
        for (name, value) in headers {
            request = request.set(name, value);
        }
        let response = match request.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, _)) => return Err(FetchError::Status(code)),
            Err(e) => return Err(FetchError::Transport(e.to_string())),
        };
        if !matches!(response.status(), 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        let Some(location) = response.header("location") else {
            return Ok(response);
        };
        let next = resolve_location(&current, location)
            .ok_or_else(|| FetchError::Redirect("Redirección con destino no válido".into()))?;
        check_redirect(&current, &next)?;
        current = next;
    }
    Err(FetchError::Redirect(format!(
        "Demasiadas redirecciones (más de {MAX_REDIRECTS})"
    )))
}

/// Nombre legible a partir de una URL (host sin `www.`), por si no hay nada mejor.
pub fn host_name(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    let host = host.strip_prefix("www.").unwrap_or(host);
    if host.is_empty() {
        "Sin nombre".into()
    } else {
        host.to_string()
    }
}

/// Consulta las cabeceras ICY de una emisora: (nombre, bitrate, género). No lee el audio.
pub fn station_info(url: &str) -> Result<(Option<String>, u32, Option<String>), String> {
    let response =
        get_checked(&http_agent(), url, &[]).map_err(|e| format!("Emisora no disponible: {e}"))?;
    let text = |h: &str| {
        response
            .header(h)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let bitrate = response
        .header("icy-br")
        .and_then(|s| {
            s.split(',')
                .next()
                .and_then(|n| n.trim().parse::<u32>().ok())
        })
        .unwrap_or(0);
    Ok((text("icy-name"), bitrate, text("icy-genre")))
}

/// Trozos de 8 KiB que se leen por adelantado (~1 MiB): absorben los microcortes de la red
/// y evitan que el hilo de audio se quede esperando datos.
const PREFETCH_CHUNKS: usize = 128;
/// Búfer independiente para podcasts: 512 × 8 KiB ≈ 4 MiB (unos 2 minutos
/// a 256 kbit/s). La cola es acotada: nunca descarga el episodio entero por adelantado.
const PODCAST_BUFFER_CHUNKS: usize = 512;
const STREAM_CHUNK_BYTES: usize = 8192;

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
                Ok(0) => {
                    let _ = tx.send(Ok(Vec::new()));
                    break;
                }
                Ok(n) => {
                    buf.truncate(n);
                    if tx.send(Ok(buf)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break;
                }
            }
        });
        Self {
            rx: Mutex::new(rx),
            chunk: Vec::new(),
            pos: 0,
        }
    }
}

impl Read for Prefetch {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.pos >= self.chunk.len() {
            let next = self.rx.get_mut().unwrap_or_else(|e| e.into_inner()).recv();
            match next {
                Ok(Ok(chunk)) if chunk.is_empty() => return Ok(0),
                Ok(Ok(chunk)) => {
                    self.chunk = chunk;
                    self.pos = 0;
                }
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

/// Búfer de podcast con reconexión HTTP Range. El hilo de red llena una cola
/// acotada; el hilo de audio consume lo recibido aunque Internet se interrumpa.
/// Si se corta la red, solicita los bytes siguientes, nunca vuelve al inicio.
struct PodcastPrefetch {
    rx: Mutex<mpsc::Receiver<io::Result<Vec<u8>>>>,
    chunk: Vec<u8>,
    pos: usize,
}

impl PodcastPrefetch {
    fn new(url: String, initial: ureq::Response) -> Self {
        let (tx, rx) = mpsc::sync_channel(PODCAST_BUFFER_CHUNKS);
        thread::spawn(move || {
            let agent = http_agent();
            let mut response = Some(initial);
            let mut offset = 0u64;
            let mut total = response
                .as_ref()
                .and_then(|r| r.header("content-length"))
                .and_then(|v| v.parse::<u64>().ok());
            let mut retry = 0u32;

            loop {
                let current = match response.take() {
                    Some(r) => r,
                    None => {
                        // Espera progresiva, sin abandonar la reproducción por un corte temporal.
                        let wait = match retry {
                            0 => 1,
                            1 => 2,
                            2 => 4,
                            3 => 8,
                            4 => 15,
                            _ => 30,
                        };
                        thread::sleep(Duration::from_secs(wait));
                        let range = format!("bytes={offset}-");
                        let ranged = get_checked(&agent, &url, &[("Range", range.as_str())]);
                        match ranged {
                            Ok(r) if r.status() == 206 => {
                                let valid_start = r
                                    .header("content-range")
                                    .and_then(|v| v.strip_prefix("bytes "))
                                    .and_then(|v| v.split_once('-'))
                                    .and_then(|(start, _)| start.parse::<u64>().ok())
                                    == Some(offset);
                                if !valid_start {
                                    let _ = tx.send(Err(io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        "el servidor respondió un rango distinto al solicitado",
                                    )));
                                    break;
                                }
                                if let Some(value) = r.header("content-range") {
                                    total = value
                                        .rsplit('/')
                                        .next()
                                        .and_then(|v| v.parse::<u64>().ok());
                                }
                                retry = 0;
                                r
                            }
                            Ok(_) => {
                                let _ = tx.send(Err(io::Error::new(
                                    io::ErrorKind::Unsupported,
                                    "el servidor no admite reanudar el podcast (HTTP Range)",
                                )));
                                break;
                            }
                            Err(_) => {
                                retry = retry.saturating_add(1);
                                continue;
                            }
                        }
                    }
                };

                let mut body = current.into_reader();
                loop {
                    let mut buf = vec![0u8; STREAM_CHUNK_BYTES];
                    match body.read(&mut buf) {
                        Ok(0) => {
                            if total.map(|n| offset >= n).unwrap_or(true) {
                                let _ = tx.send(Ok(Vec::new()));
                                return;
                            }
                            // EOF antes del tamaño anunciado: se considera corte y se reanuda.
                            break;
                        }
                        Ok(n) => {
                            offset = offset.saturating_add(n as u64);
                            buf.truncate(n);
                            if tx.send(Ok(buf)).is_err() {
                                return;
                            }
                            if total.map(|n| offset >= n).unwrap_or(false) {
                                let _ = tx.send(Ok(Vec::new()));
                                return;
                            }
                        }
                        Err(_) => break,
                    }
                }
                retry = retry.saturating_add(1);
                // El siguiente ciclo pide exactamente el byte que todavía no se recibió.
            }
        });
        Self {
            rx: Mutex::new(rx),
            chunk: Vec::new(),
            pos: 0,
        }
    }
}

impl Read for PodcastPrefetch {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        while self.pos >= self.chunk.len() {
            match self.rx.get_mut().unwrap_or_else(|e| e.into_inner()).recv() {
                Ok(Ok(chunk)) if chunk.is_empty() => return Ok(0),
                Ok(Ok(chunk)) => {
                    self.chunk = chunk;
                    self.pos = 0;
                }
                Ok(Err(error)) => return Err(error),
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
        if size == 0 {
            return Ok(());
        }
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
        if buf.is_empty() {
            return Ok(0);
        }
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
    let end = block
        .iter()
        .rposition(|b| *b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
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
    let headers: &[(&str, &str)] = if icy { &[("Icy-MetaData", "1")] } else { &[] };
    let response = get_checked(&http_agent(), url, headers)
        .map_err(|e| format!("No se pudo abrir el flujo: {e}"))?;
    let title: StreamTitle = Arc::new(Mutex::new(String::new()));
    let metaint = if icy {
        response
            .header("icy-metaint")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0)
    } else {
        0
    };
    let reader: Box<dyn Read + Send + Sync + 'static> = if icy {
        // Radio conserva su lector y su comportamiento actuales.
        Box::new(Prefetch::new(response.into_reader()))
    } else {
        // Podcast: hasta ~4 MiB de audio adelantado y reconexión desde el byte pendiente.
        Box::new(PodcastPrefetch::new(url.to_string(), response))
    };
    let inner: Box<dyn Read + Send + Sync + 'static> = if metaint > 0 {
        Box::new(IcyReader {
            inner: reader,
            metaint,
            left: metaint,
            title: title.clone(),
        })
    } else {
        reader
    };
    Ok((StreamReader::new(inner), title))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn icy_stream(metaint: usize, blocks: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, meta) in blocks.iter().enumerate() {
            out.extend(std::iter::repeat_n(b'A' + i as u8, metaint));
            let mut bytes = meta.as_bytes().to_vec();
            let units = bytes.len().div_ceil(16);
            bytes.resize(units * 16, 0);
            out.push(units as u8);
            out.extend(bytes);
        }
        out.extend(std::iter::repeat_n(b'Z', 3));
        out
    }

    #[test]
    fn icy_reader_strips_metadata_and_publishes_title() {
        let data = icy_stream(
            4,
            &[
                "StreamTitle='Artista - Canción';",
                "",
                "StreamTitle='Don't Stop';StreamUrl='';",
            ],
        );
        let title: StreamTitle = Arc::new(Mutex::new(String::new()));
        let mut reader = IcyReader {
            inner: Box::new(io::Cursor::new(data)),
            metaint: 4,
            left: 4,
            title: title.clone(),
        };
        let mut audio = Vec::new();
        let mut buf = [0u8; 3]; // lecturas pequeñas: cruzan los límites de bloque
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            audio.extend_from_slice(&buf[..n]);
        }
        assert_eq!(String::from_utf8(audio).unwrap(), "AAAABBBBCCCCZZZ");
        assert_eq!(*title.lock().unwrap(), "Don't Stop");
    }

    #[test]
    fn stream_title_parsing_handles_latin1_empty_and_missing() {
        assert_eq!(
            parse_stream_title(b"StreamTitle='Caf\xe9 del Mar';\0\0").as_deref(),
            Some("Café del Mar")
        );
        assert_eq!(
            parse_stream_title(b"StreamTitle='';\0").as_deref(),
            Some("")
        );
        assert_eq!(parse_stream_title(b"StreamUrl='x';\0\0"), None);
        assert_eq!(
            parse_stream_title(b"StreamTitle='Sin cierre").as_deref(),
            Some("Sin cierre")
        );
    }

    struct Flaky(Vec<u8>, bool);
    impl Read for Flaky {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                if self.1 {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "sin datos"));
                }
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
        assert_eq!(
            reader.read(&mut buf).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
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
        assert_eq!(r.stream_position().unwrap(), 2);
        assert_eq!(r.seek(SeekFrom::Start(2)).unwrap(), 2);
        assert!(r.seek(SeekFrom::Start(0)).is_err());
        assert!(r.seek(SeekFrom::End(0)).is_err());
    }
    #[test]
    fn host_name_strips_scheme_and_www() {
        assert_eq!(
            host_name("https://www.radio.example.com:8000/live?x=1"),
            "radio.example.com"
        );
        assert_eq!(host_name("http://u:p@ice1.somafm.com/a"), "ice1.somafm.com");
    }
    #[test]
    fn rejects_non_http_urls() {
        assert!(open_url("file:///etc/passwd", false).is_err());
    }
}

#[cfg(test)]
mod redirect_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Servidor local: responde a cada petición con la respuesta que da `route(ruta)`.
    fn serve(route: fn(&str) -> String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = text.split_whitespace().nth(1).unwrap_or("/").to_string();
                let _ = stream.write_all(route(&path).as_bytes());
            }
        });
        format!("http://{addr}")
    }

    fn redirect(to: &str) -> String {
        format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn resolves_relative_locations() {
        assert_eq!(
            resolve_location("https://a.com/x/y?q=1", "z").as_deref(),
            Some("https://a.com/x/z")
        );
        assert_eq!(
            resolve_location("https://a.com/x/y", "/r").as_deref(),
            Some("https://a.com/r")
        );
        assert_eq!(
            resolve_location("https://a.com/x", "//b.com/p").as_deref(),
            Some("https://b.com/p")
        );
        assert_eq!(
            resolve_location("http://a.com", "http://b.com/").as_deref(),
            Some("http://b.com/")
        );
        assert_eq!(resolve_location("http://a.com", "bad\nloc"), None);
    }

    #[test]
    fn blocks_https_to_http_downgrade() {
        assert!(matches!(
            check_redirect("https://a.com/x", "http://a.com/x"),
            Err(FetchError::Redirect(_))
        ));
        assert!(check_redirect("https://a.com", "https://b.com").is_ok());
        assert!(check_redirect("http://a.com", "https://a.com").is_ok());
        assert!(check_redirect("http://a.com", "http://b.com").is_ok());
        assert!(check_redirect("https://a.com", "ftp://a.com").is_err());
        assert!(check_redirect("http://a.com", "file:///etc/passwd").is_err());
    }

    #[test]
    fn follows_relative_redirect_chain() {
        let base = serve(|p| match p {
            "/a" => redirect("/b"),
            "/b" => redirect("c"),
            "/c" => ok("fin"),
            _ => ok("?"),
        });
        let r = get_checked(&http_agent(), &format!("{base}/a"), &[]).unwrap();
        assert_eq!(r.into_string().unwrap(), "fin");
    }

    #[test]
    fn stops_redirect_loops() {
        let base = serve(|_| redirect("/loop"));
        let e = get_checked(&http_agent(), &format!("{base}/loop"), &[]).unwrap_err();
        assert!(matches!(e, FetchError::Redirect(m) if m.contains("Demasiadas")));
    }

    #[test]
    fn rejects_non_http_redirect_target() {
        let base = serve(|_| redirect("file:///etc/passwd"));
        let e = get_checked(&http_agent(), &format!("{base}/x"), &[]).unwrap_err();
        assert!(matches!(e, FetchError::Redirect(_)));
    }

    #[test]
    fn reports_error_status() {
        let base = serve(|_| {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()
        });
        let e = get_checked(&http_agent(), &format!("{base}/x"), &[]).unwrap_err();
        assert!(matches!(e, FetchError::Status(404)));
    }
}
