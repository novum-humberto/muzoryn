//! Cliente mínimo de Radio Browser. Solo realiza búsquedas; la reproducción
//! y persistencia siguen usando las estructuras nativas de MUZORYN.
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct StationResult {
    pub uuid: String,
    pub name: String,
    pub url: String,
    pub country: String,
    pub language: String,
    pub tags: String,
    pub codec: String,
    pub bitrate: u32,
}

#[derive(Deserialize)]
struct ApiStation {
    stationuuid: Option<String>,
    name: Option<String>,
    url: Option<String>,
    url_resolved: Option<String>,
    country: Option<String>,
    language: Option<String>,
    tags: Option<String>,
    codec: Option<String>,
    bitrate: Option<u32>,
    lastcheckok: Option<u8>,
}

const PREFIXES: [(&str, &str); 9] = [
    ("country:", "country"), ("país:", "country"), ("pais:", "country"),
    ("language:", "language"), ("idioma:", "language"),
    ("genre:", "tag"), ("género:", "tag"), ("genero:", "tag"), ("tag:", "tag"),
];

fn plain(text: &str) -> String {
    text.trim().to_lowercase().chars().map(|c| match c {
        'á' => 'a', 'é' => 'e', 'í' => 'i', 'ó' => 'o', 'ú' | 'ü' => 'u', 'ñ' => 'n', other => other,
    }).collect()
}

/// Código ISO del país a partir de su nombre en español o inglés, o el propio código de dos letras.
fn country_code(value: &str) -> Option<String> {
    let v = plain(value);
    let code = match v.as_str() {
        "estados unidos" | "eeuu" | "ee.uu." | "ee uu" | "usa" | "united states" => "US",
        "japon" | "japan" => "JP",
        "espana" | "spain" => "ES",
        "mexico" => "MX",
        "cuba" => "CU",
        "argentina" => "AR",
        "chile" => "CL",
        "colombia" => "CO",
        "peru" => "PE",
        "venezuela" => "VE",
        "brasil" | "brazil" => "BR",
        "francia" | "france" => "FR",
        "alemania" | "germany" => "DE",
        "italia" | "italy" => "IT",
        "reino unido" | "inglaterra" | "united kingdom" | "uk" => "GB",
        "canada" => "CA",
        "portugal" => "PT",
        "rusia" | "russia" => "RU",
        "china" => "CN",
        "corea del sur" | "corea" | "south korea" | "korea" => "KR",
        "india" => "IN",
        "australia" => "AU",
        "uruguay" => "UY",
        "ecuador" => "EC",
        "bolivia" => "BO",
        "paraguay" => "PY",
        "republica dominicana" | "dominican republic" => "DO",
        "puerto rico" => "PR",
        "panama" => "PA",
        "costa rica" => "CR",
        "guatemala" => "GT",
        "honduras" => "HN",
        "el salvador" => "SV",
        "nicaragua" => "NI",
        "paises bajos" | "holanda" | "netherlands" => "NL",
        "suecia" | "sweden" => "SE",
        "noruega" | "norway" => "NO",
        "grecia" | "greece" => "GR",
        "turquia" | "turkey" => "TR",
        "sudafrica" | "south africa" => "ZA",
        "irlanda" | "ireland" => "IE",
        "suiza" | "switzerland" => "CH",
        "austria" => "AT",
        "belgica" | "belgium" => "BE",
        "polonia" | "poland" => "PL",
        _ => {
            if v.len() == 2 && v.chars().all(|c| c.is_ascii_alphabetic()) { return Some(v.to_uppercase()); }
            return None;
        }
    };
    Some(code.to_string())
}

/// Nombre del idioma como lo guarda Radio Browser (inglés, minúsculas) a partir del español.
/// Lo que no se reconoce se envía tal cual.
fn language_name(value: &str) -> String {
    let v = plain(value);
    let name = match v.as_str() {
        "espanol" | "castellano" => "spanish",
        "ingles" => "english",
        "frances" => "french",
        "aleman" => "german",
        "italiano" => "italian",
        "portugues" => "portuguese",
        "ruso" => "russian",
        "japones" => "japanese",
        "chino" | "mandarin" => "chinese",
        "coreano" => "korean",
        "arabe" => "arabic",
        "catalan" => "catalan",
        "gallego" => "galician",
        "vasco" | "euskera" => "basque",
        "neerlandes" | "holandes" => "dutch",
        "sueco" => "swedish",
        "griego" => "greek",
        "turco" => "turkish",
        "polaco" => "polish",
        "hindi" => "hindi",
        _ => return value.trim().to_string(),
    };
    name.to_string()
}

/// Etiqueta de género como la usa Radio Browser (inglés) a partir del español.
/// Lo que no se reconoce se envía tal cual (salsa, jazz, rock… ya coinciden).
fn genre_name(value: &str) -> String {
    let v = plain(value);
    let name = match v.as_str() {
        "clasica" | "musica clasica" => "classical",
        "noticias" => "news",
        "deportes" => "sports",
        "regueton" => "reggaeton",
        "latina" | "musica latina" => "latin",
        "electronica" => "electronic",
        "infantil" | "ninos" => "kids",
        "religiosa" | "religion" => "religious",
        "cultura" => "culture",
        "hablada" | "charla" => "talk",
        "baladas" => "ballads",
        "exitos" | "hits" => "top 40",
        "variada" | "variedad" => "variety",
        "folclore" | "folklore" => "folk",
        "tradicional" => "traditional",
        "oldies" | "clasicos" => "oldies",
        _ => return value.trim().to_string(),
    };
    name.to_string()
}

pub(crate) fn encode(value: &str) -> String {
    value.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// Separa la consulta en filtros combinables: texto libre (nombre) más `country:`, `language:` y `genre:`.
fn filters(query: &str) -> Result<Vec<(&'static str, String)>, String> {
    let mut marks: Vec<(usize, usize, &'static str)> = Vec::new();
    let mut prev_space = true;
    for (i, ch) in query.char_indices() {
        if prev_space {
            if let Some((prefix, field)) = PREFIXES.iter().find(|(prefix, _)| query[i..].starts_with(*prefix)) {
                marks.push((i, prefix.len(), *field));
            }
        }
        prev_space = ch.is_whitespace();
    }
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let head = marks.first().map(|m| &query[..m.0]).unwrap_or(query).trim();
    if !head.is_empty() { out.push(("name", head.to_string())); }
    for (n, (start, len, field)) in marks.iter().enumerate() {
        let end = marks.get(n + 1).map(|m| m.0).unwrap_or(query.len());
        let value = query[start + len..end].trim();
        if value.is_empty() { return Err("Escribe un valor después del filtro (country:, language: o genre:)".into()); }
        out.push((*field, value.to_string()));
    }
    Ok(out)
}

/// Busca emisoras activas por nombre, país, idioma o etiquetas; los filtros se pueden combinar
/// (por ejemplo `genre:jazz country:japón`).
pub fn search(query: &str) -> Result<Vec<StationResult>, String> {
    let query = query.trim();
    if query.is_empty() { return Err("Escribe un término para buscar emisoras".into()); }
    let mut params: Vec<String> = Vec::new();
    for (field, value) in filters(query)? {
        let (key, val) = if field == "country" {
            match country_code(&value) { Some(code) => ("countrycode", code), None => ("country", value) }
        } else if field == "language" {
            (field, language_name(&value))
        } else if field == "tag" {
            (field, genre_name(&value))
        } else { (field, value) };
        params.push(format!("{key}={}", encode(&val)));
    }
    let url = format!("https://de1.api.radio-browser.info/json/stations/search?{}&limit=40&order=clickcount&reverse=true&hidebroken=true", params.join("&"));
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(12)).build();
    let response = agent.get(&url)
        .set("User-Agent", concat!("MUZORYN/", env!("CARGO_PKG_VERSION"), " RadioBrowser"))
        .call().map_err(|e| format!("Radio Browser no disponible: {e}"))?;
    let body = response.into_string().map_err(|e| format!("No se pudo leer la respuesta de Radio Browser: {e}"))?;
    let rows: Vec<ApiStation> = serde_json::from_str(&body).map_err(|e| format!("Respuesta de Radio Browser no válida: {e}"))?;
    let mut out = Vec::new();
    for row in rows {
        if row.lastcheckok == Some(0) { continue; }
        let name = row.name.unwrap_or_default().trim().to_string();
        let url = row.url_resolved.filter(|s| !s.trim().is_empty()).or(row.url).unwrap_or_default();
        if name.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) { continue; }
        out.push(StationResult {
            uuid: row.stationuuid.unwrap_or_default(),
            name, url,
            country: row.country.unwrap_or_default(),
            language: row.language.unwrap_or_default(),
            tags: row.tags.unwrap_or_default(),
            codec: row.codec.unwrap_or_default(),
            bitrate: row.bitrate.unwrap_or(0),
        });
    }
    Ok(out)
}

/// Avisa a Radio Browser de que se reproduce una emisora (cuenta un «clic» de popularidad).
/// Solo envía el `stationuuid`; los fallos se ignoran porque no afectan a la reproducción.
pub fn register_click(uuid: &str) {
    if uuid.is_empty() || !uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') { return; }
    let url = format!("https://de1.api.radio-browser.info/json/url/{uuid}");
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).build();
    let _ = agent.get(&url)
        .set("User-Agent", concat!("MUZORYN/", env!("CARGO_PKG_VERSION"), " RadioBrowser"))
        .call();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combines_genre_and_country() {
        let f = filters("genre:jazz country:japón").unwrap();
        assert_eq!(f, vec![("tag", "jazz".to_string()), ("country", "japón".to_string())]);
        assert_eq!(country_code("japón").as_deref(), Some("JP"));
        assert_eq!(country_code("Estados Unidos").as_deref(), Some("US"));
        assert_eq!(country_code("xx").as_deref(), Some("XX"));
        assert_eq!(country_code("Narnia"), None);
    }

    #[test]
    fn spanish_values_are_translated_for_the_api() {
        assert_eq!(language_name("Español"), "spanish");
        assert_eq!(language_name(" INGLÉS "), "english");
        assert_eq!(language_name("Swahili"), "Swahili");
        assert_eq!(genre_name("Clásica"), "classical");
        assert_eq!(genre_name("salsa"), "salsa");
        let f = filters("género:salsa país:cuba idioma:Español").unwrap();
        assert_eq!(f, vec![("tag", "salsa".to_string()), ("country", "cuba".to_string()), ("language", "Español".to_string())]);
        assert_eq!(country_code("Cuba").as_deref(), Some("CU"));
    }

    #[test]
    fn click_ignores_invalid_uuid() {
        // No debe intentar ninguna petición (ni entrar en pánico) con valores no válidos.
        register_click("");
        register_click("../etc/passwd");
    }

    #[test]
    fn plain_name_mixed_and_errors() {
        assert_eq!(filters("jazz fm").unwrap(), vec![("name", "jazz fm".to_string())]);
        assert_eq!(filters("salsa genre:tropical").unwrap().len(), 2);
        assert!(filters("genre:").is_err());
    }
}
