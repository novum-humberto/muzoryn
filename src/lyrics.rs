//! Letras sincronizadas: lectura de archivos LRC junto a la pista.
use std::{fs, path::Path};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lyrics {
    /// (milisegundos, texto). Si no está sincronizada, todos los tiempos son 0.
    pub lines: Vec<(i64, String)>,
    pub synced: bool,
}

impl Lyrics {
    /// Busca `pista.lrc` junto al archivo de audio.
    pub fn load_for(audio: &Path) -> Option<Lyrics> {
        let text = fs::read_to_string(audio.with_extension("lrc")).ok()?;
        let lyrics = parse(&text);
        if lyrics.lines.is_empty() { None } else { Some(lyrics) }
    }

    /// Índice de la línea activa para una posición en milisegundos.
    pub fn current(&self, position_ms: i64) -> Option<usize> {
        if !self.synced || self.lines.is_empty() { return None; }
        let mut found = None;
        for (i, (time, _)) in self.lines.iter().enumerate() {
            if *time <= position_ms { found = Some(i); } else { break; }
        }
        found
    }
}

fn parse_stamp(stamp: &str) -> Option<i64> {
    let (minutes, rest) = stamp.split_once(':')?;
    let minutes: i64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once('.') {
        Some((s, f)) => (s, f),
        None => (rest, ""),
    };
    let seconds: i64 = seconds.trim().parse().ok()?;
    let millis: i64 = if fraction.is_empty() {
        0
    } else {
        let digits: String = fraction.chars().filter(|c| c.is_ascii_digit()).take(3).collect();
        let value: i64 = digits.parse().ok()?;
        match digits.len() { 1 => value * 100, 2 => value * 10, _ => value }
    };
    Some((minutes * 60 + seconds) * 1000 + millis)
}

pub fn parse(text: &str) -> Lyrics {
    let mut timed: Vec<(i64, String)> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    for raw in text.lines() {
        let mut line = raw.trim().trim_start_matches('\u{feff}');
        let mut stamps = Vec::new();
        let mut metadata = false;
        while let Some(rest) = line.strip_prefix('[') {
            let Some(end) = rest.find(']') else { break };
            match parse_stamp(&rest[..end]) {
                Some(ms) => stamps.push(ms),
                None => { metadata = true; break; }
            }
            line = rest[end + 1..].trim_start();
        }
        if metadata { continue; }
        if stamps.is_empty() {
            if !line.is_empty() { plain.push(line.to_string()); }
        } else {
            for ms in stamps { timed.push((ms, line.trim().to_string())); }
        }
    }
    if !timed.is_empty() {
        timed.sort_by_key(|(ms, _)| *ms);
        Lyrics { lines: timed, synced: true }
    } else {
        Lyrics { lines: plain.into_iter().map(|t| (0, t)).collect(), synced: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_timestamps_and_ignores_metadata() {
        let l = parse("[ar:Alguien]\n[00:01.50]Primera\n[00:03.00][00:09.25]Segunda\n");
        assert!(l.synced);
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[0], (1500, "Primera".to_string()));
        assert_eq!(l.lines[2].0, 9250);
    }
    #[test]
    fn current_line_follows_position() {
        let l = parse("[00:01.00]a\n[00:05.00]b\n");
        assert_eq!(l.current(500), None);
        assert_eq!(l.current(1000), Some(0));
        assert_eq!(l.current(6000), Some(1));
    }
    #[test]
    fn unsynced_text_is_kept() {
        let l = parse("hola\nmundo");
        assert!(!l.synced);
        assert_eq!(l.lines.len(), 2);
        assert_eq!(l.current(1000), None);
    }
}
