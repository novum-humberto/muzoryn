//! Directorio de podcasts (Apple Podcasts, API pública y sin clave) para buscar, ver los más
//! escuchados y navegar por categorías. Las consultas se hacen contra la tienda de un país
//! hispanohablante y con términos en español, de modo que los resultados sean podcasts en español.

use std::{collections::HashSet, time::Duration};

use serde::Deserialize;

/// Tienda por defecto (código de país de 2 letras). Se cambia con `podcast_country`.
pub const DEFAULT_COUNTRY: &str = "mx";
const MAX_RESULTS: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowResult {
    pub title: String,
    pub author: String,
    pub feed_url: String,
    pub genre: String,
    pub episodes: u32,
}

/// Categoría de Apple Podcasts: identificador, nombre en español y término de búsqueda.
pub struct Category {
    pub id: &'static str,
    pub name: &'static str,
}

pub const CATEGORIES: [Category; 19] = [
    Category {
        id: "1489",
        name: "Noticias",
    },
    Category {
        id: "1318",
        name: "Tecnología",
    },
    Category {
        id: "1488",
        name: "Crimen real",
    },
    Category {
        id: "1303",
        name: "Comedia",
    },
    Category {
        id: "1324",
        name: "Sociedad y cultura",
    },
    Category {
        id: "1487",
        name: "Historia",
    },
    Category {
        id: "1533",
        name: "Ciencia",
    },
    Category {
        id: "1512",
        name: "Salud y bienestar",
    },
    Category {
        id: "1321",
        name: "Negocios",
    },
    Category {
        id: "1310",
        name: "Música",
    },
    Category {
        id: "1545",
        name: "Deportes",
    },
    Category {
        id: "1301",
        name: "Arte",
    },
    Category {
        id: "1304",
        name: "Educación",
    },
    Category {
        id: "1483",
        name: "Ficción",
    },
    Category {
        id: "1309",
        name: "TV y cine",
    },
    Category {
        id: "1314",
        name: "Religión y espiritualidad",
    },
    Category {
        id: "1305",
        name: "Niños y familia",
    },
    Category {
        id: "1502",
        name: "Ocio",
    },
    Category {
        id: "1511",
        name: "Gobierno",
    },
];

/// Código de país válido (2 letras, en minúsculas) o el de por defecto.
pub fn normalize_country(value: &str) -> String {
    let v = value.trim().to_lowercase();
    if v.len() == 2 && v.chars().all(|c| c.is_ascii_lowercase()) {
        v
    } else {
        DEFAULT_COUNTRY.to_string()
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    results: Option<Vec<ApiShow>>,
}

#[derive(Deserialize)]
struct ApiShow {
    #[serde(rename = "collectionName")]
    name: Option<String>,
    #[serde(rename = "artistName")]
    artist: Option<String>,
    #[serde(rename = "feedUrl")]
    feed: Option<String>,
    #[serde(rename = "primaryGenreName")]
    genre: Option<String>,
    #[serde(rename = "trackCount")]
    count: Option<u32>,
    #[serde(rename = "collectionId")]
    id: Option<u64>,
}

#[derive(Deserialize)]
struct ChartResponse {
    feed: Option<ChartFeed>,
}
#[derive(Deserialize)]
struct ChartFeed {
    results: Option<Vec<ChartEntry>>,
}
#[derive(Deserialize)]
struct ChartEntry {
    id: Option<String>,
}

fn valid_http(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    matches!(rest, Some(r) if !r.is_empty() && !r.starts_with('/') && !r.split('/').next().unwrap_or("").contains('@'))
}

/// Convierte la respuesta JSON del directorio en shows con feed válido y sin duplicados.
/// Devuelve también el id de cada uno para poder ordenar las listas por ranking.
fn parse_shows(body: &str) -> Result<Vec<(u64, ShowResult)>, String> {
    let response: SearchResponse = serde_json::from_str(body)
        .map_err(|e| format!("Respuesta del directorio no válida: {e}"))?;
    let rows = response
        .results
        .ok_or("Respuesta del directorio sin resultados")?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let feed = row.feed.unwrap_or_default().trim().to_string();
        if !valid_http(&feed) || !seen.insert(feed.clone()) {
            continue;
        }
        let title = row.name.unwrap_or_default().trim().to_string();
        out.push((
            row.id.unwrap_or(0),
            ShowResult {
                title: if title.is_empty() {
                    "Sin título".into()
                } else {
                    title
                },
                author: row.artist.unwrap_or_default().trim().to_string(),
                feed_url: feed,
                genre: row.genre.unwrap_or_default().trim().to_string(),
                episodes: row.count.unwrap_or(0),
            },
        ));
    }
    Ok(out)
}

fn parse_chart_ids(body: &str) -> Result<Vec<String>, String> {
    let chart: ChartResponse =
        serde_json::from_str(body).map_err(|e| format!("Ranking no válido: {e}"))?;
    let rows = chart
        .feed
        .and_then(|f| f.results)
        .ok_or("El ranking no trae resultados")?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.id)
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
        .collect())
}

/// Direcciones del directorio; se pueden sustituir en las pruebas.
pub struct Directory {
    pub search_base: String,
    pub charts_base: String,
}

impl Default for Directory {
    fn default() -> Self {
        Self {
            search_base: "https://itunes.apple.com".into(),
            charts_base: "https://rss.marketingtools.apple.com/api/v2".into(),
        }
    }
}

impl Directory {
    fn get(&self, url: &str) -> Result<String, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .redirects(0)
            .build();
        let response = crate::media::get_checked(
            &agent,
            url,
            &[(
                "User-Agent",
                concat!("MUZORYN/", env!("CARGO_PKG_VERSION"), " podcasts"),
            )],
        )
        .map_err(|e| format!("Directorio de podcasts no disponible: {e}"))?;
        response
            .into_string()
            .map_err(|e| format!("No se pudo leer la respuesta del directorio: {e}"))
    }

    /// Busca podcasts por nombre o tema; con `genre_id` se limita a una categoría.
    pub fn search(
        &self,
        term: &str,
        genre_id: Option<&str>,
        country: &str,
    ) -> Result<Vec<ShowResult>, String> {
        let term = term.trim();
        if term.is_empty() {
            return Err("Escribe qué podcast quieres buscar".into());
        }
        let mut url = format!(
            "{}/search?term={}&media=podcast&entity=podcast&limit={MAX_RESULTS}&country={}&lang=es_es",
            self.search_base.trim_end_matches('/'), crate::radio_browser::encode(term), normalize_country(country));
        if let Some(id) =
            genre_id.filter(|g| g.chars().all(|c| c.is_ascii_digit()) && !g.is_empty())
        {
            url.push_str(&format!("&genreId={id}"));
        }
        Ok(parse_shows(&self.get(&url)?)?
            .into_iter()
            .map(|(_, s)| s)
            .collect())
    }

    /// Los podcasts más escuchados de la tienda del país, en orden de ranking.
    pub fn top(&self, country: &str) -> Result<Vec<ShowResult>, String> {
        let country = normalize_country(country);
        let chart_url = format!(
            "{}/{country}/podcasts/top/{MAX_RESULTS}/podcasts.json",
            self.charts_base.trim_end_matches('/')
        );
        let ids = parse_chart_ids(&self.get(&chart_url)?)?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{}/lookup?id={}&entity=podcast&country={country}&lang=es_es",
            self.search_base.trim_end_matches('/'),
            ids.join(",")
        );
        let found = parse_shows(&self.get(&url)?)?;
        let mut ranked = Vec::new();
        for id in &ids {
            if let Some((_, show)) = found.iter().find(|(sid, _)| sid.to_string() == *id) {
                ranked.push(show.clone());
            }
        }
        Ok(ranked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };

    const SEARCH: &str = r#"{"resultCount":4,"results":[
        {"collectionId":11,"collectionName":"Radio Ambulante","artistName":"NPR","feedUrl":"https://feeds.example.org/ra.xml","primaryGenreName":"Noticias","trackCount":210},
        {"collectionId":12,"collectionName":"Sin feed","artistName":"X","primaryGenreName":"Noticias"},
        {"collectionId":13,"collectionName":"Duplicado","feedUrl":"https://feeds.example.org/ra.xml"},
        {"collectionId":14,"collectionName":"  ","feedUrl":"http://u:p@feeds.example.org/x.xml"},
        {"collectionId":15,"collectionName":"Hoy en la historia","artistName":"Ana","feedUrl":"http://feeds.example.org/h","trackCount":5}]}"#;

    #[test]
    fn parse_keeps_valid_unique_feeds() {
        let shows = parse_shows(SEARCH).unwrap();
        let titles: Vec<_> = shows.iter().map(|(_, s)| s.title.as_str()).collect();
        assert_eq!(titles, ["Radio Ambulante", "Hoy en la historia"]);
        assert_eq!(shows[0].1.episodes, 210);
        assert!(parse_shows("{}").is_err() && parse_shows("no json").is_err());
    }

    #[test]
    fn chart_ids_are_numeric_only() {
        let ids = parse_chart_ids(
            r#"{"feed":{"results":[{"id":"11"},{"id":"x1"},{"id":" 15 "},{"name":"sin id"}]}}"#,
        )
        .unwrap();
        assert_eq!(ids, ["11", "15"]);
        assert!(parse_chart_ids(r#"{"feed":{}}"#).is_err());
    }

    #[test]
    fn country_is_normalized() {
        assert_eq!(normalize_country(" ES "), "es");
        assert_eq!(normalize_country("mexico"), DEFAULT_COUNTRY);
        assert_eq!(normalize_country(""), DEFAULT_COUNTRY);
    }

    #[test]
    fn categories_have_unique_numeric_ids() {
        let ids: HashSet<_> = CATEGORIES.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), CATEGORIES.len());
        assert!(CATEGORIES
            .iter()
            .all(|c| c.id.chars().all(|ch| ch.is_ascii_digit())));
    }

    /// Servidor local mínimo: responde según la ruta y guarda las peticiones recibidas.
    fn serve() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let seen = log.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                seen.lock().unwrap().push(path.clone());
                let body = if path.contains("/podcasts/top/") {
                    r#"{"feed":{"results":[{"id":"15"},{"id":"11"}]}}"#.to_string()
                } else {
                    SEARCH.to_string() // el orden de la respuesta no es el del ranking
                };
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (base, log)
    }

    #[test]
    fn search_asks_for_spanish_storefront_and_category() {
        let (base, log) = serve();
        let dir = Directory {
            search_base: base.clone(),
            charts_base: base,
        };
        let shows = dir
            .search("historia de méxico", Some("1487"), "ES")
            .unwrap();
        assert_eq!(shows.len(), 2);
        let path = log.lock().unwrap()[0].clone();
        assert!(path.contains("term=historia%20de%20m%C3%A9xico"), "{path}");
        assert!(
            path.contains("country=es")
                && path.contains("lang=es_es")
                && path.contains("genreId=1487"),
            "{path}"
        );
        assert!(dir.search("  ", None, "mx").is_err());
        dir.search("x", Some("14;87"), "mx").unwrap();
        assert!(!log.lock().unwrap()[1].contains("genreId"));
    }

    #[test]
    fn top_keeps_chart_order() {
        let (base, log) = serve();
        let dir = Directory {
            search_base: base.clone(),
            charts_base: base,
        };
        let shows = dir.top("mx").unwrap();
        let titles: Vec<_> = shows.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Hoy en la historia", "Radio Ambulante"]);
        let log = log.lock().unwrap();
        assert!(log[0].starts_with("/mx/podcasts/top/"));
        assert!(log[1].starts_with("/lookup?id=15,11") && log[1].contains("country=mx"));
    }

    #[test]
    fn unreachable_directory_reports_error() {
        let dir = Directory {
            search_base: "http://127.0.0.1:1".into(),
            charts_base: "http://127.0.0.1:1".into(),
        };
        assert!(dir
            .search("a", None, "mx")
            .unwrap_err()
            .contains("no disponible"));
    }
}
