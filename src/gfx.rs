//! Carátulas de alta resolución: protocolo Kitty, iTerm2 o Sixel; medios bloques como respaldo.
//! Autodetecta por variables de entorno; `MUZORYN_GRAPHICS=kitty|iterm|sixel|blocks` lo fuerza.
use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    sync::{Arc, Mutex, OnceLock},
};
use image::{imageops::FilterType, RgbImage};
use ratatui::layout::Rect;
use crate::cover::Thumb;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Proto { Blocks, Kitty, Iterm, Sixel }

fn detect() -> Proto {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    match env("MUZORYN_GRAPHICS").to_lowercase().as_str() {
        "kitty" => return Proto::Kitty,
        "iterm" | "iterm2" => return Proto::Iterm,
        "sixel" => return Proto::Sixel,
        "blocks" | "off" | "none" => return Proto::Blocks,
        _ => {}
    }
    let (term, prog) = (env("TERM"), env("TERM_PROGRAM"));
    // Los multiplexores no reenvían los gráficos sin configuración extra.
    if !env("TMUX").is_empty() || term.starts_with("screen") { return Proto::Blocks; }
    if term == "xterm-kitty" || term == "xterm-ghostty" || prog == "ghostty" || prog == "WezTerm"
        || !env("KITTY_WINDOW_ID").is_empty() { Proto::Kitty }
    else if prog == "iTerm.app" || prog == "mintty" { Proto::Iterm }
    else if term.starts_with("foot") || term.starts_with("mlterm") || !env("KONSOLE_VERSION").is_empty() { Proto::Sixel }
    else { Proto::Blocks }
}

pub fn proto() -> Proto {
    static P: OnceLock<Proto> = OnceLock::new();
    *P.get_or_init(detect)
}

#[derive(Clone)]
struct Placement { thumb: Arc<Thumb>, x: u16, y: u16, cols: u16, rows: u16 }

impl Placement {
    fn key(&self) -> (u32, u16, u16, u16, u16) { (self.thumb.id, self.x, self.y, self.cols, self.rows) }
}

#[derive(Default)]
struct State {
    cell: (f32, f32),
    pending: Vec<Placement>,
    last: Vec<Placement>,
    sent: HashSet<u32>,
    emit_next: bool,
    invalid: bool,
}

fn st() -> std::sync::MutexGuard<'static, State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State::default())).lock().unwrap_or_else(|e| e.into_inner())
}

/// Llamar justo antes de `terminal.draw`.
pub fn begin_frame() {
    if proto() == Proto::Blocks { return; }
    let cell = match crossterm::terminal::window_size() {
        Ok(w) if w.width > 0 && w.height > 0 && w.columns > 0 && w.rows > 0 =>
            (w.width as f32 / w.columns as f32, w.height as f32 / w.rows as f32),
        _ => (8.0, 16.0),
    };
    let mut s = st();
    s.cell = cell;
    s.pending.clear();
}

/// Fuerza reenviar las imágenes (p. ej. tras redimensionar).
pub fn invalidate() {
    let mut state = st();
    state.invalid = true;
    // El compositor del terminal puede conservar placements con la geometría anterior
    // tras un resize de Hyprland. Forzamos una retransmisión completa en el siguiente frame.
}

/// Registra una carátula en `area`; devuelve el rectángulo de celdas que ocupará, ajustado a su proporción.
pub fn register(thumb: &Arc<Thumb>, area: Rect) -> Option<Rect> {
    if proto() == Proto::Blocks || area.width == 0 || area.height == 0 || thumb.hi_w == 0 || thumb.hi_h == 0 { return None; }
    let mut s = st();
    let (cw, ch) = if s.cell.0 > 0.0 && s.cell.1 > 0.0 { s.cell } else { (8.0, 16.0) };
    let (iw, ih) = (thumb.hi_w as f32, thumb.hi_h as f32);
    let k = (area.width as f32 * cw / iw).min(area.height as f32 * ch / ih);
    let cols = ((iw * k / cw).round() as u16).clamp(1, area.width);
    let rows = ((ih * k / ch).round() as u16).clamp(1, area.height);
    let x = area.x + (area.width - cols) / 2;
    let y = area.y + (area.height - rows) / 2;
    s.pending.push(Placement { thumb: thumb.clone(), x, y, cols, rows });
    Some(Rect::new(x, y, cols, rows))
}

/// Llamar justo después de `terminal.draw`. Devuelve `true` si hay que limpiar la pantalla
/// (`terminal.clear()`) y redibujar: sólo ocurre con iTerm2/Sixel, que no tienen capas.
pub fn flush<W: Write>(out: &mut W) -> io::Result<bool> {
    let p = proto();
    if p == Proto::Blocks { return Ok(false); }
    let mut s = st();
    let same = s.pending.len() == s.last.len()
        && s.pending.iter().zip(&s.last).all(|(a, b)| a.key() == b.key());
    if p == Proto::Kitty {
        if same && !s.invalid { return Ok(false); }
        let force_reload = s.invalid;
        s.invalid = false;
        let mut o = String::from("\x1b7\x1b_Ga=d,d=a,q=2\x1b\\");
        if force_reload {
            // Elimina también los datos de imagen antiguos: evita que Kitty/Hyprland
            // mantenga una textura con tamaño/posición previa al redimensionado.
            o.push_str("\x1b_Ga=d,d=A,q=2\x1b\\");
            s.sent.clear();
        }
        if s.sent.len() > 256 { o.push_str("\x1b_Ga=d,d=A,q=2\x1b\\"); s.sent.clear(); }
        let pending = s.pending.clone();
        for (i, pl) in pending.iter().enumerate() {
            let id = pl.thumb.id;
            if s.sent.insert(id) {
                let b64 = base64(&pl.thumb.png);
                let n = b64.len().div_ceil(4096);
                for (j, chunk) in b64.as_bytes().chunks(4096).enumerate() {
                    let more = (j + 1 < n) as u8;
                    if j == 0 { o.push_str(&format!("\x1b_Ga=t,f=100,t=d,i={id},q=2,m={more};")); }
                    else { o.push_str(&format!("\x1b_Gm={more},q=2;")); }
                    o.push_str(std::str::from_utf8(chunk).unwrap_or(""));
                    o.push_str("\x1b\\");
                }
            }
            o.push_str(&format!("\x1b[{};{}H\x1b_Ga=p,i={id},p={},c={},r={},C=1,z=-1073741825,q=2\x1b\\",
                pl.y + 1, pl.x + 1, i + 1, pl.cols, pl.rows));
        }
        o.push_str("\x1b8");
        out.write_all(o.as_bytes())?;
        out.flush()?;
        s.last = pending;
        return Ok(false);
    }
    // iTerm2 / Sixel: la imagen vive en las celdas; al cambiar hay que limpiar y redibujar.
    if s.emit_next && same {
        s.emit_next = false;
        let cell = s.cell;
        let mut o = String::from("\x1b7");
        if p == Proto::Sixel { o.push_str("\x1b[?80l"); }
        for pl in &s.last {
            o.push_str(&format!("\x1b[{};{}H", pl.y + 1, pl.x + 1));
            if p == Proto::Iterm {
                o.push_str(&format!("\x1b]1337;File=inline=1;size={};width={};height={};preserveAspectRatio=0:{}\x07",
                    pl.thumb.png.len(), pl.cols, pl.rows, base64(&pl.thumb.png)));
            } else if let Ok(img) = image::load_from_memory(&pl.thumb.png) {
                let (pw, ph) = (((pl.cols as f32 * cell.0) as u32).max(1), ((pl.rows as f32 * cell.1) as u32).saturating_sub(1).max(1));
                o.push_str(&sixel(&img.resize_exact(pw, ph, FilterType::Lanczos3).to_rgb8()));
            }
        }
        o.push_str("\x1b8");
        out.write_all(o.as_bytes())?;
        out.flush()?;
        return Ok(false);
    }
    if !same || s.invalid {
        s.invalid = false;
        s.last = s.pending.clone();
        s.emit_next = !s.last.is_empty();
        return Ok(true);
    }
    Ok(false)
}

/// Borra las imágenes del terminal al salir.
pub fn cleanup<W: Write>(out: &mut W) -> io::Result<()> {
    if proto() == Proto::Kitty { out.write_all(b"\x1b_Ga=d,d=A,q=2\x1b\\")?; out.flush()?; }
    Ok(())
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut o = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        o.push(T[(n >> 18) as usize & 63] as char);
        o.push(T[(n >> 12) as usize & 63] as char);
        o.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        o.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    o
}

/// Codificador Sixel: paleta fija de 252 colores (6×7×6) con tramado ordenado.
fn sixel(img: &RgbImage) -> String {
    const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];
    const LV: [u32; 3] = [6, 7, 6];
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut idx = vec![0u16; w * h];
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x as u32, y as u32).0;
            let d = BAYER[(y % 4) * 4 + x % 4] / 16.0 - 0.47;
            let q = |v: u8, n: u32| ((v as f32 / 255.0 * (n - 1) as f32 + d).round().clamp(0.0, (n - 1) as f32)) as u32;
            idx[y * w + x] = ((q(p[0], LV[0]) * LV[1] + q(p[1], LV[1])) * LV[2] + q(p[2], LV[2])) as u16;
        }
    }
    let mut o = format!("\x1bPq\"1;1;{w};{h}");
    for i in 0..(LV[0] * LV[1] * LV[2]) {
        let (r, g, b) = (i / (LV[1] * LV[2]), (i / LV[2]) % LV[1], i % LV[2]);
        o.push_str(&format!("#{i};2;{};{};{}", r * 100 / (LV[0] - 1), g * 100 / (LV[1] - 1), b * 100 / (LV[2] - 1)));
    }
    for band in (0..h).step_by(6) {
        let mut rows: HashMap<u16, Vec<u8>> = HashMap::new();
        for dy in 0..(h - band).min(6) {
            for x in 0..w {
                rows.entry(idx[(band + dy) * w + x]).or_insert_with(|| vec![0u8; w])[x] |= 1 << dy;
            }
        }
        let mut keys: Vec<u16> = rows.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            o.push_str(&format!("#{k}"));
            let r = &rows[&k];
            let mut i = 0;
            while i < w {
                let mut j = i;
                while j < w && r[j] == r[i] { j += 1; }
                let ch = (63 + r[i]) as char;
                if j - i > 3 { o.push_str(&format!("!{}{ch}", j - i)); } else { for _ in i..j { o.push(ch); } }
                i = j;
            }
            o.push('$');
        }
        o.push('-');
    }
    o.push_str("\x1b\\");
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
    #[test]
    fn sixel_has_header_and_terminator() {
        let s = sixel(&RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30])));
        assert!(s.starts_with("\x1bPq\"1;1;8;8") && s.ends_with("\x1b\\"));
    }
}
