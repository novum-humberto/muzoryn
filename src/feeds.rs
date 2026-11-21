//! Podcasts: lectura de feeds RSS y descarga de episodios.
use crate::media::{get_checked, http_agent, FetchError};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub title: String,
    pub url: String,
    pub duration: String,
    pub description: String,
    pub date: String,
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Contenido de `<tag ...>...</tag>`; admite CDATA.
fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let mut from = 0;
    while let Some(rel) = block[from..].find(&open) {
        let start = from + rel;
        let after = start + open.len();
        match block[after..].chars().next() {
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') | Some('/') => {
                let gt = block[after..].find('>')? + after;
                if block[..gt].ends_with('/') {
                    return Some(String::new());
                }
                let close = format!("</{tag}>");
                let end = block[gt + 1..].find(&close)? + gt + 1;
                let raw = block[gt + 1..end].trim();
                let raw = raw
                    .strip_prefix("<![CDATA[")
                    .and_then(|r| r.strip_suffix("]]>"))
                    .unwrap_or(raw);
                return Some(decode_entities(raw.trim()));
            }
            _ => from = after,
        }
    }
    None
}

fn attribute(block: &str, tag: &str, name: &str) -> Option<String> {
    let start = block.find(&format!("<{tag}"))?;
    let end = block[start..].find('>')? + start;
    let element = &block[start..end];
    let key = format!("{name}=\"");
    let at = element.find(&key)? + key.len();
    let close = element[at..].find('"')? + at;
    Some(decode_entities(&element[at..close]))
}

/// Convierte la duración iTunes/RSS a segundos. Acepta `HH:MM:SS`, `MM:SS`
/// y segundos simples; devuelve None si el dato está ausente o mal formado.
pub fn parse_episode_duration(value: &str) -> Option<std::time::Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let parts: Vec<&str> = value.split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut total = 0u64;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let number = part.parse::<u64>().ok()?;
        total = total.checked_mul(60)?.checked_add(number)?;
        // En formatos con campos, minutos y segundos no deben exceder 59.
        if index > 0 && number > 59 {
            return None;
        }
    }
    if total == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(total))
    }
}

pub fn parse_rss(xml: &str) -> Vec<Episode> {
    let mut episodes = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<item") {
        let Some(end) = rest[start..].find("</item>") else {
            break;
        };
        let block = &rest[start..start + end];
        if let Some(url) = attribute(block, "enclosure", "url") {
            let title = tag_text(block, "title").unwrap_or_else(|| "Sin título".into());
            let description = tag_text(block, "description")
                .or_else(|| tag_text(block, "itunes:summary"))
                .unwrap_or_default();
            episodes.push(Episode {
                title: strip_tags(&title),
                url,
                duration: tag_text(block, "itunes:duration").unwrap_or_default(),
                description: strip_tags(&description),
                date: tag_text(block, "pubDate").unwrap_or_default(),
            });
            if episodes.len() >= 60 {
                break;
            }
        }
        rest = &rest[start + end + 7..];
    }
    episodes
}

/// Título del canal (lo que hay antes del primer `<item>`).
pub fn channel_title(xml: &str) -> Option<String> {
    let head = xml.find("<item").map(|i| &xml[..i]).unwrap_or(xml);
    tag_text(head, "title")
        .map(|t| strip_tags(&t))
        .filter(|t| !t.is_empty())
}

/// URL de la imagen del canal (`itunes:image` o `<image><url>`).
pub fn channel_image(xml: &str) -> Option<String> {
    let head = xml.find("<item").map(|i| &xml[..i]).unwrap_or(xml);
    attribute(head, "itunes:image", "href")
        .or_else(|| {
            let s = head.find("<image>")?;
            tag_text(&head[s..], "url")
        })
        .map(|u| u.trim().to_string())
        .filter(|u| u.starts_with("http"))
}

/// Descarga un feed y devuelve (título del canal, episodios).
pub fn fetch_feed_full(url: &str) -> Result<(Option<String>, Vec<Episode>), String> {
    let body = get_checked(&http_agent(), url, &[])
        .map_err(|e| format!("Feed no disponible: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let episodes = parse_rss(&body);
    if episodes.is_empty() {
        return Err("La URL no es un feed de podcast con audio".into());
    }
    Ok((channel_title(&body), episodes))
}

pub fn fetch_feed(url: &str) -> Result<Vec<Episode>, String> {
    let body = get_checked(&http_agent(), url, &[])
        .map_err(|e| format!("Feed no disponible: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let episodes = parse_rss(&body);
    if episodes.is_empty() {
        Err("El feed no contiene episodios con audio".into())
    } else {
        Ok(episodes)
    }
}

pub fn safe_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim().chars().take(80).collect::<String>();
    if trimmed.is_empty() {
        "episodio".into()
    } else {
        trimmed
    }
}

/// Ruta local de un episodio descargado (aunque aún no exista).
pub fn episode_path(folder: &Path, url: &str, title: &str) -> PathBuf {
    let path_part = url.split('?').next().unwrap_or(url);
    let ext = Path::new(path_part)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp3");
    let ext = if ext.is_empty() || ext.len() > 5 {
        "mp3"
    } else {
        ext
    };
    folder.join(format!("{}.{ext}", safe_file_name(title)))
}

/// Hash FNV-1a de 64 bits: estable entre versiones de Rust, apto para nombres de archivo.
fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Archivo oculto (por carpeta de canal) que recuerda de qué URL viene cada descarga.
const INDEX_FILE: &str = ".muzoryn-index.tsv";

fn read_index(dir: &Path) -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(dir.join(INDEX_FILE))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(name, url)| (name.to_string(), url.to_string()))
        .collect()
}

fn record_in_index(dir: &Path, name: &str, url: &str) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(INDEX_FILE))
        .map_err(|e| e.to_string())?;
    let clean = |t: &str| t.replace(['\t', '\n', '\r'], " ");
    writeln!(file, "{}\t{}", clean(name), clean(url)).map_err(|e| e.to_string())
}

/// Nombre alternativo cuando otro episodio ya ocupa «Título.ext»: «Título [hash].ext».
fn disambiguated(plain: &Path, url: &str) -> PathBuf {
    let stem = plain
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("episodio");
    let ext = plain.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    plain.with_file_name(format!("{stem} [{:08x}].{ext}", stable_hash(url) as u32))
}

/// Serializa la reserva del nombre final entre hilos de descarga del mismo proceso.
static NAME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// ¿Ocupa `plain` un episodio distinto de `url`? Un archivo sin entrada en el índice (descarga
/// antigua) se considera del episodio que lo pide, para no duplicar lo ya descargado.
fn taken_by_other(plain: &Path, url: &str) -> bool {
    if !plain.exists() {
        return false;
    }
    let (Some(dir), Some(name)) = (plain.parent(), plain.file_name().and_then(|n| n.to_str()))
    else {
        return false;
    };
    read_index(dir).get(name).is_some_and(|owner| owner != url)
}

/// Ruta de destino organizada por canal: podcasts/<nombre-del-podcast>/<episodio>.<ext>.
/// Dos episodios con el mismo título y extensión no se pisan: el segundo recibe un sufijo
/// `[hash-de-su-URL]`, siempre el mismo para ese episodio.
pub fn episode_path_for_podcast(folder: &Path, podcast: &str, url: &str, title: &str) -> PathBuf {
    let plain = episode_path(&folder.join(safe_file_name(podcast)), url, title);
    if taken_by_other(&plain, url) {
        disambiguated(&plain, url)
    } else {
        plain
    }
}

/// Localiza primero la estructura nueva y después las descargas antiguas en la raíz.
pub fn find_episode_path(folder: &Path, podcast: &str, url: &str, title: &str) -> PathBuf {
    let grouped = episode_path_for_podcast(folder, podcast, url, title);
    if grouped.exists() {
        grouped
    } else {
        episode_path(folder, url, title)
    }
}

pub fn download_episode(url: &str, folder: &Path, title: &str) -> Result<PathBuf, String> {
    download_episode_with(url, folder, title, |_, _| {})
}

/// Igual que `download_episode`, pero informa `(bytes recibidos, bytes totales si se conocen)` mientras descarga.
pub fn download_episode_with<P: FnMut(u64, Option<u64>)>(
    url: &str,
    folder: &Path,
    title: &str,
    progress: P,
) -> Result<PathBuf, String> {
    download_episode_cancellable(url, folder, title, &AtomicBool::new(false), progress)
}

/// Descarga cancelable. Comprueba el token entre bloques y elimina el archivo parcial.
pub fn download_episode_cancellable<P: FnMut(u64, Option<u64>)>(
    url: &str,
    folder: &Path,
    title: &str,
    cancel: &AtomicBool,
    progress: P,
) -> Result<PathBuf, String> {
    download_episode_cancellable_for_podcast(url, folder, "Podcast", title, cancel, progress)
}

/// Límites de tiempo de una descarga. Evitan que una conexión colgada ocupe un hueco para siempre.
#[derive(Clone, Copy, Debug)]
pub struct DownloadLimits {
    /// Espera máxima para conectar.
    pub connect: Duration,
    /// Espera máxima a que llegue el siguiente bloque (servidor mudo).
    pub read: Duration,
    /// Duración total máxima de un episodio.
    pub total: Duration,
    /// Ventana en la que debe llegar al menos `min_bytes` (servidor que gotea datos).
    pub window: Duration,
    pub min_bytes: u64,
}

impl Default for DownloadLimits {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(15),
            read: Duration::from_secs(30),
            total: Duration::from_secs(2 * 60 * 60),
            window: Duration::from_secs(60),
            min_bytes: 16 * 1024,
        }
    }
}

fn download_agent(limits: &DownloadLimits) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(limits.connect)
        .timeout_read(limits.read)
        .user_agent("muzoryn/7.0.1")
        .redirects(0) // las redirecciones las sigue `get_checked`
        .build()
}

/// Abre la conexión del episodio. Reintenta hasta 3 veces (esperas de 1 s y 2 s) los errores
/// transitorios —fallos de red, 429 y 5xx— que los servidores devuelven cuando reciben varias
/// descargas a la vez. Respeta la cancelación durante la espera.
fn open_with_retry(
    url: &str,
    cancel: &AtomicBool,
    limits: &DownloadLimits,
) -> Result<ureq::Response, String> {
    const ATTEMPTS: u32 = 3;
    let agent = download_agent(limits);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let (transient, message) = match get_checked(&agent, url, &[]) {
            Ok(response) => return Ok(response),
            Err(FetchError::Status(code)) => (
                code == 429 || code >= 500,
                format!("Descarga fallida: el servidor respondió HTTP {code}"),
            ),
            Err(FetchError::Redirect(e)) => (false, format!("Descarga fallida: {e}")),
            Err(FetchError::Transport(e)) => (true, format!("Descarga fallida: {e}")),
        };
        if !transient || attempt >= ATTEMPTS {
            return Err(message);
        }
        for _ in 0..(10 * attempt) {
            if cancel.load(Ordering::SeqCst) {
                return Err("Descarga cancelada".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

pub fn download_episode_cancellable_for_podcast<P: FnMut(u64, Option<u64>)>(
    url: &str,
    folder: &Path,
    podcast: &str,
    title: &str,
    cancel: &AtomicBool,
    progress: P,
) -> Result<PathBuf, String> {
    download_episode_with_limits(
        url,
        folder,
        podcast,
        title,
        cancel,
        &DownloadLimits::default(),
        progress,
    )
}

/// Descarga con límites de tiempo explícitos (los usa la app con los valores por defecto).
pub fn download_episode_with_limits<P: FnMut(u64, Option<u64>)>(
    url: &str,
    folder: &Path,
    podcast: &str,
    title: &str,
    cancel: &AtomicBool,
    limits: &DownloadLimits,
    mut progress: P,
) -> Result<PathBuf, String> {
    use std::io::{Read, Write};
    if cancel.load(Ordering::SeqCst) {
        return Err("Descarga cancelada".into());
    }
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let target = episode_path_for_podcast(folder, podcast, url, title);
    let dir = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| folder.to_path_buf());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    const MAX_EPISODE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    let started = Instant::now();
    let response = open_with_retry(url, cancel, limits)?;
    let total = response
        .header("Content-Length")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0);
    if total.is_some_and(|n| n > MAX_EPISODE_BYTES) {
        return Err("El episodio supera el límite de descarga de 2 GiB".into());
    }
    let mut reader = response.into_reader();
    // El nombre temporal incluye un hash de la URL: episodios distintos no comparten .part.
    let tmp = target.with_file_name(format!(
        ".{}.{:016x}.part",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("episode"),
        stable_hash(url)
    ));
    let result = (|| -> Result<(), String> {
        let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 64 * 1024];
        let mut received = 0u64;
        let mut window_start = Instant::now();
        let mut window_base = 0u64;
        progress(0, total);
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("Descarga cancelada".into());
            }
            if started.elapsed() > limits.total {
                return Err("Descarga cortada: superó el tiempo máximo permitido".into());
            }
            let n = reader.read(&mut buf).map_err(|e| {
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) {
                    "Descarga cortada: el servidor dejó de enviar datos".to_string()
                } else {
                    e.to_string()
                }
            })?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            received = received.saturating_add(n as u64);
            if received > MAX_EPISODE_BYTES {
                return Err("La descarga superó el límite de 2 GiB".into());
            }
            if window_start.elapsed() >= limits.window {
                if received - window_base < limits.min_bytes {
                    return Err("Descarga cortada: velocidad demasiado baja".into());
                }
                window_start = Instant::now();
                window_base = received;
            }
            progress(received, total);
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if cancel.load(Ordering::SeqCst) {
        let _ = std::fs::remove_file(&tmp);
        return Err("Descarga cancelada".into());
    }
    // Reserva del nombre final: si otro episodio lo ocupó mientras descargábamos, usa el sufijo.
    let _guard = NAME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let final_path = if taken_by_other(&target, url) {
        disambiguated(&target, url)
    } else {
        target
    };
    if let Err(e) = std::fs::rename(&tmp, &final_path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    if let Some(name) = final_path.file_name().and_then(|n| n.to_str()) {
        let _ = record_in_index(&dir, name, url);
    }
    Ok(final_path)
}

/// Episodios que aún no están en disco (sin repetir archivo de destino).
pub fn pending_downloads<'a>(episodes: &'a [Episode], folder: &Path) -> Vec<&'a Episode> {
    let mut targets = std::collections::HashSet::new();
    episodes
        .iter()
        .filter(|ep| {
            let target = episode_path(folder, &ep.url, &ep.title);
            !target.exists() && targets.insert(target)
        })
        .collect()
}

pub fn pending_downloads_for_podcast<'a>(
    episodes: &'a [Episode],
    folder: &Path,
    podcast: &str,
) -> Vec<&'a Episode> {
    // Se deduplica por URL: dos episodios con el mismo título son distintos y ambos se descargan.
    let mut urls = std::collections::HashSet::new();
    episodes
        .iter()
        .filter(|ep| {
            !find_episode_path(folder, podcast, &ep.url, &ep.title).exists()
                && urls.insert(ep.url.as_str())
        })
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BulkReport {
    pub downloaded: usize,
    pub failed: usize,
    pub cancelled: bool,
}

/// Descarga uno tras otro los episodios pendientes de un canal. Los que fallan se cuentan y se sigue
/// con el siguiente; los ya descargados se omiten, así que repetirla continúa donde se quedó.
/// `progress(hechos, total, fallidos, título)` se llama tras cada episodio.
pub fn download_all<F, P>(
    episodes: &[Episode],
    folder: &Path,
    cancel: &AtomicBool,
    fetch: F,
    mut progress: P,
) -> BulkReport
where
    F: Fn(&Episode, &Path) -> Result<PathBuf, String>,
    P: FnMut(usize, usize, usize, &str),
{
    let pending = pending_downloads(episodes, folder);
    let total = pending.len();
    let mut report = BulkReport::default();
    for ep in pending {
        if cancel.load(Ordering::SeqCst) {
            report.cancelled = true;
            break;
        }
        match fetch(ep, folder) {
            Ok(_) => report.downloaded += 1,
            Err(_) => report.failed += 1,
        }
        progress(
            report.downloaded + report.failed,
            total,
            report.failed,
            &ep.title,
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    const RSS: &str = r#"<rss><channel><title>Canal</title>
<item><title><![CDATA[La era &amp; digital]]></title><description>Hola &lt;b&gt;mundo&lt;/b&gt;</description>
<pubDate>Mon, 01 Jan 2026</pubDate><itunes:duration>24:17</itunes:duration>
<enclosure url="https://x.test/a.mp3?x=1&amp;y=2" length="1" type="audio/mpeg"/></item>
<item><title>Sin audio</title></item>
<item><title>Segundo</title><enclosure url="https://x.test/b.mp3" /></item>
</channel></rss>"#;
    #[test]
    fn parses_items_with_enclosures() {
        let eps = parse_rss(RSS);
        assert_eq!(eps.len(), 2);
        assert_eq!(eps[0].title, "La era & digital");
        assert_eq!(eps[0].url, "https://x.test/a.mp3?x=1&y=2");
        assert_eq!(eps[0].duration, "24:17");
        assert_eq!(eps[0].description, "Hola mundo");
        assert_eq!(eps[1].title, "Segundo");
    }
    #[test]
    fn parses_podcast_episode_durations_for_progress_bar() {
        assert_eq!(
            parse_episode_duration("24:17"),
            Some(std::time::Duration::from_secs(1457))
        );
        assert_eq!(
            parse_episode_duration("1:02:03"),
            Some(std::time::Duration::from_secs(3723))
        );
        assert_eq!(
            parse_episode_duration("3600"),
            Some(std::time::Duration::from_secs(3600))
        );
        assert_eq!(parse_episode_duration(""), None);
        assert_eq!(parse_episode_duration("1:75"), None);
    }

    #[test]
    fn channel_title_is_read() {
        assert_eq!(channel_title(RSS).as_deref(), Some("Canal"));
    }
    #[test]
    fn channel_image_is_read() {
        let x = r#"<rss><channel><itunes:image href="https://x.test/c.jpg"/><item><itunes:image href="https://x.test/e.jpg"/></item></channel></rss>"#;
        assert_eq!(channel_image(x).as_deref(), Some("https://x.test/c.jpg"));
        assert_eq!(
            channel_image(
                "<rss><channel><image><url>https://x.test/i.png</url></image></channel></rss>"
            )
            .as_deref(),
            Some("https://x.test/i.png")
        );
    }
    fn ep(title: &str, url: &str) -> Episode {
        Episode {
            title: title.into(),
            url: url.into(),
            ..Default::default()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muzoryn-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn episode_paths_are_grouped_by_podcast_and_keep_legacy_downloads() {
        let dir = temp_dir("podcast-folders");
        let grouped = episode_path_for_podcast(
            &dir,
            "Historia / Ciencia",
            "https://example.org/audio/ep-01.mp3",
            "El origen: parte 1",
        );
        assert!(grouped.starts_with(dir.join("Historia _ Ciencia")));
        assert_eq!(
            grouped.file_name().unwrap().to_string_lossy(),
            "El origen_ parte 1.mp3"
        );
        let legacy = episode_path(
            &dir,
            "https://example.org/audio/ep-01.mp3",
            "El origen: parte 1",
        );
        std::fs::write(&legacy, b"old-download").unwrap();
        assert_eq!(
            find_episode_path(
                &dir,
                "Historia",
                "https://example.org/audio/ep-01.mp3",
                "El origen: parte 1"
            ),
            legacy
        );
        assert_ne!(
            episode_path_for_podcast(
                &dir,
                "Historia",
                "https://example.org/audio/ep-01.mp3",
                "El origen: parte 1"
            ),
            legacy
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bulk_download_skips_existing_and_counts_failures() {
        let dir = temp_dir("bulk");
        let eps = vec![
            ep("Uno", "https://x.test/1.mp3"),
            ep("Dos", "https://x.test/2.mp3"),
            ep("Tres", "https://x.test/3.mp3"),
            ep("Uno", "https://x.test/1b.mp3"),
        ];
        std::fs::write(episode_path(&dir, &eps[1].url, &eps[1].title), b"x").unwrap();
        assert_eq!(
            pending_downloads(&eps, &dir).len(),
            2,
            "omite el que existe y el título repetido"
        );
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let report = download_all(
            &eps,
            &dir,
            &cancel,
            |e, folder| {
                if e.title == "Tres" {
                    Err("sin red".into())
                } else {
                    let p = episode_path(folder, &e.url, &e.title);
                    std::fs::write(&p, b"audio").unwrap();
                    Ok(p)
                }
            },
            |done, total, failed, title| seen.push((done, total, failed, title.to_string())),
        );
        assert_eq!(
            report,
            BulkReport {
                downloaded: 1,
                failed: 1,
                cancelled: false
            }
        );
        assert_eq!(
            seen,
            vec![(1, 2, 0, "Uno".to_string()), (2, 2, 1, "Tres".to_string())]
        );
        // Repetir continúa solo con lo que falló.
        assert_eq!(pending_downloads(&eps, &dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bulk_download_can_be_cancelled() {
        let dir = temp_dir("cancel");
        let eps = vec![
            ep("A", "https://x.test/a.mp3"),
            ep("B", "https://x.test/b.mp3"),
            ep("C", "https://x.test/c.mp3"),
        ];
        let cancel = AtomicBool::new(false);
        let report = download_all(
            &eps,
            &dir,
            &cancel,
            |e, folder| {
                let p = episode_path(folder, &e.url, &e.title);
                std::fs::write(&p, b"a").unwrap();
                Ok(p)
            },
            |done, _, _, _| {
                if done == 1 {
                    cancel.store(true, Ordering::SeqCst)
                }
            },
        );
        assert_eq!(
            report,
            BulkReport {
                downloaded: 1,
                failed: 0,
                cancelled: true
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_download_stops_before_network_and_creates_no_partial_file() {
        let dir = std::env::temp_dir().join(format!(
            "muzoryn-cancel-before-start-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let cancel = AtomicBool::new(true);
        let result = download_episode_cancellable(
            "https://example.invalid/audio.mp3",
            &dir,
            "cancelado",
            &cancel,
            |_, _| {},
        );
        assert_eq!(result.unwrap_err(), "Descarga cancelada");
        assert!(!dir.exists());
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("a/b:c"), "a_b_c");
        assert_eq!(safe_file_name("///"), "___");
        assert_eq!(safe_file_name(""), "episodio");
    }

    // ---- Pruebas de límites de tiempo y colisión de nombres (7.0.1) ----

    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;

    /// Servidor HTTP local mínimo: `handler(ruta, conexión)` atiende cada petición en su hilo.
    fn serve<F>(handler: F) -> String
    where
        F: Fn(String, std::net::TcpStream) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handler = std::sync::Arc::new(handler);
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let handler = handler.clone();
                std::thread::spawn(move || {
                    let mut conn = conn;
                    let mut buf = [0u8; 2048];
                    let n = conn.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                    handler(path, conn);
                });
            }
        });
        format!("http://{addr}")
    }

    fn fast_limits() -> DownloadLimits {
        DownloadLimits {
            connect: Duration::from_millis(500),
            read: Duration::from_millis(400),
            total: Duration::from_secs(30),
            window: Duration::from_secs(30),
            min_bytes: 1,
        }
    }

    fn no_files_left(dir: &Path) -> bool {
        std::fs::read_dir(dir)
            .map(|d| {
                d.flatten()
                    .all(|e| !e.file_name().to_string_lossy().ends_with(".part"))
            })
            .unwrap_or(true)
    }

    #[test]
    fn download_is_cut_when_server_stops_sending_data() {
        let base = serve(|_, mut c| {
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\nabc");
            std::thread::sleep(Duration::from_secs(10)); // se queda colgado
        });
        let dir = temp_dir("stall");
        let started = Instant::now();
        let err = download_episode_with_limits(
            &format!("{base}/x.mp3"),
            &dir,
            "Canal",
            "Colgado",
            &AtomicBool::new(false),
            &fast_limits(),
            |_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("dejó de enviar datos"), "mensaje: {err}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(no_files_left(&dir.join("Canal")), "no deja .part");
        assert!(
            !episode_path_for_podcast(&dir, "Canal", &format!("{base}/x.mp3"), "Colgado").exists()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_is_cut_when_total_time_is_exceeded() {
        let base = serve(|_, mut c| {
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\n");
            for _ in 0..200 {
                if c.write_all(b"x").is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let dir = temp_dir("total");
        let limits = DownloadLimits {
            total: Duration::from_millis(300),
            ..fast_limits()
        };
        let err = download_episode_with_limits(
            &format!("{base}/x.mp3"),
            &dir,
            "Canal",
            "Lento",
            &AtomicBool::new(false),
            &limits,
            |_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("tiempo máximo"), "mensaje: {err}");
        assert!(no_files_left(&dir.join("Canal")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_is_cut_when_speed_stays_below_minimum() {
        let base = serve(|_, mut c| {
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\n");
            for _ in 0..200 {
                if c.write_all(b"x").is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let dir = temp_dir("slow");
        let limits = DownloadLimits {
            window: Duration::from_millis(300),
            min_bytes: 10_000,
            ..fast_limits()
        };
        let err = download_episode_with_limits(
            &format!("{base}/x.mp3"),
            &dir,
            "Canal",
            "Gotea",
            &AtomicBool::new(false),
            &limits,
            |_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("velocidad"), "mensaje: {err}");
        assert!(no_files_left(&dir.join("Canal")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn healthy_download_is_not_cut_by_limits() {
        let base = serve(|_, mut c| {
            let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
        });
        let dir = temp_dir("healthy");
        let path = download_episode_with_limits(
            &format!("{base}/ok.mp3"),
            &dir,
            "Canal",
            "Bueno",
            &AtomicBool::new(false),
            &DownloadLimits::default(),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn body_by_path(path: String, mut c: std::net::TcpStream) {
        let body = if path.starts_with("/a") {
            "AAAA"
        } else {
            "BBBB"
        };
        let _ = write!(
            c,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
    }

    #[test]
    fn two_episodes_with_same_title_do_not_overwrite_each_other() {
        let base = serve(body_by_path);
        let dir = temp_dir("collision");
        let (ua, ub) = (format!("{base}/a.mp3"), format!("{base}/b.mp3"));
        let never = AtomicBool::new(false);
        let dl = |u: &str| {
            download_episode_with_limits(
                u,
                &dir,
                "Canal",
                "Mismo título",
                &never,
                &fast_limits(),
                |_, _| {},
            )
            .unwrap()
        };
        let pa = dl(&ua);
        let pb = dl(&ub);
        assert_ne!(pa, pb);
        assert_eq!(std::fs::read(&pa).unwrap(), b"AAAA");
        assert_eq!(std::fs::read(&pb).unwrap(), b"BBBB");
        // Cada episodio vuelve a encontrar su propio archivo (y solo ese).
        assert_eq!(find_episode_path(&dir, "Canal", &ua, "Mismo título"), pa);
        assert_eq!(find_episode_path(&dir, "Canal", &ub, "Mismo título"), pb);
        assert!(pb.file_name().unwrap().to_string_lossy().contains(" ["));
        // Ya están ambos: no queda nada pendiente.
        let eps = vec![ep("Mismo título", &ua), ep("Mismo título", &ub)];
        assert!(pending_downloads_for_podcast(&eps, &dir, "Canal").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_downloads_with_same_title_keep_both_files() {
        let base = serve(|p, c| {
            std::thread::sleep(Duration::from_millis(150)); // fuerza el solapamiento
            body_by_path(p, c);
        });
        let dir = temp_dir("concurrent");
        let (ua, ub) = (format!("{base}/a.mp3"), format!("{base}/b.mp3"));
        let handles: Vec<_> = [ua.clone(), ub.clone()]
            .into_iter()
            .map(|u| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    download_episode_with_limits(
                        &u,
                        &dir,
                        "Canal",
                        "Mismo título",
                        &AtomicBool::new(false),
                        &fast_limits(),
                        |_, _| {},
                    )
                    .unwrap()
                })
            })
            .collect();
        let paths: Vec<PathBuf> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_ne!(paths[0], paths[1]);
        let mut bodies: Vec<Vec<u8>> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
        bodies.sort();
        assert_eq!(bodies, vec![b"AAAA".to_vec(), b"BBBB".to_vec()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_for_podcast_keeps_same_title_episodes_but_drops_duplicate_urls() {
        let dir = temp_dir("pending-same-title");
        let eps = vec![
            ep("Igual", "https://x.test/1.mp3"),
            ep("Igual", "https://x.test/2.mp3"),
            ep("Igual", "https://x.test/2.mp3"),
        ];
        assert_eq!(pending_downloads_for_podcast(&eps, &dir, "Canal").len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disambiguated_name_is_stable_and_keeps_extension() {
        let plain = PathBuf::from("/x/Canal/Título.mp3");
        let a = disambiguated(&plain, "https://x.test/2.mp3");
        assert_eq!(a, disambiguated(&plain, "https://x.test/2.mp3"));
        assert_ne!(a, disambiguated(&plain, "https://x.test/3.mp3"));
        assert_eq!(a.extension().unwrap(), "mp3");
        assert!(a
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Título ["));
    }
}
