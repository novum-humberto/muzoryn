//! Importación/exportación de listas de emisoras (independiente de las listas musicales).
use crate::config::Station;
use std::collections::BTreeMap;

pub const FORMATS: [&str; 5] = ["m3u", "m3u8", "pls", "xspf", "json"];

pub fn import(text: &str, format: &str) -> Result<Vec<Station>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let normalized = if text.contains("\\n") && !text.contains('\n') {
        text.replace("\\r\\n", "\n").replace("\\n", "\n")
    } else {
        text.to_string()
    };
    let text = normalized.as_str();
    let mut stations = Vec::new();
    match format.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "m3u" | "m3u8" => {
            let mut name = String::new();
            for line in text.lines().map(str::trim) {
                if let Some(info) = line.strip_prefix("#EXTINF:") {
                    name = info
                        .split_once(',')
                        .map(|(_, n)| n.trim().to_string())
                        .unwrap_or_default();
                } else if !line.is_empty() && !line.starts_with('#') && valid_url(line) {
                    stations.push(Station::new(
                        if name.is_empty() {
                            line.to_string()
                        } else {
                            name.clone()
                        },
                        line,
                        0,
                    ));
                    name.clear();
                }
            }
        }
        "pls" => {
            let mut rows = BTreeMap::<usize, (String, String)>::new();
            for line in text.lines().map(str::trim) {
                if let Some((k, v)) = line.split_once('=') {
                    let (kind, n) = if let Some(n) = k.strip_prefix("File") {
                        (0, n)
                    } else if let Some(n) = k.strip_prefix("Title") {
                        (1, n)
                    } else {
                        continue;
                    };
                    if let Ok(i) = n.parse::<usize>() {
                        let row = rows.entry(i).or_default();
                        if kind == 0 {
                            row.0 = v.trim().to_string();
                        } else {
                            row.1 = v.trim().to_string();
                        }
                    }
                }
            }
            for (_, (url, title)) in rows {
                if valid_url(&url) {
                    stations.push(Station::new(
                        if title.is_empty() { url.clone() } else { title },
                        url,
                        0,
                    ));
                }
            }
        }
        "json" => {
            let values: Vec<Station> = serde_json::from_str(text)
                .map_err(|e| format!("JSON de emisoras no válido: {e}"))?;
            stations = values.into_iter().filter(|s| valid_url(&s.url)).collect();
        }
        "xspf" => {
            let mut rest = text;
            while let Some(pos) = rest.find("<track>") {
                rest = &rest[pos + 7..];
                let Some(end) = rest.find("</track>") else {
                    break;
                };
                let block = &rest[..end];
                let title = xml_value(block, "title").unwrap_or_default();
                if let Some(url) = xml_value(block, "location") {
                    if valid_url(&url) {
                        stations.push(Station::new(
                            if title.is_empty() { url.clone() } else { title },
                            url,
                            0,
                        ));
                    }
                }
                rest = &rest[end + 8..];
            }
        }
        other => {
            return Err(format!(
                "Formato de radio no admitido: {other}. Usa {}",
                FORMATS.join(", ")
            ))
        }
    }
    // Deduplicar por URL normalizada, preservando el primer nombre/orden.
    let mut seen = std::collections::HashSet::new();
    stations.retain(|s| seen.insert(s.url.trim().trim_end_matches('/').to_ascii_lowercase()));
    Ok(stations)
}

pub fn export(stations: &[Station], format: &str) -> Result<String, String> {
    let f = format.trim_start_matches('.').to_ascii_lowercase();
    let valid: Vec<&Station> = stations.iter().filter(|s| valid_url(&s.url)).collect();
    match f.as_str() {
        "m3u" | "m3u8" => {
            let mut out = String::from("#EXTM3U\n");
            for s in valid {
                let name = s.name.replace(['\n', '\r'], " ");
                out.push_str(&format!("#EXTINF:-1,{name}\n{}\n", s.url));
            }
            Ok(out)
        }
        "pls" => {
            let mut out = format!("[playlist]\nNumberOfEntries={}\nVersion=2\n", valid.len());
            for (i, s) in valid.iter().enumerate() {
                let name = s.name.replace(['\n', '\r'], " ");
                out.push_str(&format!(
                    "File{}={}\nTitle{}={}\nLength{}=-1\n",
                    i + 1,
                    s.url,
                    i + 1,
                    name,
                    i + 1
                ));
            }
            Ok(out)
        }
        "json" => serde_json::to_string_pretty(&valid).map_err(|e| e.to_string()),
        "xspf" => {
            let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\"><title>MUZORYN Radio</title><trackList>\n");
            for s in valid {
                out.push_str(&format!(
                    "<track><title>{}</title><location>{}</location></track>\n",
                    xml_escape(&s.name),
                    xml_escape(&s.url)
                ));
            }
            out.push_str("</trackList></playlist>\n");
            Ok(out)
        }
        _ => Err(format!(
            "Formato no admitido: {f}. Usa {}",
            FORMATS.join(", ")
        )),
    }
}

fn valid_url(s: &str) -> bool {
    let s = s.trim().to_ascii_lowercase();
    s.starts_with("http://") || s.starts_with("https://")
}
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn xml_value(s: &str, tag: &str) -> Option<String> {
    let a = format!("<{tag}>");
    let b = format!("</{tag}>");
    let v = s.split_once(&a)?.1.split_once(&b)?.0.trim();
    Some(
        v.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn m3u_roundtrip() {
        let v = vec![Station::new("Radio Uno", "https://radio.example/live", 128)];
        let x = export(&v, "m3u").unwrap();
        let got = import(&x, "m3u").unwrap();
        assert_eq!(got[0].name, "Radio Uno");
        assert_eq!(got[0].url, v[0].url);
    }
    #[test]
    fn pls_import_order_and_invalid_filter() {
        let x="[playlist]\nFile2=https://b.test/live\nTitle2=Dos\nFile1=https://a.test/live\nTitle1=Uno\nFile3=ftp://bad.test";
        let got = import(x, "pls").unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].name, "Uno");
    }
}
