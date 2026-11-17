//! Podcasts: lectura de feeds RSS y descarga de episodios.
use std::{fs::File, io, path::{Path, PathBuf}, sync::atomic::{AtomicBool, Ordering}};
use crate::media::http_agent;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub title: String,
    pub url: String,
    pub duration: String,
    pub description: String,
    pub date: String,
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"")
        .replace("&#39;", "'").replace("&apos;", "'").replace("&amp;", "&")
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
                if block[..gt].ends_with('/') { return Some(String::new()); }
                let close = format!("</{tag}>");
                let end = block[gt + 1..].find(&close)? + gt + 1;
                let raw = block[gt + 1..end].trim();
                let raw = raw.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")).unwrap_or(raw);
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

pub fn parse_rss(xml: &str) -> Vec<Episode> {
    let mut episodes = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<item") {
        let Some(end) = rest[start..].find("</item>") else { break };
        let block = &rest[start..start + end];
        if let Some(url) = attribute(block, "enclosure", "url") {
            let title = tag_text(block, "title").unwrap_or_else(|| "Sin título".into());
            let description = tag_text(block, "description")
                .or_else(|| tag_text(block, "itunes:summary")).unwrap_or_default();
            episodes.push(Episode {
                title: strip_tags(&title),
                url,
                duration: tag_text(block, "itunes:duration").unwrap_or_default(),
                description: strip_tags(&description),
                date: tag_text(block, "pubDate").unwrap_or_default(),
            });
            if episodes.len() >= 60 { break; }
        }
        rest = &rest[start + end + 7..];
    }
    episodes
}

/// Título del canal (lo que hay antes del primer `<item>`).
pub fn channel_title(xml: &str) -> Option<String> {
    let head = xml.find("<item").map(|i| &xml[..i]).unwrap_or(xml);
    tag_text(head, "title").map(|t| strip_tags(&t)).filter(|t| !t.is_empty())
}

/// URL de la imagen del canal (`itunes:image` o `<image><url>`).
pub fn channel_image(xml: &str) -> Option<String> {
    let head = xml.find("<item").map(|i| &xml[..i]).unwrap_or(xml);
    attribute(head, "itunes:image", "href")
        .or_else(|| { let s = head.find("<image>")?; tag_text(&head[s..], "url") })
        .map(|u| u.trim().to_string())
        .filter(|u| u.starts_with("http"))
}

/// Descarga un feed y devuelve (título del canal, episodios).
pub fn fetch_feed_full(url: &str) -> Result<(Option<String>, Vec<Episode>), String> {
    let body = http_agent().get(url).call().map_err(|e| format!("Feed no disponible: {e}"))?
        .into_string().map_err(|e| e.to_string())?;
    let episodes = parse_rss(&body);
    if episodes.is_empty() { return Err("La URL no es un feed de podcast con audio".into()); }
    Ok((channel_title(&body), episodes))
}

pub fn fetch_feed(url: &str) -> Result<Vec<Episode>, String> {
    let body = http_agent().get(url).call().map_err(|e| format!("Feed no disponible: {e}"))?
        .into_string().map_err(|e| e.to_string())?;
    let episodes = parse_rss(&body);
    if episodes.is_empty() { Err("El feed no contiene episodios con audio".into()) } else { Ok(episodes) }
}

pub fn safe_file_name(title: &str) -> String {
    let cleaned: String = title.chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim().chars().take(80).collect::<String>();
    if trimmed.is_empty() { "episodio".into() } else { trimmed }
}

/// Ruta local de un episodio descargado (aunque aún no exista).
pub fn episode_path(folder: &Path, url: &str, title: &str) -> PathBuf {
    let path_part = url.split('?').next().unwrap_or(url);
    let ext = Path::new(path_part).extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    let ext = if ext.is_empty() || ext.len() > 5 { "mp3" } else { ext };
    folder.join(format!("{}.{ext}", safe_file_name(title)))
}

pub fn download_episode(url: &str, folder: &Path, title: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let target = episode_path(folder, url, title);
    let response = http_agent().get(url).call().map_err(|e| format!("Descarga fallida: {e}"))?;
    let mut reader = response.into_reader();
    let tmp = target.with_extension("part");
    {
        let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
        io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Episodios que aún no están en disco (sin repetir archivo de destino).
pub fn pending_downloads<'a>(episodes: &'a [Episode], folder: &Path) -> Vec<&'a Episode> {
    let mut targets = std::collections::HashSet::new();
    episodes.iter()
        .filter(|ep| {
            let target = episode_path(folder, &ep.url, &ep.title);
            !target.exists() && targets.insert(target)
        })
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BulkReport { pub downloaded: usize, pub failed: usize, pub cancelled: bool }

/// Descarga uno tras otro los episodios pendientes de un canal. Los que fallan se cuentan y se sigue
/// con el siguiente; los ya descargados se omiten, así que repetirla continúa donde se quedó.
/// `progress(hechos, total, fallidos, título)` se llama tras cada episodio.
pub fn download_all<F, P>(episodes: &[Episode], folder: &Path, cancel: &AtomicBool, fetch: F, mut progress: P) -> BulkReport
where F: Fn(&Episode, &Path) -> Result<PathBuf, String>, P: FnMut(usize, usize, usize, &str) {
    let pending = pending_downloads(episodes, folder);
    let total = pending.len();
    let mut report = BulkReport::default();
    for ep in pending {
        if cancel.load(Ordering::SeqCst) { report.cancelled = true; break; }
        match fetch(ep, folder) {
            Ok(_) => report.downloaded += 1,
            Err(_) => report.failed += 1,
        }
        progress(report.downloaded + report.failed, total, report.failed, &ep.title);
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
    fn channel_title_is_read() {
        assert_eq!(channel_title(RSS).as_deref(), Some("Canal"));
    }
    #[test]
    fn channel_image_is_read() {
        let x = r#"<rss><channel><itunes:image href="https://x.test/c.jpg"/><item><itunes:image href="https://x.test/e.jpg"/></item></channel></rss>"#;
        assert_eq!(channel_image(x).as_deref(), Some("https://x.test/c.jpg"));
        assert_eq!(channel_image("<rss><channel><image><url>https://x.test/i.png</url></image></channel></rss>").as_deref(), Some("https://x.test/i.png"));
    }
    fn ep(title: &str, url: &str) -> Episode {
        Episode { title: title.into(), url: url.into(), ..Default::default() }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muzoryn-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bulk_download_skips_existing_and_counts_failures() {
        let dir = temp_dir("bulk");
        let eps = vec![ep("Uno", "https://x.test/1.mp3"), ep("Dos", "https://x.test/2.mp3"), ep("Tres", "https://x.test/3.mp3"), ep("Uno", "https://x.test/1b.mp3")];
        std::fs::write(episode_path(&dir, &eps[1].url, &eps[1].title), b"x").unwrap();
        assert_eq!(pending_downloads(&eps, &dir).len(), 2, "omite el que existe y el título repetido");
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let report = download_all(&eps, &dir, &cancel,
            |e, folder| if e.title == "Tres" { Err("sin red".into()) } else {
                let p = episode_path(folder, &e.url, &e.title); std::fs::write(&p, b"audio").unwrap(); Ok(p) },
            |done, total, failed, title| seen.push((done, total, failed, title.to_string())));
        assert_eq!(report, BulkReport { downloaded: 1, failed: 1, cancelled: false });
        assert_eq!(seen, vec![(1, 2, 0, "Uno".to_string()), (2, 2, 1, "Tres".to_string())]);
        // Repetir continúa solo con lo que falló.
        assert_eq!(pending_downloads(&eps, &dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bulk_download_can_be_cancelled() {
        let dir = temp_dir("cancel");
        let eps = vec![ep("A", "https://x.test/a.mp3"), ep("B", "https://x.test/b.mp3"), ep("C", "https://x.test/c.mp3")];
        let cancel = AtomicBool::new(false);
        let report = download_all(&eps, &dir, &cancel,
            |e, folder| { let p = episode_path(folder, &e.url, &e.title); std::fs::write(&p, b"a").unwrap(); Ok(p) },
            |done, _, _, _| if done == 1 { cancel.store(true, Ordering::SeqCst) });
        assert_eq!(report, BulkReport { downloaded: 1, failed: 0, cancelled: true });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("a/b:c"), "a_b_c");
        assert_eq!(safe_file_name("///"), "___");
        assert_eq!(safe_file_name(""), "episodio");
    }
}
