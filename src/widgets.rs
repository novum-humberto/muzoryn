//! Widgets propios: visualizador de espectro y portadas generadas.
use crate::{
    cover::Thumb,
    theme::{mix, Theme},
};
use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Estilos de visualizador (el estilo clásico de barras con degradado se eliminó).
pub const VIZ_NAMES: [&str; 10] = [
    "Segmentos",
    "Mosaico",
    "Led",
    "Medidor",
    "Matriz",
    "Espejo",
    "Braille",
    "Onda",
    "Pulso",
    "Aurora",
];

/// Visualizador de espectro; `style` indexa `VIZ_NAMES` (se toma módulo 10).
pub struct Visualizer<'a> {
    pub bands: &'a [f32],
    pub theme: &'a Theme,
    pub style: usize,
}

fn noise(a: usize, b: usize) -> f32 {
    let mut h = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    ((h >> 40) as f32) / 16_777_216.0
}

/// Valor interpolado de la banda para la posición `i` de `n`.
fn sample(bands: &[f32], i: usize, n: usize) -> f32 {
    if bands.len() == 1 || n < 2 {
        return bands[0].clamp(0.0, 1.0);
    }
    let pos = i as f32 * (bands.len() - 1) as f32 / (n - 1) as f32;
    let (a, b) = (
        pos.floor() as usize,
        (pos.floor() as usize + 1).min(bands.len() - 1),
    );
    (bands[a] + (bands[b] - bands[a]) * pos.fract()).clamp(0.0, 1.0)
}

fn zone(t: &Theme, f: f32) -> Color {
    let c = if f < 0.6 {
        t.ok.fg
    } else if f < 0.85 {
        t.playback.fg
    } else {
        t.error.fg
    };
    c.unwrap_or(t.accent)
}

/// Escribe en (x, fila contada desde abajo).
fn put(buf: &mut Buffer, a: Rect, x: usize, row: usize, ch: char, fg: Color) {
    if x >= a.width as usize || row >= a.height as usize {
        return;
    }
    buf[(a.x + x as u16, a.y + a.height - 1 - row as u16)]
        .set_char(ch)
        .set_fg(fg);
}

/// Escribe en (x, fila contada desde arriba).
fn put_top(buf: &mut Buffer, a: Rect, x: usize, y: usize, ch: char, fg: Color) {
    if x >= a.width as usize || y >= a.height as usize {
        return;
    }
    buf[(a.x + x as u16, a.y + y as u16)]
        .set_char(ch)
        .set_fg(fg);
}

fn braille(buf: &mut Buffer, a: Rect, dots: &[Vec<bool>], color: impl Fn(usize) -> Color) {
    const L: [u16; 4] = [0x01, 0x02, 0x04, 0x40];
    const R: [u16; 4] = [0x08, 0x10, 0x20, 0x80];
    let (w, h) = (a.width as usize, a.height as usize);
    for cy in 0..h {
        for cx in 0..w {
            let mut mask = 0u16;
            for k in 0..4 {
                let dy = (h - 1 - cy) * 4 + (3 - k); // k=0 es la fila superior de la celda
                if dots[cx * 2][dy] {
                    mask |= L[k];
                }
                if dots[cx * 2 + 1][dy] {
                    mask |= R[k];
                }
            }
            if mask != 0 {
                put_top(
                    buf,
                    a,
                    cx,
                    cy,
                    char::from_u32(0x2800 + mask as u32).unwrap_or(' '),
                    color(cx),
                );
            }
        }
    }
}

impl Widget for Visualizer<'_> {
    fn render(self, a: Rect, buf: &mut Buffer) {
        if a.width == 0 || a.height == 0 || self.bands.is_empty() {
            return;
        }
        let (w, h) = (a.width as usize, a.height as usize);
        let t = self.theme;
        let bands = self.bands;
        let grad = |x: usize| t.gradient(x as f32 / (w.max(2) - 1) as f32);
        let dim = |c: Color, k: f32| mix(t.bg, c, k);
        match self.style % 10 {
            // 0 · Segmentos: pilas de bloques con zonas verde/ámbar/rojo y estela tenue.
            0 => {
                let n = (w + 1) / 3;
                let x0 = (w - (n * 3).saturating_sub(1).min(w)) / 2;
                for i in 0..n {
                    let filled = (sample(bands, i, n) * h as f32).round() as usize;
                    for row in 0..h {
                        let c = zone(t, (row as f32 + 0.5) / h as f32);
                        for dx in 0..2 {
                            let col = if row < filled { c } else { dim(c, 0.13) };
                            put(buf, a, x0 + i * 3 + dx, row, '▆', col);
                        }
                    }
                }
            }
            // 1 · Mosaico: columnas anchas con degradado horizontal y fantasmas dispersos.
            1 => {
                let n = (w + 1) / 3;
                let x0 = (w - (n * 3).saturating_sub(1).min(w)) / 2;
                for i in 0..n {
                    let e = (sample(bands, i, n) * (h * 8) as f32).round() as usize;
                    let c = t.gradient(i as f32 / (n.max(2) - 1) as f32);
                    let full = e / 8;
                    for row in 0..h {
                        let (ch, col) = if row < full {
                            ('█', c)
                        } else if row == full && e % 8 > 0 {
                            (BLOCKS[e % 8], c)
                        } else if row < full + 3 && noise(i, row) < 0.35 {
                            ('█', dim(c, 0.14))
                        } else {
                            continue;
                        };
                        for dx in 0..2 {
                            put(buf, a, x0 + i * 3 + dx, row, ch, col);
                        }
                    }
                }
            }
            // 2 · Led: puntos cuadrados apilados con remate superior.
            2 => {
                let n = w / 2;
                for i in 0..n {
                    let filled = (sample(bands, i, n) * h as f32).round() as usize;
                    for row in 0..filled {
                        put(
                            buf,
                            a,
                            i * 2,
                            row,
                            '■',
                            zone(t, (row as f32 + 0.5) / h as f32),
                        );
                    }
                    if filled > 0 && filled < h {
                        put(buf, a, i * 2, filled, '▬', t.accent3);
                    }
                }
            }
            // 3 · Medidor: vúmetros horizontales L/R con tres filas por canal.
            3 => {
                let rows = ((h.saturating_sub(1)) / 2).clamp(1, 3);
                let mw = w.saturating_sub(3).max(1);
                for (c, label) in ["L", "R"].iter().enumerate() {
                    let y0 = c * (rows + 1);
                    if y0 >= h {
                        break;
                    }
                    put_top(
                        buf,
                        a,
                        0,
                        y0 + rows / 2,
                        label.chars().next().unwrap(),
                        t.muted.fg.unwrap_or(t.accent),
                    );
                    for k in 0..rows {
                        let v = bands
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| i % 2 == c && i * rows / bands.len() == k)
                            .map(|(_, v)| *v)
                            .fold(0.0f32, f32::max)
                            .clamp(0.0, 1.0);
                        let filled = (v * mw as f32).round() as usize;
                        for x in 0..mw {
                            let (ch, col) = if x < filled {
                                ('▮', zone(t, x as f32 / mw as f32))
                            } else {
                                ('·', dim(t.muted.fg.unwrap_or(t.accent), 0.7))
                            };
                            put_top(buf, a, x + 3, y0 + k, ch, col);
                        }
                        if filled > 0 {
                            put_top(
                                buf,
                                a,
                                filled.min(mw - 1) + 3,
                                y0 + k,
                                '■',
                                t.error.fg.unwrap_or(t.accent3),
                            );
                        }
                    }
                }
            }
            // 4 · Matriz: partículas que se dispersan hacia arriba.
            4 => {
                for x in 0..w {
                    let filled = (sample(bands, x, w) * h as f32).ceil() as usize;
                    for row in 0..filled {
                        let density = 1.0 - (row as f32 / filled.max(1) as f32) * 0.85;
                        if row == 0 || noise(x, row) < density * density {
                            put(
                                buf,
                                a,
                                x,
                                row,
                                '▪',
                                if row == 0 {
                                    t.playback.fg.unwrap_or(t.accent)
                                } else {
                                    zone(t, row as f32 / h as f32 * 0.7)
                                },
                            );
                        }
                    }
                }
            }
            // 5 · Espejo: barras finas simétricas respecto al eje central.
            5 => {
                let (top, bot) = (h / 2, h - h / 2);
                let n = w / 2;
                for i in 0..n {
                    let v = sample(bands, i, n);
                    let c = t.gradient(i as f32 / (n.max(2) - 1) as f32);
                    let up = (v * (top * 8) as f32).round() as usize;
                    for row in 0..top {
                        let lv = up.saturating_sub(row * 8).min(8);
                        if lv > 0 {
                            put(buf, a, i * 2, bot + row, BLOCKS[lv], c);
                        }
                    }
                    let down = (v * (bot * 8) as f32).round() as usize;
                    for row in 0..bot {
                        let lv = down.saturating_sub(row * 8).min(8);
                        let ch = match lv {
                            0 => continue,
                            8 => '█',
                            4..=7 => '▀',
                            _ => '▔',
                        };
                        put_top(buf, a, i * 2, top + row, ch, dim(c, 0.65));
                    }
                }
            }
            // 6 · Braille: relleno de alta resolución (2×4 puntos por celda).
            6 => {
                let mut dots = vec![vec![false; h * 4]; w * 2];
                for (dx, column) in dots.iter_mut().enumerate() {
                    let up = (sample(bands, dx, w * 2) * (h * 4) as f32).round() as usize;
                    for dot in column.iter_mut().take(up.min(h * 4)) {
                        *dot = true;
                    }
                }
                braille(buf, a, &dots, grad);
            }
            // 7 · Onda: curva continua de puntos braille.
            7 => {
                let mut dots = vec![vec![false; h * 4]; w * 2];
                let ys: Vec<usize> = (0..w * 2)
                    .map(|dx| (sample(bands, dx, w * 2) * (h * 4 - 1) as f32).round() as usize)
                    .collect();
                for (dx, column) in dots.iter_mut().enumerate() {
                    let next = ys[(dx + 1).min(w * 2 - 1)];
                    let start = ys[dx].min(next);
                    let end = ys[dx].max(next);
                    for dot in column.iter_mut().take(end + 1).skip(start) {
                        *dot = true;
                    }
                }
                braille(buf, a, &dots, grad);
            }
            // 8 · Pulso: tallos finos con cabeza circular.
            8 => {
                let n = w / 2;
                for i in 0..n {
                    let filled = (sample(bands, i, n) * h as f32).round() as usize;
                    let c = t.gradient(i as f32 / (n.max(2) - 1) as f32);
                    for row in 0..filled.saturating_sub(1) {
                        put(buf, a, i * 2, row, '│', dim(c, 0.45));
                    }
                    put(
                        buf,
                        a,
                        i * 2,
                        filled.saturating_sub(1),
                        if filled == 0 { '·' } else { '●' },
                        c,
                    );
                }
            }
            // 9 · Aurora: columnas continuas con degradado vertical.
            _ => {
                for x in 0..w {
                    let e = (sample(bands, x, w) * (h * 8) as f32).round() as usize;
                    for row in 0..h {
                        let lv = e.saturating_sub(row * 8).min(8);
                        if lv == 0 {
                            break;
                        }
                        put(
                            buf,
                            a,
                            x,
                            row,
                            BLOCKS[lv],
                            t.gradient(row as f32 / (h.max(2) - 1) as f32),
                        );
                    }
                }
            }
        }
    }
}

const PALETTES: [([u8; 3], [u8; 3], [u8; 3]); 8] = [
    ([30, 18, 80], [10, 26, 70], [143, 107, 255]),
    ([10, 40, 70], [27, 18, 80], [39, 215, 242]),
    ([60, 16, 70], [20, 18, 58], [240, 107, 232]),
    ([70, 36, 20], [42, 18, 64], [245, 177, 74]),
    ([10, 54, 52], [11, 36, 64], [62, 224, 143]),
    ([70, 20, 44], [26, 18, 80], [255, 107, 138]),
    ([16, 36, 80], [16, 26, 64], [107, 168, 255]),
    ([40, 24, 80], [11, 42, 58], [196, 166, 255]),
];

fn rgb(c: [u8; 3]) -> Color {
    Color::Rgb(c[0], c[1], c[2])
}

/// Color de un píxel de la portada generada de forma determinista a partir de `seed`.
pub fn art_pixel(seed: u64, px: f32, py: f32, w: f32, h: f32) -> Color {
    let palette = PALETTES[(seed % PALETTES.len() as u64) as usize];
    let phase = ((seed >> 8) % 100) as f32 / 100.0 * std::f32::consts::TAU;
    let size = w.min(h).max(1.0);
    let nx = (px - w / 2.0) / size;
    let ny = (py - h * 0.55) / size;
    let r = (nx * nx + ny * ny).sqrt();
    let base = mix(
        rgb(palette.0),
        rgb(palette.1),
        (py / h.max(1.0)).clamp(0.0, 1.0),
    );
    let glow = (-r * r * 9.0).exp();
    let ring = (r * 16.0 + phase).sin() * 0.5 + 0.5;
    let line =
        (1.0 - (ring - 0.5).abs() * 2.0).max(0.0).powf(10.0) * (1.0 - r * 1.4).max(0.0) * 0.8;
    mix(base, rgb(palette.2), (glow * 0.75 + line).clamp(0.0, 1.0))
}

/// Portada abstracta con medios bloques (dos píxeles por celda).
pub struct CoverArt {
    pub seed: u64,
}

impl Widget for CoverArt {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let w = area.width as f32;
        let h = area.height as f32 * 2.0;
        for cy in 0..area.height {
            for cx in 0..area.width {
                let top = art_pixel(self.seed, cx as f32, cy as f32 * 2.0, w, h);
                let bottom = art_pixel(self.seed, cx as f32, cy as f32 * 2.0 + 1.0, w, h);
                buf[(area.x + cx, area.y + cy)]
                    .set_char('▀')
                    .set_fg(top)
                    .set_bg(bottom);
            }
        }
    }
}

/// Carátula real (miniatura) con medios bloques, conservando la proporción.
pub struct ImageCover<'a> {
    pub thumb: &'a std::sync::Arc<Thumb>,
}

impl Widget for ImageCover<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t: &Thumb = self.thumb;
        if area.width == 0 || area.height == 0 || t.w == 0 || t.h == 0 {
            return;
        }
        // Terminal con gráficos: se deja el hueco vacío y la imagen real se pinta tras el dibujo.
        if let Some(r) = crate::gfx::register(self.thumb, area) {
            for y in r.top()..r.bottom() {
                for x in r.left()..r.right() {
                    buf[(x, y)].reset();
                }
            }
            return;
        }
        let (tw, th) = (area.width as f32, area.height as f32 * 2.0);
        let s = (tw / t.w as f32).min(th / t.h as f32);
        let ow = ((t.w as f32 * s).round() as u32).clamp(1, area.width as u32);
        let oh = (((t.h as f32 * s).round() as u32 / 2) * 2).clamp(2, area.height as u32 * 2);
        let ox = (area.width as u32 - ow) / 2;
        let oy = (area.height as u32 * 2 - oh) / 4;
        let sample = |x: u32, y: u32| -> Color {
            let (x0, x1) = (
                x * t.w / ow,
                ((x + 1) * t.w / ow).max(x * t.w / ow + 1).min(t.w),
            );
            let (y0, y1) = (
                y * t.h / oh,
                ((y + 1) * t.h / oh).max(y * t.h / oh + 1).min(t.h),
            );
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let p = t.px[(yy * t.w + xx) as usize];
                    r += p[0] as u32;
                    g += p[1] as u32;
                    b += p[2] as u32;
                    n += 1;
                }
            }
            let n = n.max(1);
            Color::Rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
        };
        for cy in 0..oh / 2 {
            for cx in 0..ow {
                let (x, y) = (area.x + (ox + cx) as u16, area.y + (oy + cy) as u16);
                if x >= area.right() || y >= area.bottom() {
                    continue;
                }
                buf[(x, y)]
                    .set_char('▀')
                    .set_fg(sample(cx, cy * 2))
                    .set_bg(sample(cx, cy * 2 + 1));
            }
        }
    }
}

/// Carátula por defecto de las emisoras: radio retro (altavoz, visor de sintonía y mandos) dibujada con medios bloques.
pub struct RadioCover;

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
}

fn rrect(p: (f32, f32), c: (f32, f32), half: (f32, f32), r: f32) -> f32 {
    let q = (
        (p.0 - c.0).abs() - half.0 + r,
        (p.1 - c.1).abs() - half.1 + r,
    );
    (q.0.max(0.0).powi(2) + q.1.max(0.0).powi(2)).sqrt() + q.0.max(q.1).min(0.0) - r
}

fn radio_pixel(u: f32, v: f32) -> Color {
    let line = [90.0, 225.0, 240.0];
    let amber = [255.0, 190.0, 60.0];
    let red = [255.0, 90.0, 90.0];
    let dist = |cx: f32, cy: f32| ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
    let mut c = lerp(
        [45.0, 25.0, 90.0],
        [10.0, 70.0, 100.0],
        ((v + 1.0) / 2.0).clamp(0.0, 1.0),
    );
    let p = (u, v);
    // Antena con punta ámbar.
    if seg_dist(p, (0.5, -0.3), (0.74, -0.92)) < 0.04 {
        c = line;
    }
    if dist(0.74, -0.92) < 0.08 {
        c = amber;
    }
    // Cuerpo del aparato.
    let body = rrect(p, (0.0, 0.3), (0.9, 0.56), 0.14);
    if body < 0.0 {
        c = [20.0, 28.0, 54.0];
    }
    if body.abs() < 0.05 {
        c = line;
    }
    // Altavoz con rejilla de rayas.
    let sp = dist(-0.42, 0.3);
    if sp < 0.38 {
        c = if ((v * 11.0).floor() as i32) % 2 == 0 {
            [60.0, 100.0, 150.0]
        } else {
            [34.0, 58.0, 100.0]
        };
    }
    if (sp - 0.38).abs() < 0.035 {
        c = line;
    }
    // Visor de sintonía con marcas y aguja roja.
    let win = rrect(p, (0.38, 0.0), (0.38, 0.17), 0.05);
    if win < 0.0 {
        c = [10.0, 46.0, 66.0];
        if ((u * 14.0).floor() as i32) % 2 == 0 && v > -0.02 && v < 0.12 {
            c = [120.0, 200.0, 215.0];
        }
        if (u - 0.5).abs() < 0.025 {
            c = red;
        }
    }
    if win.abs() < 0.03 {
        c = line;
    }
    // Dos mandos.
    for kx in [0.16f32, 0.6] {
        let k = dist(kx, 0.52);
        if k < 0.13 {
            c = amber;
        }
        if k < 0.045 {
            c = [20.0, 28.0, 54.0];
        }
    }
    Color::Rgb(c[0] as u8, c[1] as u8, c[2] as u8)
}

impl Widget for RadioCover {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let (w, h) = (area.width as f32, area.height as f32 * 2.0);
        let side = w.min(h);
        let px = |x: f32, y: f32| {
            radio_pixel((x - w / 2.0) / (side / 2.0), (y - h / 2.0) / (side / 2.0))
        };
        for cy in 0..area.height {
            for cx in 0..area.width {
                let (x, y) = (cx as f32 + 0.5, cy as f32 * 2.0);
                buf[(area.x + cx, area.y + cy)]
                    .set_char('▀')
                    .set_fg(px(x, y + 0.5))
                    .set_bg(px(x, y + 1.5));
            }
        }
    }
}

/// Vinilo girando con su aguja: carátula de la pantalla de inicio cuando no suena nada.
pub struct VinylCover {
    pub tick: u64,
}

fn vinyl_pixel(u: f32, v: f32, angle: f32) -> Color {
    let mut c = lerp(
        [45.0, 25.0, 90.0],
        [10.0, 70.0, 100.0],
        ((v + 1.0) / 2.0).clamp(0.0, 1.0),
    );
    let p = (u, v);
    let plate = rrect(p, (0.0, 0.0), (0.98, 0.98), 0.12);
    if plate < 0.0 {
        c = [24.0, 20.0, 40.0];
    }
    if plate.abs() < 0.03 {
        c = [90.0, 225.0, 240.0];
    }
    let r = (u * u + v * v).sqrt();
    let th = v.atan2(u);
    if r < 0.84 {
        let groove = ((r * 46.0).sin() * 0.5 + 0.5) * 10.0;
        let sheen = (2.0 * (th - angle)).cos().max(0.0).powi(6) * 38.0;
        let base = 16.0 + groove + sheen;
        c = [base, base, base + 8.0];
        if r > 0.80 {
            c = [40.0, 40.0, 50.0];
        }
    }
    if r < 0.3 {
        c = if r > 0.22 {
            [196.0, 166.0, 255.0]
        } else {
            [255.0, 107.0, 138.0]
        };
        let mark = ((u - 0.15 * angle.cos()).powi(2) + (v - 0.15 * angle.sin()).powi(2)).sqrt();
        if mark < 0.05 {
            c = [255.0, 255.0, 255.0];
        }
    }
    if r < 0.035 {
        c = [10.0, 10.0, 14.0];
    }
    // Brazo con aguja apoyada sobre el disco.
    let arm =
        seg_dist(p, (0.88, -0.88), (0.66, -0.46)).min(seg_dist(p, (0.66, -0.46), (0.4, -0.1)));
    if arm < 0.032 {
        c = [205.0, 205.0, 215.0];
    }
    if seg_dist(p, (0.4, -0.1), (0.31, 0.02)) < 0.055 {
        c = [255.0, 190.0, 60.0];
    }
    if ((u - 0.88).powi(2) + (v + 0.88).powi(2)).sqrt() < 0.1 {
        c = [150.0, 150.0, 165.0];
    }
    Color::Rgb(c[0] as u8, c[1] as u8, c[2] as u8)
}

impl Widget for VinylCover {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let angle = self.tick as f32 * 0.18;
        let (w, h) = (area.width as f32, area.height as f32 * 2.0);
        let side = w.min(h);
        let px = |x: f32, y: f32| {
            vinyl_pixel(
                (x - w / 2.0) / (side / 2.0),
                (y - h / 2.0) / (side / 2.0),
                angle,
            )
        };
        for cy in 0..area.height {
            for cx in 0..area.width {
                let (x, y) = (cx as f32 + 0.5, cy as f32 * 2.0);
                buf[(area.x + cx, area.y + cy)]
                    .set_char('▀')
                    .set_fg(px(x, y + 0.5))
                    .set_bg(px(x, y + 1.5));
            }
        }
    }
}

/// Letras grandes 5x5 con las que se escribe el nombre de la app.
const FONT: [[&str; 5]; 7] = [
    ["#   #", "## ##", "# # #", "#   #", "#   #"],
    ["#   #", "#   #", "#   #", "#   #", " ### "],
    ["#####", "   # ", "  #  ", " #   ", "#####"],
    [" ### ", "#   #", "#   #", "#   #", " ### "],
    ["#### ", "#   #", "#### ", "#  # ", "#   #"],
    ["#   #", " # # ", "  #  ", "  #  ", "  #  "],
    ["#   #", "##  #", "# # #", "#  ##", "#   #"],
];

fn block_on(col: usize, row: usize) -> bool {
    let c = col % 48;
    let (letter, within) = (c / 6, c % 6);
    letter < 7 && within < 5 && FONT[letter][row].as_bytes()[within] == b'#'
}

/// Texto con el nombre de la app que corre por la pantalla en cuatro diseños rotativos:
/// letras grandes, ola, cinta de texto y letras con sombra.
pub struct Marquee<'a> {
    pub theme: &'a Theme,
    pub tick: u64,
}

impl Widget for Marquee<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let (w, h) = (area.width as usize, area.height as usize);
        let (t, tick) = (self.theme, self.tick);
        let mut design = ((tick / 160) % 4) as usize;
        if h < 3 || (h < 7 && (design == 0 || design == 3)) {
            design = 2;
        }
        let hue = |x: usize| {
            let ph = (x as f32 / w as f32 + tick as f32 * 0.012).rem_euclid(1.0);
            t.gradient((ph * 2.0 - 1.0).abs())
        };
        let dim = t.muted.fg.unwrap_or(t.accent);
        let shift = (tick / 2) as usize;
        let mut put = |x: usize, y: usize, ch: char, color: Color| {
            buf[(area.x + x as u16, area.y + y as u16)]
                .set_char(ch)
                .set_fg(color);
        };
        match design {
            0 => {
                let top = (h - 5) / 2;
                for r in 0..5 {
                    for x in 0..w {
                        if block_on(x + shift, r) {
                            put(x, top + r, '█', hue(x));
                        }
                    }
                }
            }
            1 => {
                let pat: Vec<char> = "M U Z O R Y N   ♫   ".chars().collect();
                for x in 0..w {
                    let ch = pat[(x + shift) % pat.len()];
                    if ch == ' ' {
                        continue;
                    }
                    let wave =
                        ((x as f32 * 0.35 + tick as f32 * 0.15).sin() * 0.5 + 0.5) * (h - 1) as f32;
                    put(x, wave.round() as usize, ch, hue(x));
                }
            }
            3 => {
                let top = (h - 6) / 2;
                let s = shift % 48;
                for r in 0..6 {
                    for x in 0..w {
                        let col = (x + 48 - s) % 48;
                        if r < 5 && block_on(col, r) {
                            put(x, top + r, '█', hue(x));
                        } else if r >= 1 && block_on((col + 47) % 48, r - 1) {
                            put(x, top + r, '▒', dim);
                        }
                    }
                }
            }
            _ => {
                let pat: Vec<char> = "  ♫  MUZORYN  ♫  Tu música. Tu control. En la terminal.  "
                    .chars()
                    .collect();
                let row = h / 2;
                for x in 0..w {
                    let ch = pat[(x + shift) % pat.len()];
                    if ch != ' ' {
                        put(x, row, ch, hue(x));
                    }
                    if h >= 3 {
                        put(x, row - 1, '━', hue(x));
                        put(x, row + 1, '─', dim);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_renders_and_changes_buffer() {
        let theme = Theme::default();
        let bands = [1.0, 0.0, 0.5, 0.25, 0.8, 0.3, 0.9, 0.1];
        for style in 0..VIZ_NAMES.len() {
            for (w, h) in [(24u16, 8u16), (3, 1), (1, 1)] {
                let area = Rect::new(0, 0, w, h);
                let mut buf = Buffer::empty(area);
                Visualizer {
                    bands: &bands,
                    theme: &theme,
                    style,
                }
                .render(area, &mut buf);
                if w >= 24 {
                    assert!(
                        buf.content().iter().any(|c| c.symbol() != " "),
                        "estilo {style} vacío"
                    );
                }
            }
        }
    }

    #[test]
    fn cover_is_deterministic_and_seed_dependent() {
        let area = Rect::new(0, 0, 8, 4);
        let (mut a, mut b, mut c) = (
            Buffer::empty(area),
            Buffer::empty(area),
            Buffer::empty(area),
        );
        CoverArt { seed: 1 }.render(area, &mut a);
        CoverArt { seed: 1 }.render(area, &mut b);
        CoverArt { seed: 2 }.render(area, &mut c);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn vinyl_spins_and_marquee_draws() {
        let area = Rect::new(0, 0, 16, 8);
        let (mut a, mut b) = (Buffer::empty(area), Buffer::empty(area));
        VinylCover { tick: 0 }.render(area, &mut a);
        VinylCover { tick: 9 }.render(area, &mut b);
        assert_ne!(a, b);
        let theme = Theme::default();
        for design in 0..4u64 {
            let area = Rect::new(0, 0, 50, 8);
            let mut buf = Buffer::empty(area);
            Marquee {
                theme: &theme,
                tick: design * 160,
            }
            .render(area, &mut buf);
            assert!(
                buf.content().iter().any(|c| c.symbol() != " "),
                "diseño {design} vacío"
            );
        }
    }
}
