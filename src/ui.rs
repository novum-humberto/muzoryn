//! Interfaz de terminal: las 12 vistas de MUZORYN (diseño 6.8: paneles con foco grueso, píldoras, degradados).
use crate::{
    app::{
        App, Cursor, InputMode, PodcastSection, RadioFilter, SearchRow, View, ALBUM_TABS,
        CROSSFADE_STEPS, LIST_MENU, SETTINGS_SECTIONS,
    },
    config,
    core::{Playback, Repeat, StreamState},
    dsp::{EQ_BANDS, EQ_LABELS, EQ_PRESETS, EQ_RANGE_DB},
    library::Track,
    media::{MediaSource, QueueItem},
    theme::Theme,
    widgets::{CoverArt, ImageCover, Marquee, RadioCover, VinylCover, Visualizer},
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------- utilidades de texto ----------

/// Sustituye caracteres de control para que una etiqueta no pueda mover el cursor del terminal.
pub fn safe_text(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

fn fit(text: &str, width: usize) -> String {
    let clean = safe_text(text);
    let n = clean.chars().count();
    if n <= width {
        return clean;
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = clean.chars().take(width - 1).collect();
    out.push('…');
    out
}

fn pad(text: &str, width: usize) -> String {
    let f = fit(text, width);
    let n = f.chars().count();
    format!("{f}{}", " ".repeat(width.saturating_sub(n)))
}

fn mmss(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

fn secs_label(seconds: u32) -> String {
    mmss(Duration::from_secs(seconds as u64))
}

fn fg(color: Color) -> Style {
    Style::default().fg(color)
}

fn bold(style: Style) -> Style {
    style.add_modifier(Modifier::BOLD)
}

fn onoff(on: bool) -> &'static str {
    if on {
        "activado"
    } else {
        "desactivado"
    }
}

// ---------- bloques comunes ----------

fn panel(t: &Theme, title: &str, focused: bool) -> Block<'static> {
    // Mantener la misma geometría de borde en todos los paneles evita que las
    // líneas parezcan desplazarse al combinar paneles enfocados y secundarios.
    // El foco se comunica mediante el color y el título, no cambiando el trazo.
    let border = if focused { fg(t.accent) } else { t.border };
    let title_span = if focused {
        Span::styled(
            format!(" ◆ {title} "),
            Style::default()
                .fg(t.bg)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" {title} "), bold(fg(t.text)))
    };
    Block::bordered()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(border)
        .style(t.panel)
        .title(Line::from(vec![Span::raw(" "), title_span]))
}

/// Píldora de texto: resaltada si está activa, apagada si no.
fn chip(t: &Theme, text: &str, active: bool) -> Span<'static> {
    if active {
        Span::styled(
            format!(" {text} "),
            Style::default()
                .fg(t.bg)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" {text} "), t.muted)
    }
}

/// Texto con degradado de tema, carácter a carácter.
fn gradient_text(t: &Theme, text: &str, strong: bool) -> Vec<Span<'static>> {
    let chars: Vec<char> = safe_text(text).chars().collect();
    let last = chars.len().max(2) - 1;
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let style = fg(t.gradient(i as f32 / last as f32));
            Span::styled(c.to_string(), if strong { bold(style) } else { style })
        })
        .collect()
}

fn view_icon(v: View) -> &'static str {
    match v {
        View::Inicio => "⌂",
        View::Biblioteca => "♫",
        View::Busqueda => "⌕",
        View::Cola => "≡",
        View::Albumes => "◉",
        View::Listas => "★",
        View::Radio => "≋",
        View::Podcasts => "◎",
        View::Letras => "♪",
        View::Audio => "◈",
        View::Completa => "▣",
        View::Ajustes => "❖",
    }
}

fn hints(t: &Theme, items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (key, label) in items {
        spans.push(Span::styled(format!(" {key} "), bold(fg(t.accent))));
        spans.push(Span::styled(format!("{label} "), t.muted));
    }
    Line::from(spans)
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn message_box(frame: &mut Frame, t: &Theme, area: Rect, title: &str, lines: Vec<Line<'static>>) {
    let block = panel(t, title, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .style(t.panel),
        inner,
    );
}

fn render_list(
    frame: &mut Frame,
    t: &Theme,
    area: Rect,
    items: Vec<ListItem<'static>>,
    cursor: &mut Cursor,
    active: bool,
) {
    cursor.sync(items.len());
    let highlight = if active { t.cursor } else { t.inactive_cursor };
    let list = List::new(items)
        .style(t.panel)
        .highlight_style(highlight)
        .highlight_symbol("▌ ");
    frame.render_stateful_widget(list, area, &mut cursor.state);
}

fn two_col(left: &str, right: &str, width: usize) -> (String, String) {
    let right_len = right.chars().count();
    let left_w = width.saturating_sub(right_len + 1);
    (pad(left, left_w), right.to_string())
}

fn progress_line(
    t: &Theme,
    pos: Duration,
    dur: Option<Duration>,
    live: bool,
    width: usize,
) -> Line<'static> {
    let left = mmss(pos);
    match dur {
        Some(d) if !live && !d.is_zero() => {
            let right = mmss(d);
            let bar = width.saturating_sub(left.len() + right.len() + 2).max(4);
            let ratio = (pos.as_secs_f64() / d.as_secs_f64()).clamp(0.0, 1.0);
            let filled = ((bar as f64) * ratio).round() as usize;
            let mut spans = vec![Span::styled(format!("{left} "), t.muted)];
            for i in 0..bar {
                if i < filled {
                    spans.push(Span::styled("━", fg(t.gradient(i as f32 / bar as f32))));
                } else if i == filled {
                    spans.push(Span::styled("●", bold(fg(t.text))));
                } else {
                    spans.push(Span::styled("─", t.border));
                }
            }
            spans.push(Span::styled(format!(" {right}"), t.muted));
            Line::from(spans)
        }
        _ => {
            let badge = Style::default()
                .fg(t.bg)
                .bg(t.error.fg.unwrap_or(t.accent3))
                .add_modifier(Modifier::BOLD);
            Line::from(vec![
                Span::styled(
                    if live { " ◉ EN DIRECTO " } else { " ● " },
                    if live { badge } else { bold(t.playback) },
                ),
                Span::styled(format!(" {left}"), t.muted),
            ])
        }
    }
}

fn state_icon(playback: Playback) -> &'static str {
    match playback {
        Playback::Playing => "▶",
        Playback::Paused => "❚❚",
        Playback::Stopped => "■",
    }
}

fn state_name(playback: Playback) -> &'static str {
    match playback {
        Playback::Playing => "Reproduciendo",
        Playback::Paused => "En pausa",
        Playback::Stopped => "Detenido",
    }
}

fn repeat_label(r: Repeat) -> &'static str {
    match r {
        Repeat::Off => "no",
        Repeat::All => "todo",
        Repeat::One => "una",
    }
}

fn controls_line(t: &Theme, app: &App) -> Line<'static> {
    let play = if app.playing() { "❚❚" } else { "▶" };
    let chip = |on: bool, text: String| {
        Span::styled(
            format!(" {text} "),
            if on {
                Style::default()
                    .fg(t.bg)
                    .bg(t.accent2)
                    .add_modifier(Modifier::BOLD)
            } else {
                t.muted
            },
        )
    };
    Line::from(vec![
        Span::styled(" ◀◀ ", bold(fg(t.text))),
        Span::styled(
            format!(" {play} "),
            Style::default()
                .fg(t.bg)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ▶▶ ", bold(fg(t.text))),
        Span::raw("  "),
        chip(app.snapshot.shuffle, "⇄ aleatorio".into()),
        chip(
            app.snapshot.repeat != Repeat::Off,
            format!("↻ {}", repeat_label(app.snapshot.repeat)),
        ),
    ])
}

fn queue_line(
    t: &Theme,
    app: &App,
    index: usize,
    item: &QueueItem,
    width: usize,
) -> ListItem<'static> {
    let current = app.snapshot.current == Some(index);
    let dur = item.duration.map(mmss).unwrap_or_else(|| {
        if item.live {
            "vivo".into()
        } else {
            "--:--".into()
        }
    });
    let label = if item.artist.is_empty() {
        item.title.clone()
    } else {
        format!("{} — {}", item.title, item.artist)
    };
    let (left, right) = two_col(
        &format!("{:>2}. {}", index + 1, label),
        &dur,
        width.saturating_sub(2),
    );
    let style = if current {
        bold(t.playback)
    } else {
        Style::default()
    };
    ListItem::new(Line::from(vec![
        Span::styled(
            if current {
                "▶ "
            } else if app.queue_track_marked(index) {
                "+ "
            } else {
                "  "
            },
            t.playback,
        ),
        Span::styled(left, style),
        Span::styled(format!(" {right}"), t.muted),
    ]))
}

fn track_header(t: &Theme, width: usize) -> Line<'static> {
    let (wt, wa, wl) = column_widths(width);
    Line::from(Span::styled(
        format!(
            "    {} {} {} {}",
            pad("CANCIÓN", wt),
            pad("ARTISTA", wa),
            pad("ÁLBUM", wl),
            "DUR."
        ),
        bold(t.muted),
    ))
}

fn column_widths(width: usize) -> (usize, usize, usize) {
    let avail = width.saturating_sub(4 + 3 + 5 + 3);
    let wt = avail * 40 / 100;
    let wa = avail * 28 / 100;
    (wt, wa, avail.saturating_sub(wt + wa))
}

fn track_line(
    t: &Theme,
    track: &Track,
    fav: bool,
    playing: bool,
    marked: bool,
    width: usize,
) -> ListItem<'static> {
    let (wt, wa, wl) = column_widths(width.saturating_sub(2));
    ListItem::new(Line::from(vec![
        Span::styled(
            match (playing, marked) {
                (true, true) => "▶+",
                (true, false) => "▶ ",
                (false, true) => "+ ",
                _ => "  ",
            },
            if marked && !playing {
                bold(fg(t.accent))
            } else {
                t.playback
            },
        ),
        Span::styled(if fav { "★ " } else { "  " }, fg(t.accent3)),
        Span::styled(file_icon(&track.path), fg(t.accent2)),
        Span::styled(
            pad(&track.title, wt),
            if playing {
                bold(t.playback)
            } else {
                Style::default()
            },
        ),
        Span::raw(" "),
        Span::styled(pad(&track.artist, wa), t.muted),
        Span::raw(" "),
        Span::styled(pad(&track.album, wl), t.muted),
        Span::raw(" "),
        Span::styled(secs_label(track.duration), t.muted),
    ]))
}

/// Icono Nerd Font según el tipo de archivo (por extensión).
fn file_icon(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp3" | "mp2" => "\u{f001} ",
        "flac" | "wav" | "aiff" | "aif" | "ape" | "alac" | "wv" => "\u{f025} ",
        "ogg" | "oga" | "opus" => "\u{f028} ",
        "m4a" | "aac" | "mp4" | "m4b" | "wma" => "\u{f1c7} ",
        "m3u" | "m3u8" | "xspf" | "toml" | "html" => "\u{f0cb} ",
        _ => "\u{f15b} ",
    }
}

fn playing_track(app: &App, index: usize) -> bool {
    app.current_track_index() == Some(index)
}

fn empty_library(frame: &mut Frame, t: &Theme, area: Rect, app: &App) {
    let dirs: Vec<String> = app
        .settings
        .music_dirs
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    message_box(
        frame,
        t,
        area,
        "Biblioteca vacía",
        vec![
            Line::from(Span::styled(
                "Todavía no hay canciones indexadas.",
                bold(fg(t.text)),
            )),
            Line::default(),
            Line::from(vec![
                Span::styled("Pulsa ", t.muted),
                Span::styled("u", bold(fg(t.accent))),
                Span::styled(" para escanear:", t.muted),
            ]),
            Line::from(Span::styled(dirs.join(", "), fg(t.accent2))),
            Line::default(),
            Line::from(Span::styled(
                "Alt+M: elegir carpetas de música desde la interfaz.",
                fg(t.accent),
            )),
            Line::from(Span::styled(
                "También puedes editarlas manualmente en settings.toml (music_dirs).",
                t.muted,
            )),
            Line::from(Span::styled(
                "Abrir archivos directamente: muzoryn <ruta>",
                t.muted,
            )),
        ],
    );
}

// ---------- marco principal ----------

pub fn render(frame: &mut Frame, app: &mut App) {
    let t = app.theme.clone();
    let area = frame.area();
    frame.render_widget(Block::default().style(t.base), area);
    if area.width < 70 || area.height < 20 {
        let msg = Paragraph::new(vec![
            Line::from(Span::styled("MUZORYN", bold(fg(t.accent)))),
            Line::from(Span::styled(
                "Amplía la terminal (mínimo 70×20) para ver la interfaz.",
                t.muted,
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(msg, centered(area.width, 3, area));
        return;
    }
    // Cabecera y pie pasan a ser paneles reales: dejan de quedar como líneas
    // sueltas y comparten bordes, fondo y geometría con el resto de la interfaz.
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(8),
        Constraint::Length(5),
        Constraint::Length(3),
    ])
    .split(area);
    render_header(frame, app, &t, rows[0]);
    let body = rows[1];
    let content = if area.width >= 100 {
        let cols = Layout::horizontal([Constraint::Length(24), Constraint::Min(40)]).split(body);
        render_sidebar(frame, app, &t, cols[0]);
        cols[1]
    } else {
        body
    };
    match app.view {
        View::Inicio => view_inicio(frame, app, &t, content),
        View::Biblioteca => view_biblioteca(frame, app, &t, content),
        View::Busqueda => view_busqueda(frame, app, &t, content),
        View::Cola => view_cola(frame, app, &t, content),
        View::Albumes => view_albumes(frame, app, &t, content),
        View::Listas => view_listas(frame, app, &t, content),
        View::Radio => view_radio(frame, app, &t, content),
        View::Podcasts => view_podcasts(frame, app, &t, content),
        View::Letras => view_letras(frame, app, &t, content),
        View::Audio => view_audio(frame, app, &t, content),
        View::Completa => view_completa(frame, app, &t, content),
        View::Ajustes => view_ajustes(frame, app, &t, content),
    }
    render_player(frame, app, &t, rows[2]);
    render_status(frame, app, &t, rows[3]);
    render_input_popup(frame, app, &t, area);
    if app.directory_picker.is_some() {
        render_directory_picker(frame, app, &t, area);
    }
    if app.dl_window {
        render_downloads(frame, app, &t, area);
    }
    if app.dl_choice.is_some() {
        render_dl_choice(frame, app, &t, area);
    }
    if app.help {
        render_help(frame, app, &t, area);
    }
    if app.manual {
        render_manual(frame, app, &t, area);
    }
    render_toast(frame, app, &t, area);
}

/// Barra de texto `███░░░ 42%` de `width` celdas en total (0.0..=1.0 o indeterminada si `None`).
fn progress_bar(t: &Theme, width: usize, frac: Option<f64>) -> Line<'static> {
    match frac {
        Some(f) => {
            let f = f.clamp(0.0, 1.0);
            let label = format!(" {:>3}%", (f * 100.0).floor() as u32);
            let bar_w = width.saturating_sub(label.chars().count());
            let filled = ((bar_w as f64) * f).round() as usize;
            Line::from(vec![
                Span::styled("█".repeat(filled), fg(t.accent)),
                Span::styled("░".repeat(bar_w.saturating_sub(filled)), t.muted),
                Span::styled(label, bold(fg(t.accent2))),
            ])
        }
        None => Line::from(Span::styled("▒".repeat(width), fg(t.accent))),
    }
}

fn human_bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    if n as f64 >= MB {
        format!("{:.1} MB", n as f64 / MB)
    } else {
        format!("{} KB", n / 1024)
    }
}

/// Fracción de la descarga de canal completo (episodios hechos + parte del actual).
fn bulk_fraction(app: &App) -> Option<f64> {
    app.bulk_download_fraction()
}

/// Progreso global: canal completo (si hay) y todos los episodios sueltos, activos o en cola.
fn overall_fraction(app: &App) -> Option<f64> {
    app.download_overall_fraction()
}

/// Indicador compacto de descargas para la barra lateral: una sola línea,
/// sin barras individuales duplicadas (éstas permanecen en Ctrl+D).
fn download_lines(app: &App, t: &Theme, width: usize, max_rows: usize) -> Vec<Line<'static>> {
    if !app.downloading() || max_rows == 0 {
        return Vec::new();
    }

    // Navegación es el panel más estrecho: usar etiquetas compactas y adaptar
    // el contenido al ancho disponible. Los detalles completos siguen en Ctrl+D.
    let status = if let Some(bulk) = &app.bulk {
        let current = (bulk.done + 1).min(bulk.total);
        let percent = bulk
            .size
            .filter(|size| *size > 0)
            .map(|size| ((bulk.received as f64 / size as f64).clamp(0.0, 1.0) * 100.0) as usize);
        let core = match percent {
            Some(pct) => format!("{current}/{} {pct}%", bulk.total),
            None => format!("{current}/{}", bulk.total),
        };
        let with_failures = if bulk.failed > 0 {
            format!("{core} !{}", bulk.failed)
        } else {
            core.clone()
        };
        if width > with_failures.chars().count() {
            format!("⇩ {with_failures}")
        } else if width >= core.chars().count() {
            core
        } else if let Some(pct) = percent {
            format!("{pct}%")
        } else {
            format!("{current}/{}", bulk.total)
        }
    } else {
        let percent = overall_fraction(app).map(|f| (f * 100.0) as usize);
        let completed = app.dl.completed_count();
        let total = app.download_total_count();
        let failures = app.dl.failed_count();
        let core = match percent {
            Some(pct) => format!("{completed}/{total} {pct}%"),
            None => format!("{completed}/{total}"),
        };
        let with_failures = if failures > 0 {
            format!("{core} !{failures}")
        } else {
            core.clone()
        };
        if width > with_failures.chars().count() {
            format!("⇩ {with_failures}")
        } else if width >= core.chars().count() {
            core
        } else if let Some(pct) = percent {
            format!("{pct}%")
        } else {
            format!("{completed}/{total}")
        }
    };
    vec![Line::from(Span::styled(
        fit(&status, width),
        bold(fg(t.accent2)),
    ))]
}

/// Subventana de descargas: todas las entradas (canal completo y episodios, activas y en cola)
/// con su estado, bytes y barra individual. Se desplaza con ↑/↓ y se cierra con Esc o Ctrl+D.
fn render_downloads(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let rect = centered(
        area.width.saturating_sub(6).min(100),
        area.height.saturating_sub(4).min(26),
        area,
    );
    frame.render_widget(Clear, rect);
    let title = format!(
        "⇩ Descargas · {} activas · {} en cola · máximo {}",
        app.dl.active_count(),
        app.dl.queued_count(),
        app.dl.parallel_limit()
    );
    let block = panel(t, &title, true).title_bottom(hints(
        t,
        &[
            ("↑↓", "elegir"),
            ("←→", "máximo 1–8"),
            ("x", "cancelar"),
            ("Esc/Ctrl+D", "cerrar"),
        ],
    ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let width = inner.width as usize;
    let mut entries: Vec<(String, Option<f64>, String)> = Vec::new();
    if let Some(b) = &app.bulk {
        entries.push((
            format!("Canal «{}» · {}/{} episodios", b.name, b.done, b.total),
            bulk_fraction(app),
            human_bytes(b.received),
        ));
    }
    for d in app.dl.jobs() {
        let extra = if d.is_queued() {
            "en cola".to_string()
        } else {
            match d.size {
                Some(sz) if sz > 0 => format!("{} / {}", human_bytes(d.received), human_bytes(sz)),
                _ => human_bytes(d.received),
            }
        };
        entries.push((d.title.clone(), d.fraction(), extra));
    }
    if entries.is_empty() {
        let msg = Paragraph::new(Line::from(Span::styled(
            "No hay descargas en curso.",
            t.muted,
        )))
        .alignment(Alignment::Center)
        .style(t.panel);
        frame.render_widget(msg, centered(inner.width, 1, inner));
        return;
    }
    // Barra global fija arriba (con separador); debajo, las barras individuales desplazables.
    let total = overall_fraction(app);
    let head = if inner.height >= 6 { 3 } else { 0 };
    if head > 0 {
        let pct = total
            .map(|f| format!(" · {}%", (f * 100.0) as usize))
            .unwrap_or_default();
        let label = format!("TOTAL · {} descargas{pct}", app.download_total_count());
        let mut head_lines = vec![Line::from(Span::styled(
            format!(" {}", fit(&label, width.saturating_sub(1))),
            bold(fg(t.accent2)),
        ))];
        let mut bar = vec![Span::raw(" ")];
        bar.extend(progress_bar(t, width.saturating_sub(2), total).spans);
        head_lines.push(Line::from(bar));
        head_lines.push(Line::from(Span::styled("─".repeat(width), t.muted)));
        frame.render_widget(
            Paragraph::new(head_lines).style(t.panel),
            Rect::new(inner.x, inner.y, inner.width, head),
        );
    }
    let list = Rect::new(inner.x, inner.y + head, inner.width, inner.height - head);
    let visible = (list.height as usize / 2).max(1);
    let start = app
        .dl_scroll
        .min(entries.len().saturating_sub(1))
        .min(entries.len().saturating_sub(visible));
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (row, (name, frac, extra)) in entries.iter().enumerate().skip(start).take(visible) {
        let label = format!("{name} · {extra}");
        let prefix = if row == app.dl_scroll { " › " } else { "   " };
        lines.push(Line::from(Span::styled(
            format!(
                "{}{}",
                prefix,
                fit(&label, width.saturating_sub(prefix.len()))
            ),
            if row == app.dl_scroll {
                bold(fg(t.accent2))
            } else {
                fg(t.text)
            },
        )));
        let mut bar = vec![Span::raw(" ")];
        bar.extend(progress_bar(t, width.saturating_sub(2), *frac).spans);
        lines.push(Line::from(bar));
    }
    frame.render_widget(Paragraph::new(lines).style(t.panel), list);
}

/// Diálogo de descarga de varios episodios: en paralelo (hasta 8 a la vez) o en cola (de uno en uno).
fn render_dl_choice(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(choice) = app.dl_choice.as_ref() else {
        return;
    };
    let rect = centered(area.width.min(64), 9, area);
    frame.render_widget(Clear, rect);
    let title = format!("⇩ Descargar {} episodios", choice.episodes.len());
    let block = panel(t, &title, true).title_bottom(hints(
        t,
        &[("↑↓", "elegir"), ("Enter", "aceptar"), ("Esc", "cancelar")],
    ));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let width = inner.width as usize;
    let options = [
        (
            crate::downloads::DownloadMode::Parallel,
            "En paralelo",
            format!("hasta {} a la vez (configurable)", app.dl.parallel_limit()),
        ),
        (
            crate::downloads::DownloadMode::Queue,
            "En cola",
            "de una en una, en orden".to_string(),
        ),
    ];
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (mode, name, detail) in options {
        let on = choice.mode == mode;
        let style = if on { t.cursor } else { fg(t.text) };
        lines.push(Line::from(Span::styled(
            fit(&format!(" {} {name}", if on { "▶" } else { " " }), width),
            style,
        )));
        lines.push(Line::from(Span::styled(
            fit(&format!("     {detail}"), width),
            t.muted,
        )));
    }
    if choice.skipped > 0 {
        lines.push(Line::from(Span::styled(
            fit(
                &format!(" {} omitidos (ya descargados o en curso)", choice.skipped),
                width,
            ),
            t.muted,
        )));
    }
    frame.render_widget(Paragraph::new(lines).style(t.panel), inner);
}

/// Aviso flotante arriba a la derecha; desaparece solo.
fn render_toast(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(toast) = &app.toast else { return };
    let text = format!(" {} {} ", if toast.ok { "✔" } else { "✖" }, toast.text);
    let w = (text.chars().count() as u16 + 2)
        .clamp(12, area.width.saturating_sub(2).max(12))
        .min(area.width);
    if area.height < 5 || w < 6 {
        return;
    }
    let rect = Rect::new(
        area.x + area.width - w - 1.min(area.width - w),
        area.y + 1,
        w,
        3,
    );
    let color = if toast.ok {
        t.accent2
    } else {
        t.error.fg.unwrap_or(t.accent3)
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(bold(fg(color)))
        .style(Style::default().bg(t.surface).fg(t.text));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            fit(&text, inner.width as usize),
            bold(fg(color)),
        )))
        .style(Style::default().bg(t.surface)),
        inner,
    );
}

fn render_header(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let title = format!("♫ MUZORYN · v{VERSION}");
    let block = panel(t, &title, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // El tema se muestra siempre con su nombre visible, no con el nombre
    // interno del archivo de configuración.
    let mut right = vec![
        Span::styled(
            format!(" TEMA: {} ", t.name.to_uppercase()),
            bold(fg(t.accent2)),
        ),
        Span::styled(
            if app.remote {
                "● daemon "
            } else {
                "○ local "
            },
            if app.remote { t.ok } else { t.muted },
        ),
    ];
    if !app.marked.is_empty() {
        right.push(Span::styled(
            format!(" + {} marcadas ", app.marked.len()),
            bold(fg(t.accent)),
        ));
    }
    if app.snapshot.sample_rate > 0 {
        right.push(Span::styled(
            format!("{} kHz · ", app.snapshot.sample_rate as f32 / 1000.0),
            t.muted,
        ));
    }
    right.push(Span::styled(
        format!("{} {}", view_icon(app.view), app.view.title()),
        Style::default()
            .fg(t.bg)
            .bg(t.accent)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(right))
            .alignment(Alignment::Right)
            .style(t.panel),
        inner,
    );
}

fn render_sidebar(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let block = panel(t, "Navegación", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;
    let spacious = inner.height as usize >= 22;
    let groups: [(&str, usize, usize); 3] =
        [("BIBLIOTECA", 0, 6), ("ESCUCHAR", 6, 9), ("SISTEMA", 9, 12)];
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (gi, (name, from, to)) in groups.iter().enumerate() {
        if spacious {
            if gi > 0 {
                lines.push(Line::default());
            }
            lines.push(Line::from(Span::styled(format!(" {name}"), bold(t.muted))));
        }
        for v in &View::ALL[*from..*to] {
            let label = pad(v.short(), width.saturating_sub(7));
            if *v == app.view {
                lines.push(Line::from(vec![
                    Span::styled("▌", fg(t.accent)),
                    Span::styled(
                        format!(" {} {} {label}", v.key_label(), view_icon(*v)),
                        t.cursor,
                    ),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(v.key_label().to_string(), bold(fg(t.accent))),
                    Span::raw(" "),
                    Span::styled(view_icon(*v).to_string(), fg(t.accent2)),
                    Span::raw(" "),
                    Span::raw(label),
                ]));
            }
        }
    }
    lines.push(Line::from(vec![
        Span::styled("  ? ", bold(fg(t.accent))),
        Span::styled("Atajos", fg(t.accent2)),
    ]));
    lines.push(Line::default());
    let (dot, label) = if app.remote {
        ("●", "Daemon activo")
    } else {
        ("○", "Modo integrado")
    };
    lines.push(Line::from(vec![
        Span::styled(format!(" {dot} "), if app.remote { t.ok } else { t.muted }),
        Span::styled(label, t.muted),
    ]));
    lines.push(Line::from(Span::styled(
        format!(" ♫ {} canciones", app.tracks.len()),
        t.muted,
    )));
    if app.scanning {
        lines.push(Line::from(Span::styled(
            format!(" ⟳ Escaneando {}…", app.scan_progress),
            fg(t.accent2),
        )));
    }
    // Panel de descargas: solo existe mientras algo se está descargando y ocupa el fondo del menú.
    let dl = download_lines(app, t, width, (inner.height as usize / 3).clamp(2, 7));
    let dl_h = (dl.len() as u16).min(inner.height);
    let nav = Rect::new(inner.x, inner.y, inner.width, inner.height - dl_h);
    frame.render_widget(Paragraph::new(lines).style(t.panel), nav);
    if dl_h > 0 {
        frame.render_widget(
            Paragraph::new(dl).style(t.panel),
            Rect::new(inner.x, inner.y + inner.height - dl_h, inner.width, dl_h),
        );
    }
}

fn render_player(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let s = &app.snapshot;
    let title = format!("{} {}", state_icon(s.playback), state_name(s.playback));
    let block = panel(t, &title, s.playback == Playback::Playing);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let width = inner.width as usize;
    let filled = ((s.volume * 10.0).round().max(0.0) as usize).min(10);
    let vol_bar = format!("{}{}", "▮".repeat(filled), "▯".repeat(10 - filled));
    let vol = format!("♪ {vol_bar} {:>3}%", (s.volume * 100.0).round() as u32);
    let vol_len = vol.chars().count();
    let badge_text = match s.playback {
        Playback::Playing => " ▶ ",
        Playback::Paused => " ❚❚ ",
        Playback::Stopped => " ■ ",
    };
    let badge_bg = if s.playback == Playback::Playing {
        t.accent
    } else {
        t.accent2
    };
    let badge = Span::styled(
        badge_text,
        Style::default()
            .fg(t.bg)
            .bg(badge_bg)
            .add_modifier(Modifier::BOLD),
    );
    let mut spans: Vec<Span<'static>> = vec![badge, Span::raw(" ")];
    match s.current_item() {
        Some(item) => {
            let room = width.saturating_sub(vol_len + 6);
            let (title, artist) = if item.live {
                live_titles(app, item)
            } else {
                (item.title.clone(), item.artist.clone())
            };
            let title_s = fit(&title, room);
            let used = title_s.chars().count();
            spans.push(Span::styled(title_s, bold(fg(t.text))));
            if !artist.is_empty() && room > used + 4 {
                spans.push(Span::styled(
                    format!(" — {}", fit(&artist, room - used - 3)),
                    t.muted,
                ));
            }
        }
        None => spans.push(Span::styled(
            "Nada en reproducción · abre Biblioteca (2), Radio (7) o Podcasts (8)",
            t.muted,
        )),
    }
    let cur: usize = spans.iter().map(|sp| sp.content.chars().count()).sum();
    spans.push(Span::raw(" ".repeat(width.saturating_sub(cur + vol_len))));
    spans.push(Span::styled(vol, fg(t.accent)));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(t.panel), rows[0]);
    let live = s.current_item().map(|i| i.live).unwrap_or(false);
    frame.render_widget(
        Paragraph::new(progress_line(t, s.position, s.duration, live, width)).style(t.panel),
        rows[1],
    );
    let next_title = s
        .current
        .and_then(|c| {
            if s.queue.is_empty() {
                None
            } else {
                s.queue.get((c + 1) % s.queue.len())
            }
        })
        .map(|i| i.title.clone());
    let mut spans = controls_line(t, app).spans;
    if s.current_item().map(|i| i.live).unwrap_or(false) || s.stream == StreamState::Connecting {
        spans.extend(stream_badge(t, s.stream));
    }
    spans.push(Span::styled(
        format!(" crossfade {}s ", s.crossfade_secs),
        t.muted,
    ));
    if s.replay_gain {
        spans.push(Span::styled(" ReplayGain ", t.muted));
    }
    if let Some(next) = next_title {
        spans.push(Span::styled("  Siguiente ▸ ", t.muted));
        spans.push(Span::styled(fit(&next, 28), fg(t.accent)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(t.panel), rows[2]);
}

fn render_status(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let title = if app.snapshot.error.is_some() {
        "Estado · error"
    } else if !app.message.is_empty() {
        "Estado"
    } else {
        "Ayuda rápida · atajos"
    };
    let block = panel(t, title, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(error) = &app.snapshot.error {
        spans.push(Span::styled(
            " ✖ ERROR ",
            Style::default()
                .fg(t.bg)
                .bg(t.error.fg.unwrap_or(t.accent3))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(error.to_string(), t.error));
    } else if !app.message.is_empty() {
        spans.push(Span::styled(
            " ℹ ",
            Style::default()
                .fg(t.bg)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(app.message.clone(), fg(t.accent)));
    } else {
        for (key, label) in [
            ("?", "ayuda"),
            ("1-0", "vistas"),
            ("espacio", "pausa"),
            ("w", "cerrar"),
            ("W", "salir"),
        ] {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::default()
                    .fg(t.bg)
                    .bg(t.accent2)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(format!(" {label}  "), t.muted));
        }
    }
    if app.scanning {
        spans.push(Span::styled(
            format!("  ⟳ escaneando {}", app.scan_progress),
            fg(t.accent2),
        ));
    }
    if app.downloading() && area.width < 100 {
        let pct = overall_fraction(app)
            .map(|f| format!(" · {}%", (f * 100.0) as usize))
            .unwrap_or_default();
        let failures = app.dl.failed_count();
        let failed_label = if failures > 0 {
            format!(" · {failures} fallidas")
        } else {
            String::new()
        };
        spans.push(Span::styled(
            format!(
                "  ⇩ {}/{} descargadas{failed_label}{pct}",
                app.dl.completed_count(),
                app.download_total_count()
            ),
            fg(t.accent2),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(t.panel), inner);
}

fn render_input_popup(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(input) = &app.input else { return };
    let title = match input.mode {
        InputMode::Search => return,
        InputMode::RadioSearch => {
            "Radio Browser: nombre o filtros, p. ej. genre:salsa country:cuba language:Español"
        }
        InputMode::PodcastSearch => {
            "Buscar podcasts en español: nombre o tema, p. ej. historia de México"
        }
        InputMode::NewList => "Nueva lista: escribe el nombre y pulsa Enter",
        InputMode::RenameList => "Renombrar lista: escribe el nombre y pulsa Enter",
        InputMode::ImportM3u => "Importar playlist: selecciona el archivo en el navegador",
        InputMode::ExportList => "Exportar lista: escribe el nombre del archivo (sin extensión)",
        InputMode::ExportListFormat => {
            "Formato de lista: m3u, m3u8, pls, toml, xspf o html (vacío = todos)"
        }
        InputMode::ExportRadioName => {
            "Exportar lista de radio: escribe el nombre del archivo (sin extensión)"
        }
        InputMode::ExportRadioFormat => {
            "Formato de radio: m3u, m3u8, pls, xspf o json (vacío = todos)"
        }
        InputMode::AddStation => "Nueva emisora: pega la URL del stream y Enter",
        InputMode::NameStation => "Nombre de la emisora (Enter vacío = automático)",
        InputMode::AddPodcast => "Nuevo podcast: pega la URL del feed RSS y Enter",
        InputMode::NamePodcast => "Nombre del podcast (Enter vacío = automático)",
    };
    let rect = centered(area.width.min(76), 5, area);
    frame.render_widget(Clear, rect);
    let caret = if app.tick % 8 < 4 { "▏" } else { " " };
    let block = panel(t, title, true);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let room = inner.width.saturating_sub(4) as usize;
    let count = input.text.chars().count();
    let tail: String = input
        .text
        .chars()
        .skip(count.saturating_sub(room))
        .collect();
    let lines = vec![
        Line::from(vec![
            Span::styled(" ❯ ", bold(fg(t.accent))),
            Span::styled(safe_text(&tail), bold(fg(t.text))),
            Span::styled(caret, fg(t.accent)),
        ]),
        Line::default(),
        hints(t, &[("Enter", "aceptar"), ("Esc", "cancelar")]),
    ];
    frame.render_widget(Paragraph::new(lines).style(t.panel), inner);
}

fn render_directory_picker(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let Some(picker) = app.directory_picker.as_ref() else {
        return;
    };
    let rect = centered(
        area.width.saturating_sub(6).min(96),
        area.height.saturating_sub(4).min(30),
        area,
    );
    frame.render_widget(Clear, rect);
    let music = picker.purpose == crate::app::PickerPurpose::MusicFolder;
    let radio = picker.purpose == crate::app::PickerPurpose::ImportRadioPlaylist;
    let list_add = picker.purpose == crate::app::PickerPurpose::ListAdd;
    let list_remove = picker.purpose == crate::app::PickerPurpose::ListRemove;
    let title = if list_add {
        "Lista · añadir carpeta o canción"
    } else if list_remove {
        "Lista · quitar carpeta o canción"
    } else if music {
        "Biblioteca · seleccionar carpeta de música"
    } else if radio {
        "Radio · importar lista de emisoras"
    } else {
        "Importar playlist · navegador de archivos"
    };
    let block = panel(t, title, true).title_bottom(if list_add || list_remove {
        hints(
            t,
            &[
                ("↑↓", "mover"),
                ("Enter", "entrar / aplicar a canción"),
                (
                    if list_add { "s" } else { "x" },
                    if list_add {
                        "añadir carpeta o canción"
                    } else {
                        "quitar carpeta o canción"
                    },
                ),
                ("Backspace", "subir"),
                ("Esc", "cerrar"),
            ],
        )
    } else if music {
        hints(
            t,
            &[
                ("↑↓", "mover"),
                ("Enter", "entrar"),
                ("s", "añadir carpeta actual"),
                ("x", "quitar carpeta seleccionada o actual de biblioteca"),
                ("Backspace", "subir"),
                ("Esc", "cerrar"),
            ],
        )
    } else {
        hints(
            t,
            &[
                ("↑↓", "mover"),
                ("Enter", "abrir/importar"),
                ("Backspace", "subir"),
                ("Esc", "cancelar"),
            ],
        )
    });
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(inner);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("Directorio actual", t.muted)),
            Line::from(Span::styled(
                picker.current.display().to_string(),
                bold(fg(t.accent)),
            )),
        ])
        .style(t.panel),
        rows[0],
    );
    let visible = rows[1].height as usize;
    let start = picker.selected.saturating_sub(visible.saturating_sub(1));
    let lines: Vec<Line<'static>> = picker
        .entries
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, path)| {
            let is_dir = path.is_dir();
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let marker = if i == picker.selected { "▸ " } else { "  " };
            let glyph = if is_dir { "▣ " } else { "♫ " };
            let style = if i == picker.selected {
                t.cursor
            } else if is_dir {
                fg(t.accent2)
            } else {
                fg(t.text)
            };
            Line::from(Span::styled(
                pad(&format!("{marker}{glyph}{name}"), rows[1].width as usize),
                style,
            ))
        })
        .collect();
    let content = if lines.is_empty() {
        vec![Line::from(Span::styled(
            if music {
                "No hay subcarpetas en este directorio. Pulsa s para añadirlo."
            } else {
                "No hay carpetas ni playlists compatibles en este directorio."
            },
            t.muted,
        ))]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(content).style(t.panel), rows[1]);
}

fn render_help(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let rect = centered(
        area.width.saturating_sub(4).min(100),
        area.height.saturating_sub(2).min(32),
        area,
    );
    frame.render_widget(Clear, rect);
    let block = panel(t, "Ayuda · atajos", true)
        .title_bottom(hints(t, &[("?", "cerrar"), ("Esc", "cerrar")]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let lines: Vec<&str> = app.keys_help.lines().collect();
    let per_col = lines.len().div_ceil(2).max(1);
    let cols =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);
    for (i, chunk) in lines.chunks(per_col).take(2).enumerate() {
        let text: Vec<Line<'static>> = chunk
            .iter()
            .map(|l| {
                let (keys, label) =
                    l.split_at(l.char_indices().nth(18).map(|(i, _)| i).unwrap_or(l.len()));
                Line::from(vec![
                    Span::styled(keys.to_string(), bold(fg(t.accent))),
                    Span::styled(label.to_string(), fg(t.text)),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(text).style(t.panel), cols[i]);
    }
}

fn render_manual(frame: &mut Frame, app: &App, t: &Theme, area: Rect) {
    let pages: [&str; 4] = [
        "MUZORYN · MANUAL DE USUARIO\n\nReproductor musical para terminal con biblioteca local, listas, radio, podcasts, letras y ecualizador.\n\nNAVEGACIÓN\n1 Inicio · 2 Biblioteca · 3 Búsqueda · 4 Cola · 5 Álbumes/artistas · 6 Listas\n7 Radio · 8 Podcasts · 9 Letras · 0 Audio/EQ · v Vista completa · , Ajustes\n\nUsa ↑/↓ para seleccionar, Enter para abrir/reproducir, Tab para cambiar panel.\n\nLISTAS VARIADAS\nEn Listas (6) elige la lista; marca pistas, carpetas o álbumes con m desde Biblioteca, Álbumes, Búsqueda o Cola; pulsa l para añadir todo. M quita las marcas.",
        "REPRODUCCIÓN Y BIBLIOTECA\n\nEspacio: pausa/reanuda · n/p: siguiente/anterior · ←/→: buscar ±5 s\n=/−: volumen · z: aleatorio · o: repetir · c: crossfade · g: ReplayGain\ne: preset del ecualizador · t: activar/desactivar EQ · f: favorito\n\nBiblioteca: escanea las carpetas definidas en settings.toml (music_dirs).\nListas: Ctrl+N crea · F2 renombra · i importa playlist · Ctrl+E exporta (nombre y extensión) · Acciones: añadir/quitar carpeta o canción.\nEl importador admite M3U/M3U8, PLS, XSPF, TOML y HTML.",
        "RADIO, PODCASTS Y TEMAS\n\nRadio: Enter reproduce; a añade URL HTTP/HTTPS; x elimina. Tab abre Radio Browser; / busca por nombre o combina filtros: genre:jazz country:japón (también language:). El país admite nombre en español o código (US, JP). Enter en un resultado lo reproduce y lo añade a Mis emisoras.\nPodcasts: en Mis podcasts, Enter carga episodios y Tab alterna entre la lista de canales y sus capítulos; así puedes volver al canal y elegir otro tantas veces como quieras. Ctrl+B abre el directorio; ←/→ alterna Populares y Categorías; / busca; Enter suscribe y abre; Esc vuelve a Mis podcasts. a añade un feed RSS; d descarga el episodio; m marca varios episodios y d los descarga juntos; D descarga el canal completo; Ctrl+D o F3 abre descargas. En la ventana de descargas, ←/→ ajusta el máximo paralelo de 1 a 8.\nLetras: lee archivos .lrc junto a la canción; ajusta desfase con ←/→.\n\nTemas: abre Ajustes → Temas, navega con ↑/↓ y pulsa Enter para aplicar directamente.\nT también avanza al siguiente tema.",
        "COMANDOS DE TERMINAL\n\nmuzoryn [rutas...]       Abrir la app y opcionalmente reproducir archivos\nmuzoryn --help | -h      Mostrar ayuda breve y opciones\nmuzoryn --man            Mostrar este manual completo en terminal\nmuzoryn --version        Mostrar versión\nmuzoryn --daemon         Iniciar daemon de audio sin interfaz\nmuzoryn ctl <orden>      Control remoto: toggle, next, prev, stop, vol+, vol-,\n                         shuffle, repeat, forward, back, quit, status\n\nH abre este manual · ? muestra los atajos originales · Esc cierra.",
    ];
    let rect = centered(
        area.width.saturating_sub(6).min(104),
        area.height.saturating_sub(4).min(30),
        area,
    );
    frame.render_widget(Clear, rect);
    let dots: Vec<&str> = (0..4)
        .map(|i| {
            if i == app.manual_page.min(3) {
                "●"
            } else {
                "○"
            }
        })
        .collect();
    let mut bottom = vec![Span::styled(format!(" {} ", dots.join(" ")), fg(t.accent))];
    bottom.extend(hints(t, &[("←→ / ↑↓", "páginas"), ("Esc", "cerrar")]).spans);
    let block = panel(
        t,
        format!("Manual de MUZORYN · página {}/4", app.manual_page + 1).as_str(),
        true,
    )
    .title_bottom(Line::from(bottom));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let lines: Vec<Line<'static>> = pages[app.manual_page.min(3)]
        .lines()
        .map(|line| {
            let letters = line.chars().filter(|c| c.is_alphabetic());
            let heading = line.chars().any(|c| c.is_alphabetic())
                && letters.clone().all(|c| c.is_uppercase());
            if line.is_empty() {
                Line::default()
            } else if heading {
                Line::from(vec![
                    Span::styled("▌ ", fg(t.accent)),
                    Span::styled(line.to_string(), bold(fg(t.accent))),
                ])
            } else {
                Line::from(Span::styled(line.to_string(), fg(t.text)))
            }
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .style(t.panel)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

/// Dibuja la carátula de lo que suena: radio, imagen real o la generada.
fn draw_now_cover(frame: &mut Frame, app: &mut App, rect: Rect) {
    let item = app.snapshot.current_item().map(|i| {
        (
            i.live,
            crate::app::hash_str(&format!("{}{}", i.album, i.artist)),
        )
    });
    match item {
        Some((true, _)) => frame.render_widget(RadioCover, rect),
        Some((false, seed)) => match app.now_cover() {
            Some(t) => frame.render_widget(ImageCover { thumb: &t }, rect),
            None => frame.render_widget(CoverArt { seed }, rect),
        },
        None => frame.render_widget(VinylCover { tick: app.tick }, rect),
    }
}

// ---------- 1. Inicio ----------

fn view_inicio(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let cols =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(area);
    let block = panel(t, "Reproduciendo ahora", true);
    let inner = block.inner(cols[0]);
    frame.render_widget(block, cols[0]);
    let cover_w = (inner.width / 3).clamp(12, 28);
    let cover_h = (cover_w / 2).max(4);
    let split =
        Layout::horizontal([Constraint::Length(cover_w + 2), Constraint::Min(10)]).split(inner);
    draw_now_cover(
        frame,
        app,
        Rect::new(
            split[0].x + 1,
            split[0].y + 1,
            cover_w.min(split[0].width.saturating_sub(1)),
            cover_h.min(split[0].height.saturating_sub(1)),
        ),
    );
    let right = split[1];
    let w = right.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    match app.snapshot.current_item() {
        Some(item) => {
            let (label, bg) = if item.live {
                (" ◉ EN DIRECTO ", t.error.fg.unwrap_or(t.accent3))
            } else {
                (" ♪ AHORA SUENA ", t.accent2)
            };
            lines.push(Line::from(Span::styled(
                label,
                Style::default()
                    .fg(t.bg)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(gradient_text(t, &fit(&item.title, w), true)));
            lines.push(Line::from(Span::styled(
                fit(&item.artist, w),
                bold(fg(t.text)),
            )));
            lines.push(Line::from(Span::styled(fit(&item.album, w), t.muted)));
        }
        None => {
            lines.push(Line::from(Span::styled(
                " ♪ LISTO PARA REPRODUCIR ",
                Style::default()
                    .fg(t.bg)
                    .bg(t.accent2)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "Tu música. Tu control. En la terminal.",
                t.muted,
            )));
            lines.push(Line::default());
            lines.push(Line::from(vec![
                chip(t, "2 Biblioteca", true),
                Span::raw(" "),
                chip(t, "7 Radio", false),
                chip(t, "8 Podcasts", false),
            ]));
        }
    }
    let top = Rect::new(right.x, right.y, right.width, 4u16.min(right.height));
    frame.render_widget(Paragraph::new(lines).style(t.panel), top);
    let viz_top = right.y + 5;
    let bottom_rows = 3;
    if right.height > 8 {
        let viz_h = right.height.saturating_sub(5 + bottom_rows);
        let viz_rect = Rect::new(right.x, viz_top, right.width, viz_h);
        if app.snapshot.current_item().is_none() {
            frame.render_widget(
                Marquee {
                    theme: t,
                    tick: app.tick,
                },
                viz_rect,
            );
        } else {
            frame.render_widget(
                Visualizer {
                    bands: &app.snapshot.spectrum,
                    theme: t,
                    style: app.settings.visualizer,
                },
                viz_rect,
            );
        }
        let live = app.snapshot.current_item().map(|i| i.live).unwrap_or(false);
        let prog = Rect::new(right.x, viz_top + viz_h + 1, right.width, 1);
        frame.render_widget(
            Paragraph::new(progress_line(
                t,
                app.snapshot.position,
                app.snapshot.duration,
                live,
                right.width as usize,
            ))
            .style(t.panel),
            prog,
        );
        let ctl = Rect::new(right.x, viz_top + viz_h + 2, right.width, 1);
        frame.render_widget(Paragraph::new(controls_line(t, app)).style(t.panel), ctl);
    }
    // Cola
    let qblock = panel(
        t,
        &format!("Cola · {} pistas", app.snapshot.queue.len()),
        false,
    )
    .title_bottom(hints(
        t,
        &[("↑↓", "mover"), ("Enter", "reproducir"), ("x", "quitar")],
    ));
    let qinner = qblock.inner(cols[1]);
    frame.render_widget(qblock, cols[1]);
    let width = qinner.width as usize;
    let items: Vec<ListItem<'static>> = app
        .snapshot
        .queue
        .iter()
        .enumerate()
        .map(|(i, item)| queue_line(t, app, i, item, width))
        .collect();
    render_list(frame, t, qinner, items, &mut app.queue_cur, true);
}

// ---------- 2. Biblioteca ----------

fn view_biblioteca(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    if app.tracks.is_empty() {
        empty_library(frame, t, area, app);
        return;
    }
    let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(30)]).split(area);
    let fblock = panel(t, "Carpetas · Alt+M", app.focus == 0).title_bottom(hints(
        t,
        &[
            ("←→", "plegar"),
            ("Enter", "abrir"),
            ("a", "cola"),
            ("m", "marcar"),
            ("Tab", "panel"),
        ],
    ));
    let finner = fblock.inner(cols[0]);
    frame.render_widget(fblock, cols[0]);
    let items: Vec<ListItem<'static>> = app
        .folders
        .iter()
        .map(|f| {
            let mark = if !f.has_children {
                "  "
            } else if f.expanded {
                "▾ "
            } else {
                "▸ "
            };
            let icon = if f.expanded && f.has_children {
                "\u{f07c} "
            } else {
                "\u{f07b} "
            };
            let room = 22usize.saturating_sub(f.depth * 2 + 4 + format!(" {}", f.count).len());
            ListItem::new(Line::from(vec![
                Span::raw("  ".repeat(f.depth)),
                Span::styled(mark, fg(t.accent)),
                Span::styled(icon, t.playback),
                Span::raw(fit(&f.name, room.max(4))),
                Span::styled(format!(" {}", f.count), t.muted),
            ]))
        })
        .collect();
    let focus = app.focus;
    render_list(frame, t, finner, items, &mut app.folder_cur, focus == 0);

    let folder = app
        .folders
        .get(app.folder_cur.index)
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let title = format!("Biblioteca · {} · {} canciones", folder, app.lib_rows.len());
    let block = panel(t, &title, app.focus == 1).title_bottom(hints(
        t,
        &[
            ("↑↓", "seleccionar"),
            ("Enter", "reproducir"),
            ("a", "cola"),
            ("m", "marcar"),
            ("l", "a lista"),
            ("f", "favorito"),
            ("/", "buscar"),
        ],
    ));
    let inner = block.inner(cols[1]);
    frame.render_widget(block, cols[1]);
    let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(2)]).split(inner);
    frame.render_widget(
        Paragraph::new(vec![
            track_header(t, inner.width as usize),
            Line::from(Span::styled("─".repeat(inner.width as usize), t.border)),
        ])
        .style(t.panel),
        parts[0],
    );
    let width = parts[1].width.saturating_sub(2) as usize;
    let items: Vec<ListItem<'static>> = app
        .lib_rows
        .iter()
        .map(|i| {
            track_line(
                t,
                &app.tracks[*i],
                app.is_favorite(*i),
                playing_track(app, *i),
                app.is_marked(*i),
                width,
            )
        })
        .collect();
    render_list(frame, t, parts[1], items, &mut app.lib_cur, focus == 1);
}

// ---------- 3. Búsqueda ----------

fn view_busqueda(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let parts = Layout::vertical([Constraint::Length(3), Constraint::Min(4)]).split(area);
    let editing = app
        .input
        .as_ref()
        .map(|i| i.mode == InputMode::Search)
        .unwrap_or(false);
    let text = app
        .input
        .as_ref()
        .filter(|i| i.mode == InputMode::Search)
        .map(|i| i.text.clone())
        .unwrap_or_else(|| app.search_text.clone());
    let block = panel(t, "Búsqueda global", editing);
    let inner = block.inner(parts[0]);
    frame.render_widget(block, parts[0]);
    let caret = if editing && app.tick % 8 < 4 {
        "▏"
    } else {
        ""
    };
    let hint = if text.is_empty() && !editing {
        "Pulsa / para escribir…"
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⌕ ", bold(fg(t.accent))),
            Span::styled(
                fit(&text, inner.width.saturating_sub(10) as usize),
                bold(fg(t.text)),
            ),
            Span::styled(caret, fg(t.accent)),
            Span::styled(hint, t.muted),
        ]))
        .style(t.panel),
        inner,
    );

    let block = panel(
        t,
        &format!(
            "Resultados · {} canciones · {} álbumes · {} artistas",
            app.search_total[0], app.search_total[1], app.search_total[2]
        ),
        !editing,
    )
    .title_bottom(hints(
        t,
        &[
            ("↑↓", "navegar"),
            ("Enter", "reproducir"),
            ("a", "cola"),
            ("m", "marcar"),
            ("Tab", "editar"),
            ("Esc", "inicio"),
        ],
    ));
    let inner = block.inner(parts[1]);
    frame.render_widget(block, parts[1]);
    if app.search_rows.is_empty() {
        let msg = if app.search_text.is_empty() {
            "Escribe para buscar por canción, álbum o artista."
        } else {
            "Sin resultados."
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, t.muted)).style(t.panel),
            inner,
        );
        return;
    }
    let width = inner.width.saturating_sub(4) as usize;
    let items: Vec<ListItem<'static>> = app
        .search_rows
        .iter()
        .map(|row| match row {
            SearchRow::Header(text) => ListItem::new(Line::from(Span::styled(
                format!(" {text} "),
                Style::default()
                    .fg(t.bg)
                    .bg(t.accent2)
                    .add_modifier(Modifier::BOLD),
            ))),
            SearchRow::Track(i) => {
                let tr = &app.tracks[*i];
                ListItem::new(Line::from(vec![
                    Span::styled(
                        if app.is_marked(*i) {
                            "+ ♪ "
                        } else {
                            "  ♪ "
                        },
                        fg(t.accent),
                    ),
                    Span::styled(
                        fit(&tr.title, width / 2),
                        if playing_track(app, *i) {
                            bold(t.playback)
                        } else {
                            Style::default()
                        },
                    ),
                    Span::styled(format!(" — {}", fit(&tr.artist, width / 3)), t.muted),
                ]))
            }
            SearchRow::Album(i) => ListItem::new(Line::from(vec![
                Span::styled(
                    if app.is_group_marked(&app.albums[*i].tracks) {
                        "+ ◉ "
                    } else {
                        "  ◉ "
                    },
                    fg(t.accent2),
                ),
                Span::raw(fit(&app.albums[*i].name, width / 2)),
                Span::styled(
                    format!("  {}", fit(&app.albums[*i].sub, width / 3)),
                    t.muted,
                ),
            ])),
            SearchRow::Artist(i) => ListItem::new(Line::from(vec![
                Span::styled("  ☻ ", fg(t.accent3)),
                Span::raw(fit(&app.artists[*i].name, width / 2)),
                Span::styled(format!("  {}", app.artists[*i].sub), t.muted),
            ])),
        })
        .collect();
    render_list(frame, t, inner, items, &mut app.search_cur, !editing);
}

// ---------- 4. Cola ----------

fn view_cola(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let total: Duration = app.snapshot.queue.iter().filter_map(|i| i.duration).sum();
    let title = format!(
        "Cola de reproducción · {} pistas · {}",
        app.snapshot.queue.len(),
        mmss(total)
    );
    let block = panel(t, &title, true).title_bottom(hints(
        t,
        &[
            ("↑↓", "mover"),
            ("Enter", "reproducir"),
            ("x", "quitar"),
            ("m", "marcar"),
            ("Alt+↑↓", "reordenar"),
            ("Ctrl+X", "vaciar"),
            ("z", "aleatorio"),
            ("o", "repetir"),
        ],
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.snapshot.queue.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "La cola está vacía. Reproduce algo desde Biblioteca, Álbumes o Listas.",
                t.muted,
            ))
            .style(t.panel),
            inner,
        );
        return;
    }
    let width = inner.width.saturating_sub(2) as usize;
    let items: Vec<ListItem<'static>> = app
        .snapshot
        .queue
        .iter()
        .enumerate()
        .map(|(i, item)| queue_line(t, app, i, item, width))
        .collect();
    render_list(frame, t, inner, items, &mut app.queue_cur, true);
}

// ---------- 5. Álbumes y artistas ----------

fn view_albumes(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(6)]).split(area);
    let mut tabs = vec![Span::raw(" ")];
    for (i, name) in ALBUM_TABS.iter().enumerate() {
        tabs.push(chip(t, name, i == app.album_tab));
        tabs.push(Span::raw(" "));
    }
    tabs.push(Span::styled(
        "Tab cambia · ←→↑↓ mover · Enter reproducir · a cola · f favorito",
        t.muted,
    ));
    frame.render_widget(Paragraph::new(Line::from(tabs)).style(t.base), parts[0]);
    let grid_area = parts[1];
    if app.grid_len() == 0 {
        empty_library(frame, t, grid_area, app);
        return;
    }
    let card_w: u16 = 22;
    let card_h: u16 = 10;
    let cols = (grid_area.width / card_w).max(1) as usize;
    let rows_visible = (grid_area.height / card_h).max(1) as usize;
    app.grid_cols = cols;
    let sel_row = app.grid_sel / cols;
    let first_row = if sel_row >= rows_visible {
        sel_row + 1 - rows_visible
    } else {
        0
    };
    let start = first_row * cols;
    let end = (start + rows_visible * cols).min(app.grid_len());
    for (slot, index) in (start..end).enumerate() {
        let (r, c) = (slot / cols, slot % cols);
        let rect = Rect::new(
            grid_area.x + c as u16 * card_w,
            grid_area.y + r as u16 * card_h,
            card_w - 1,
            card_h.min(grid_area.height),
        );
        if rect.right() > grid_area.right() || rect.bottom() > grid_area.bottom() {
            continue;
        }
        let group = app.grid()[index].clone();
        let selected = index == app.grid_sel;
        let fav = app.group_is_favorite(&group);
        let block = Block::bordered()
            .border_type(ratatui::widgets::BorderType::Rounded)
            .style(t.panel)
            .border_style(if selected { fg(t.accent) } else { t.border });
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        if inner.height < 4 {
            continue;
        }
        let cover_rect = Rect::new(inner.x, inner.y, inner.width, inner.height - 2);
        match app.group_cover(&group) {
            Some(t) => frame.render_widget(ImageCover { thumb: &t }, cover_rect),
            None => frame.render_widget(CoverArt { seed: group.seed }, cover_rect),
        }
        let name_style = if selected {
            Style::default()
                .fg(t.bg)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            bold(fg(t.text))
        };
        let star = if app.is_group_marked(&group.tracks) {
            "+ "
        } else if fav {
            "★ "
        } else {
            ""
        };
        let w = inner.width as usize;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(star, fg(t.accent3)),
                Span::styled(
                    fit(&group.name, w.saturating_sub(star.chars().count())),
                    name_style,
                ),
            ]))
            .style(t.panel),
            Rect::new(inner.x, inner.y + inner.height - 2, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(Span::styled(fit(&group.sub, w), t.muted)).style(t.panel),
            Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
        );
    }
}

// ---------- 6. Listas ----------

fn view_listas(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let cols = Layout::horizontal([Constraint::Percentage(36), Constraint::Min(30)]).split(area);
    let focus = app.focus;
    let block =
        panel(t, &format!("Mis listas · {}", app.list_len()), focus == 0).title_bottom(hints(
            t,
            &[
                ("Enter", "reproducir"),
                ("Ctrl+N", "nueva"),
                ("x", "eliminar"),
            ],
        ));
    let inner = block.inner(cols[0]);
    frame.render_widget(block, cols[0]);
    let width = inner.width.saturating_sub(2) as usize;
    let items: Vec<ListItem<'static>> = (0..app.list_len())
        .map(|i| {
            let (icon, name) = if i == 0 {
                ("★", "Favoritas".to_string())
            } else {
                ("♫", app.playlists[i - 1].1.clone())
            };
            let (left, right) = two_col(
                &name,
                &app.list_count(i).to_string(),
                width.saturating_sub(2),
            );
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{icon} "),
                    fg(if i == 0 { t.accent3 } else { t.accent2 }),
                ),
                Span::raw(left),
                Span::styled(format!(" {right}"), t.muted),
            ]))
        })
        .collect();
    render_list(frame, t, inner, items, &mut app.list_cur, focus == 0);

    let right = Layout::vertical([
        Constraint::Length(LIST_MENU.len() as u16 + 2),
        Constraint::Min(4),
    ])
    .split(cols[1]);
    let mblock = panel(t, "Acciones", focus == 1)
        .title_bottom(hints(t, &[("Tab", "panel"), ("Enter", "ejecutar")]));
    let minner = mblock.inner(right[0]);
    frame.render_widget(mblock, right[0]);
    let icons = ["+", "✎", "✕", "⇩", "⇧", "⊕", "⊖"];
    let items: Vec<ListItem<'static>> = LIST_MENU
        .iter()
        .enumerate()
        .map(|(i, label)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", icons[i]), fg(t.accent)),
                Span::raw(label.to_string()),
            ]))
        })
        .collect();
    render_list(frame, t, minner, items, &mut app.menu_cur, focus == 1);

    let title = format!("{} · {} pistas", app.list_name(), app.list_tracks.len());
    let block = panel(t, &title, false).title_bottom(hints(
        t,
        &[
            ("Ctrl+E", "exportar"),
            ("i", "importar"),
            ("l", "añadir pista aquí"),
            ("Acciones", "añadir/quitar carpeta"),
        ],
    ));
    let inner = block.inner(right[1]);
    frame.render_widget(block, right[1]);
    if app.list_tracks.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Lista vacía. En Biblioteca o Búsqueda pulsa l (o f para favoritas).",
                t.muted,
            ))
            .style(t.panel)
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let width = inner.width as usize;
    let lines: Vec<Line<'static>> = app
        .list_tracks
        .iter()
        .take(inner.height as usize)
        .enumerate()
        .map(|(n, i)| {
            let tr = &app.tracks[*i];
            let (left, right) = two_col(
                &format!("{:>2}. {} — {}", n + 1, tr.title, tr.artist),
                &secs_label(tr.duration),
                width,
            );
            Line::from(vec![
                Span::raw(left),
                Span::styled(format!(" {right}"), t.muted),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).style(t.panel), inner);
}

// ---------- 7. Radio ----------

fn radio_chips(t: &Theme, app: &App) -> Vec<Span<'static>> {
    let browser = app.radio_browser_mode;
    let filter = app.radio_filter;
    let mut spans = Vec::new();
    for (label, on) in [
        ("Mis emisoras", !browser && filter == RadioFilter::All),
        ("Favoritas ★", !browser && filter == RadioFilter::Favs),
        ("Recientes", !browser && filter == RadioFilter::Recent),
        ("Explorar Radio Browser", browser),
    ] {
        spans.push(chip(t, label, on));
        spans.push(Span::raw(" "));
    }
    spans
}

/// Texto e icono del estado de conexión de una emisora en directo.
fn stream_badge(t: &Theme, state: StreamState) -> Option<Span<'static>> {
    let (text, style) = match state {
        StreamState::Idle => return None,
        StreamState::Connecting => (" … Conectando… ", fg(t.accent3)),
        StreamState::Live => (" ● En vivo ", t.ok),
        StreamState::Reconnecting => (" ↻ Reconectando… ", fg(t.accent3)),
        StreamState::Unavailable => (" ✖ Emisora no disponible ", t.error),
    };
    Some(Span::styled(text, style))
}

/// Título y subtítulo de una emisora en directo: la canción si la emisora la envía (ICY);
/// si no, el nombre de la emisora y su género.
fn live_titles(app: &App, item: &QueueItem) -> (String, String) {
    match &app.snapshot.now_playing {
        Some(song) => (song.clone(), item.title.clone()),
        None => (item.title.clone(), item.artist.clone()),
    }
}

fn view_radio(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let fav_mark = |fav: bool| if fav { "(★) " } else { "(•) " };
    if app.radio_browser_mode {
        let parts = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);
        let tabs = panel(t, "Radio", true).title_bottom(hints(
            t,
            &[
                ("Tab", "cambiar sección"),
                ("/", "buscar"),
                ("f", "favorita"),
                ("i", "importar lista"),
                ("Ctrl+E", "exportar resultados"),
            ],
        ));
        let tinner = tabs.inner(parts[0]);
        frame.render_widget(tabs, parts[0]);
        frame.render_widget(
            Paragraph::new(Line::from(radio_chips(t, app))).style(t.panel),
            tinner,
        );
        let block = panel(
            t,
            if app.radio_searching {
                "Buscando emisoras…"
            } else {
                "Resultados de Radio Browser"
            },
            true,
        );
        let inner = block.inner(parts[1]);
        frame.render_widget(block, parts[1]);
        let width = inner.width.saturating_sub(2) as usize;
        let items: Vec<ListItem<'static>> = app
            .radio_results
            .iter()
            .map(|st| {
                let fav = app
                    .settings
                    .stations
                    .iter()
                    .any(|s| s.url == st.url && s.fav);
                let detail = [st.country.as_str(), st.language.as_str(), st.tags.as_str()]
                    .into_iter()
                    .filter(|v| !v.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                let meta = format!("{} {} kbps", st.codec, st.bitrate);
                let (left, right) = two_col(&st.name, &meta, width.saturating_sub(2));
                let sub = fit(&detail, width.saturating_sub(3));
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(fav_mark(fav), fg(t.accent3)),
                        Span::styled(left, bold(fg(t.text))),
                        Span::styled(format!(" {right}"), t.muted),
                    ]),
                    Line::from(Span::styled(format!("   {sub}"), t.muted)),
                ])
            })
            .collect();
        render_list(frame, t, inner, items, &mut app.radio_result_cur, true);
        let status = if app.radio_results.is_empty() {
            "/ busca por nombre o combina filtros: genre:jazz country:japón (también language:). Ctrl+E exporta la lista de resultados.".to_string()
        } else {
            "Enter reproduce y guarda · f favorita ★ · i importar · Ctrl+E exportar resultados. Tab cambia de sección.".to_string()
        };
        frame.render_widget(
            Paragraph::new(Span::styled(status, t.muted)).alignment(Alignment::Center),
            parts[2],
        );
        return;
    }
    let top = Layout::vertical([Constraint::Length(1), Constraint::Min(6)]).split(area);
    let mut chips = vec![Span::raw(" ")];
    chips.extend(radio_chips(t, app));
    chips.push(Span::styled(
        "  i importar · Ctrl+E exportar · Tab cambiar",
        t.muted,
    ));
    frame.render_widget(Paragraph::new(Line::from(chips)).style(t.base), top[0]);
    let parts = Layout::vertical([Constraint::Min(5), Constraint::Length(11)]).split(top[1]);
    let (title, empty) = match app.radio_filter {
        RadioFilter::All => (
            "Radio en directo",
            "Aún no hay emisoras: pulsa a para añadir una URL o Tab para explorar Radio Browser.",
        ),
        RadioFilter::Favs => (
            "Emisoras favoritas ★",
            "Aún no hay favoritas: pulsa f sobre una emisora para marcarla.",
        ),
        RadioFilter::Recent => (
            "Escuchadas recientemente",
            "Todavía no has escuchado ninguna emisora.",
        ),
    };
    let block = panel(t, title, true).title_bottom(hints(
        t,
        &[
            ("↑↓", "seleccionar"),
            ("Enter", "reproducir"),
            ("f", "favorita"),
            ("a", "añadir"),
            ("x", "quitar"),
            ("s", "detener"),
        ],
    ));
    let inner = block.inner(parts[0]);
    frame.render_widget(block, parts[0]);
    let width = inner.width.saturating_sub(2) as usize;
    let tick = app.tick;
    let playing = app.playing();
    let visible = app.radio_visible();
    let items: Vec<ListItem<'static>> = visible
        .iter()
        .filter_map(|i| app.settings.stations.get(*i))
        .map(|st| {
            let active = app
                .snapshot
                .current_item()
                .map(|c| c.media == MediaSource::Url(st.url.clone()))
                .unwrap_or(false);
            let bars = if active && playing {
                if tick % 8 < 4 {
                    "● EN VIVO"
                } else {
                    "○ EN VIVO"
                }
            } else if active {
                "❚❚ pausa "
            } else {
                "         "
            };
            let kbps = if st.bitrate > 0 {
                format!("{} kbps", st.bitrate)
            } else {
                "— kbps".into()
            };
            let (left, right) = two_col(&st.name, &kbps, width.saturating_sub(16));
            ListItem::new(Line::from(vec![
                Span::styled(fav_mark(st.fav), fg(t.accent3)),
                Span::styled(
                    format!("{bars} "),
                    if active {
                        fg(t.accent)
                    } else {
                        fg(t.ok.fg.unwrap_or(t.accent))
                    },
                ),
                Span::styled(
                    left,
                    if active {
                        bold(t.playback)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!(" {right}"), t.muted),
            ]))
        })
        .collect();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(empty, t.muted))
                .wrap(Wrap { trim: true })
                .style(t.panel),
            inner,
        );
    } else {
        render_list(frame, t, inner, items, &mut app.station_cur, true);
    }

    let block = panel(t, "En el aire", false);
    let inner = block.inner(parts[1]);
    frame.render_widget(block, parts[1]);
    let side = Layout::horizontal([
        Constraint::Length(inner.height.min(8) * 2 + 2),
        Constraint::Min(10),
    ])
    .split(inner);
    let cover_h = inner.height.min(8);
    frame.render_widget(
        RadioCover,
        Rect::new(side[0].x, side[0].y, cover_h * 2, cover_h),
    );
    let rows = Layout::vertical([
        Constraint::Min(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(side[1]);
    frame.render_widget(
        Visualizer {
            bands: &app.snapshot.spectrum,
            theme: t,
            style: app.settings.visualizer,
        },
        rows[0],
    );
    let room = side[1].width as usize;
    match app.snapshot.current_item() {
        Some(item) if item.live => {
            let (song, sub) = live_titles(app, item);
            frame.render_widget(
                Paragraph::new(Span::styled(fit(&format!("♪ {song}"), room), fg(t.accent)))
                    .style(t.panel),
                rows[1],
            );
            let mut spans = vec![Span::styled(fit(&sub, room.saturating_sub(26)), t.muted)];
            spans.extend(stream_badge(t, app.snapshot.stream));
            frame.render_widget(Paragraph::new(Line::from(spans)).style(t.panel), rows[2]);
        }
        _ => {
            let text = "Pulsa Enter para escuchar · a para añadir una emisora con su URL";
            frame.render_widget(
                Paragraph::new(Span::styled(fit(text, room), fg(t.accent))).style(t.panel),
                rows[1],
            );
        }
    }
}

// ---------- 8. Podcasts ----------

fn podcast_chips(t: &Theme, app: &App) -> Vec<Span<'static>> {
    let section = app.podcast_section;
    let mut items = vec![
        ("Directorio", false),
        ("Populares", section == PodcastSection::Popular),
        ("Categorías", section == PodcastSection::Categories),
    ];
    if section == PodcastSection::Results {
        items.push(("Resultados", true));
    }
    let mut spans = vec![Span::raw(" ")];
    for (label, on) in items {
        spans.push(chip(t, label, on));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!(
            " tienda: {} · ←/→ sección · Esc volver",
            app.settings.podcast_country.to_uppercase()
        ),
        t.muted,
    ));
    spans
}

/// Explorador de podcasts en español: populares, categorías y resultados de búsqueda.
fn view_podcast_browser(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let top = Layout::vertical([Constraint::Length(1), Constraint::Min(5)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(podcast_chips(t, app))).style(t.base),
        top[0],
    );
    let cols = Layout::horizontal([Constraint::Min(34), Constraint::Length(34)]).split(top[1]);
    let categories = app.podcast_section == PodcastSection::Categories && !app.category_open;
    let title = match app.podcast_section {
        PodcastSection::Popular => "Podcasts más escuchados".to_string(),
        PodcastSection::Results => "Resultados de la búsqueda".to_string(),
        PodcastSection::Categories if app.category_open => format!(
            "Categoría: {}",
            crate::podcast_directory::CATEGORIES
                .get(app.category_cur.index)
                .map(|c| c.name)
                .unwrap_or("")
        ),
        PodcastSection::Categories => "Categorías".to_string(),
    };
    let mut browser_hints = Vec::new();
    if app.podcast_section == PodcastSection::Popular {
        // La ayuda vive en el borde inferior del panel, que puede ser estrecho.
        // Priorizar la acción de recarga evita que quede truncada.
        browser_hints.push(("u", "recargar populares"));
    }
    browser_hints.extend([
        ("Enter", "abrir y suscribir"),
        ("f", "suscribir"),
        ("/", "buscar"),
    ]);
    browser_hints.push(("←→", "sección"));
    browser_hints.push(("Esc", "volver"));
    let block = panel(t, &title, true).title_bottom(hints(t, &browser_hints));
    let inner = block.inner(cols[0]);
    frame.render_widget(block, cols[0]);
    let width = inner.width.saturating_sub(2) as usize;
    if categories {
        let items: Vec<ListItem<'static>> = crate::podcast_directory::CATEGORIES
            .iter()
            .map(|c| {
                ListItem::new(Line::from(vec![
                    Span::styled("▸ ", fg(t.accent2)),
                    Span::raw(c.name),
                ]))
            })
            .collect();
        render_list(frame, t, inner, items, &mut app.category_cur, true);
    } else if app.shows.is_empty() {
        let msg = if app.shows_loading {
            "Buscando podcasts…"
        } else {
            "Sin resultados. Pulsa / para buscar un podcast por nombre o tema."
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, t.muted))
                .wrap(Wrap { trim: true })
                .style(t.panel),
            inner,
        );
    } else {
        let items: Vec<ListItem<'static>> = app
            .shows
            .iter()
            .map(|show| {
                let subscribed = app.settings.podcasts.iter().any(|p| p.url == show.feed_url);
                let count = if show.episodes > 0 {
                    format!("{} ep.", show.episodes)
                } else {
                    String::new()
                };
                let (left, right) = two_col(&show.title, &count, width.saturating_sub(4));
                let detail = [show.author.as_str(), show.genre.as_str()]
                    .into_iter()
                    .filter(|v| !v.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            if subscribed { "✓ " } else { "◉ " },
                            fg(if subscribed { t.accent3 } else { t.accent2 }),
                        ),
                        Span::styled(left, bold(fg(t.text))),
                        Span::styled(format!(" {right}"), t.muted),
                    ]),
                    Line::from(Span::styled(
                        format!("  {}", fit(&detail, width.saturating_sub(3))),
                        t.muted,
                    )),
                ])
            })
            .collect();
        render_list(frame, t, inner, items, &mut app.show_cur, true);
    }

    let block = panel(t, "Detalles", false);
    let inner = block.inner(cols[1]);
    frame.render_widget(block, cols[1]);
    let mut lines: Vec<Line<'static>> = Vec::new();
    match app.shows.get(app.show_cur.index).filter(|_| !categories) {
        Some(show) => {
            let subscribed = app.settings.podcasts.iter().any(|p| p.url == show.feed_url);
            lines.push(Line::from(Span::styled(
                safe_text(&show.title),
                bold(fg(t.text)),
            )));
            if !show.author.is_empty() {
                lines.push(Line::from(Span::styled(safe_text(&show.author), t.muted)));
            }
            if !show.genre.is_empty() {
                lines.push(Line::from(Span::styled(safe_text(&show.genre), t.muted)));
            }
            if show.episodes > 0 {
                lines.push(Line::from(Span::styled(
                    format!("{} episodios", show.episodes),
                    t.muted,
                )));
            }
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                if subscribed {
                    "✓ Suscrito (f para cancelar)"
                } else {
                    "f para suscribirte"
                },
                fg(t.accent3),
            )));
            lines.push(Line::from(Span::styled(
                "Enter abre sus episodios",
                t.muted,
            )));
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                safe_text(&crate::media::host_name(&show.feed_url)),
                t.muted,
            )));
        }
        None => {
            lines.push(Line::from(Span::styled(
                "Podcasts en español",
                bold(fg(t.text)),
            )));
            lines.push(Line::from(Span::styled("Busca con / o recorre las categorías. Enter en un podcast lo suscribe y abre sus episodios.", t.muted)));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(t.panel)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn view_podcasts(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    if app.podcast_browser {
        return view_podcast_browser(frame, app, t, area);
    }
    let cols = Layout::horizontal([Constraint::Min(34), Constraint::Length(34)]).split(area);
    // Reserva espacio para ambos paneles incluso cuando hay muchos canales.
    // El diseño anterior fijaba hasta 8 filas para la lista y otras 5 para episodios,
    // lo que desbordaba el alto disponible en terminales medianas.
    let list_h = ((app.settings.podcasts.len().clamp(1, 6) + 2) as u16)
        .min(area.height.saturating_sub(3))
        .max(3);
    let left = Layout::vertical([Constraint::Length(list_h), Constraint::Min(3)]).split(cols[0]);
    let focus = app.focus;
    let block = panel(t, "Mis podcasts", focus == 0).title_bottom(hints(
        t,
        &[
            ("Enter", "episodios"),
            ("Tab", "cambiar panel"),
            ("Ctrl+B", "directorio"),
            ("/", "buscar"),
            ("a", "añadir URL"),
            ("x", "quitar"),
        ],
    ));
    let inner = block.inner(left[0]);
    frame.render_widget(block, left[0]);
    let width = inner.width.saturating_sub(2) as usize;
    let items: Vec<ListItem<'static>> = app
        .settings
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let count = app
                .episodes
                .get(&i)
                .map(|e| format!("{} ep.", e.len()))
                .unwrap_or_else(|| "—".into());
            let (l, r) = two_col(&p.name, &count, width.saturating_sub(2));
            ListItem::new(Line::from(vec![
                Span::styled("◉ ", fg(t.accent2)),
                Span::raw(l),
                Span::styled(format!(" {r}"), t.muted),
            ]))
        })
        .collect();
    render_list(frame, t, inner, items, &mut app.podcast_cur, focus == 0);

    let mut episodes_title = match &app.bulk {
        Some(b) => format!("Episodios · ⇩ canal {}/{}", b.done, b.total),
        None => "Episodios".to_string(),
    };
    let marked_eps = app.marked_episode_count();
    if marked_eps > 0 {
        episodes_title.push_str(&format!(" · {marked_eps} marcados"));
    }
    let block = panel(t, &episodes_title, focus == 1).title_bottom(hints(
        t,
        &[
            ("u", "actualizar feed"),
            ("↑↓", "seleccionar"),
            ("Tab", "volver a canales"),
            ("Enter", "reproducir"),
            ("d", "descargar"),
            ("D", "canal completo"),
        ],
    ));
    let inner = block.inner(left[1]);
    frame.render_widget(block, left[1]);
    let idx = app.podcast_cur.index;
    match app.episodes.get(&idx) {
        None => {
            let msg = if app.feed_loading == Some(idx) {
                "Cargando feed…"
            } else {
                "Sin datos. Pulsa u para cargar el feed."
            };
            frame.render_widget(
                Paragraph::new(Span::styled(msg, t.muted)).style(t.panel),
                inner,
            );
        }
        Some(eps) => {
            let width = inner.width.saturating_sub(2) as usize;
            let items: Vec<ListItem<'static>> = eps
                .iter()
                .enumerate()
                .map(|(n, ep)| {
                    let mark = if app.is_downloaded(ep) { "⇩ " } else { "  " };
                    let (l, r) = two_col(
                        &format!("T{}. {}", n + 1, ep.title),
                        &format!("{mark}{}", ep.duration),
                        width.saturating_sub(2),
                    );
                    let lead = if app.is_episode_marked(ep) {
                        Span::styled("✓ ", bold(fg(t.accent2)))
                    } else {
                        Span::styled("▷ ", t.muted)
                    };
                    ListItem::new(Line::from(vec![
                        lead,
                        Span::raw(l),
                        Span::styled(format!(" {r}"), t.muted),
                    ]))
                })
                .collect();
            render_list(frame, t, inner, items, &mut app.episode_cur, focus == 1);
        }
    }

    let block = panel(t, "Detalles", false);
    let inner = block.inner(cols[1]);
    frame.render_widget(block, cols[1]);
    let (inner, cover_area) = if inner.height >= 18 {
        let ch = 8u16;
        (
            Rect::new(
                inner.x,
                inner.y + ch + 1,
                inner.width,
                inner.height - ch - 1,
            ),
            Some(Rect::new(
                inner.x + inner.width.saturating_sub(ch * 2) / 2,
                inner.y,
                (ch * 2).min(inner.width),
                ch,
            )),
        )
    } else {
        (inner, None)
    };
    if let Some(r) = cover_area {
        match app.podcast_cover(idx) {
            Some(th) => frame.render_widget(ImageCover { thumb: &th }, r),
            None => frame.render_widget(
                CoverArt {
                    seed: crate::app::hash_str(
                        &app.settings
                            .podcasts
                            .get(idx)
                            .map(|p| p.url.clone())
                            .unwrap_or_default(),
                    ),
                },
                r,
            ),
        }
    }
    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            chip(t, "▶ Enter", true),
            Span::raw(" "),
            chip(t, "⇩ d", false),
            Span::raw(" "),
            chip(t, "⇩⇩ D", false),
            Span::raw(" "),
            chip(t, "↻ u", false),
            Span::raw(" "),
            chip(t, "✓ m", false),
        ]),
        Line::from(Span::styled(
            " reproducir · descargar uno · canal completo · actualizar · marcar",
            t.muted,
        )),
        Line::default(),
    ];
    if let Some(ep) = app
        .episodes
        .get(&idx)
        .and_then(|e| e.get(app.episode_cur.index))
    {
        lines.push(Line::from(Span::styled(
            safe_text(&ep.title),
            bold(fg(t.text)),
        )));
        lines.push(Line::from(Span::styled(safe_text(&ep.date), t.muted)));
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            fit(&safe_text(&ep.description), 600),
            t.muted,
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(t.panel)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

// ---------- 9. Letras ----------

fn view_letras(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let title = match app.snapshot.current_item() {
        Some(i) => format!("Letras · {}", i.title),
        None => "Letras sincronizadas".into(),
    };
    let block = panel(t, &title, true)
        .title_bottom(hints(t, &[("espacio", "pausa"), ("[ ]", "desfase ±0.5 s")]));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let parts = Layout::vertical([Constraint::Min(3), Constraint::Length(2)]).split(inner);
    match &app.lyrics {
        None => {
            let text = vec![
                Line::from(Span::styled("No hay letra para esta pista.", bold(fg(t.text)))),
                Line::default(),
                Line::from(Span::styled("Coloca un archivo .lrc con el mismo nombre junto al audio (por ejemplo cancion.flac y cancion.lrc).", t.muted)),
            ];
            frame.render_widget(
                Paragraph::new(text)
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true })
                    .style(t.panel),
                centered(parts[0].width, 5, parts[0]),
            );
        }
        Some(lyrics) => {
            let current = app.lyric_index().unwrap_or(0) as i64;
            let h = parts[0].height as i64;
            let first = if lyrics.synced {
                (current - h / 2).max(0)
            } else {
                0
            };
            let lines: Vec<Line<'static>> = lyrics
                .lines
                .iter()
                .enumerate()
                .skip(first as usize)
                .take(h as usize)
                .map(|(i, (_, text))| {
                    let d = (i as i64 - current).abs();
                    if lyrics.synced && d == 0 {
                        let mut sp = vec![Span::styled("▶ ", fg(t.accent))];
                        sp.extend(gradient_text(t, text, true));
                        sp.push(Span::styled(" ◀", fg(t.accent)));
                        return Line::from(sp);
                    }
                    let style = if !lyrics.synced {
                        Style::default()
                    } else if d == 0 {
                        bold(fg(t.accent))
                    } else if d == 1 {
                        fg(t.text)
                    } else {
                        t.muted
                    };
                    Line::from(Span::styled(safe_text(text), style))
                })
                .collect();
            frame.render_widget(
                Paragraph::new(lines)
                    .alignment(Alignment::Center)
                    .style(t.panel),
                parts[0],
            );
        }
    }
    let live = app.snapshot.current_item().map(|i| i.live).unwrap_or(false);
    let offset = format!("± desfase: {:+.1}s", app.lyrics_offset_ms as f32 / 1000.0);
    let prog = progress_line(
        t,
        app.snapshot.position,
        app.snapshot.duration,
        live,
        parts[1]
            .width
            .saturating_sub(offset.chars().count() as u16 + 2) as usize,
    );
    let mut spans = prog.spans;
    spans.push(Span::styled(format!("  {offset}"), t.muted));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(t.panel), parts[1]);
}

// ---------- 10. Audio / ecualizador ----------

fn eq_preset_name(gains: &[f32; EQ_BANDS]) -> &'static str {
    EQ_PRESETS
        .iter()
        .find(|(_, g)| {
            g.iter()
                .zip(gains.iter())
                .all(|(a, b)| (a - b).abs() < 0.01)
        })
        .map(|(n, _)| *n)
        .unwrap_or("Personalizado")
}

fn view_audio(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let options_width = 32u16.min(area.width.saturating_sub(40));
    let cols =
        Layout::horizontal([Constraint::Min(40), Constraint::Length(options_width)]).split(area);
    let block = panel(t, "Ecualizador de 10 bandas", true).title_bottom(hints(
        t,
        &[
            ("←→", "banda"),
            ("↑↓", "±1 dB"),
            ("Enter", "restablecer"),
            ("e", "preset"),
        ],
    ));
    let inner = block.inner(cols[0]);
    frame.render_widget(block, cols[0]);
    let gutter: u16 = 6;
    if inner.height >= 8 && inner.width >= gutter + 3 * EQ_BANDS as u16 {
        let track_h = (inner.height - 3) as usize;
        let band_w = (inner.width - gutter) / EQ_BANDS as u16;
        let zero_row = track_h / 2;
        let buf = frame.buffer_mut();
        buf.set_string(inner.x, inner.y, "+12 dB", t.muted);
        buf.set_string(inner.x, inner.y + zero_row as u16, " 0 dB", t.muted);
        buf.set_string(inner.x, inner.y + track_h as u16 - 1, "-12 dB", t.muted);
        for (band, band_label) in EQ_LABELS.iter().enumerate().take(EQ_BANDS) {
            let x = inner.x + gutter + band as u16 * band_w + band_w / 2;
            let gain = app.eq[band];
            let thumb = (((EQ_RANGE_DB - gain) / (2.0 * EQ_RANGE_DB)) * (track_h as f32 - 1.0))
                .round() as usize;
            let selected = band == app.eq_sel;
            let color = t.gradient(band as f32 / (EQ_BANDS - 1) as f32);
            for row in 0..track_h {
                let y = inner.y + row as u16;
                let between = (row >= thumb.min(zero_row)) && (row <= thumb.max(zero_row));
                if row == thumb {
                    buf.set_string(x, y, "◆", bold(fg(if selected { t.text } else { color })));
                } else if between {
                    buf.set_string(x, y, "█", fg(color));
                } else if row == zero_row {
                    buf.set_string(x, y, "┼", t.border);
                } else {
                    buf.set_string(x, y, "│", t.border);
                }
            }
            let db = format!("{:+.0}", gain);
            let lx = x.saturating_sub(db.chars().count() as u16 / 2);
            buf.set_string(
                lx,
                inner.y + track_h as u16,
                db,
                if selected {
                    bold(fg(t.accent))
                } else {
                    t.muted
                },
            );
            let label = if selected {
                format!("▲{}", band_label)
            } else {
                band_label.to_string()
            };
            let fx = x.saturating_sub(label.chars().count() as u16 / 2);
            buf.set_string(
                fx,
                inner.y + track_h as u16 + 1,
                label,
                if selected {
                    bold(fg(t.accent))
                } else {
                    t.muted
                },
            );
        }
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Amplía la terminal para ver el ecualizador.",
                t.muted,
            ))
            .style(t.panel),
            inner,
        );
    }

    let block = panel(t, "Opciones de audio", false);
    let inner = block.inner(cols[1]);
    frame.render_widget(block, cols[1]);
    let s = &app.snapshot;
    let row = |key: &str, label: &str, value: String, on: bool| -> Line<'static> {
        Line::from(vec![
            Span::styled(
                if on { "✓ " } else { "○ " },
                if on { t.ok } else { t.muted },
            ),
            Span::styled(format!("{label:<13}"), bold(fg(t.text))),
            Span::styled(value, if on { bold(t.ok) } else { t.muted }),
            Span::styled(format!("  [{key}]"), fg(t.accent)),
        ])
    };
    let rate = if s.sample_rate > 0 {
        format!("{} Hz", s.sample_rate)
    } else {
        "—".into()
    };
    let crossfade = if s.crossfade_secs == 0 {
        "desactivado".to_string()
    } else {
        format!("{} s", s.crossfade_secs)
    };
    let lines = vec![
        row(
            "g",
            "ReplayGain",
            onoff(s.replay_gain).into(),
            s.replay_gain,
        ),
        Line::default(),
        row("c", "Crossfade", crossfade, s.crossfade_secs > 0),
        Line::default(),
        row("t", "Ecualizador", onoff(s.eq_enabled).into(), s.eq_enabled),
        Line::default(),
        row("e", "Preset", eq_preset_name(&app.eq).into(), true),
        Line::default(),
        Line::from(vec![
            Span::styled("  Salida:      ", t.muted),
            Span::raw("predeterminada (Rodio)"),
        ]),
        Line::from(vec![
            Span::styled("  Frecuencia:  ", t.muted),
            Span::raw(rate),
        ]),
        Line::default(),
        Line::from(Span::styled(
            format!(
                "  Crossfade: {} s",
                CROSSFADE_STEPS
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            ),
            t.muted,
        )),
    ];
    frame.render_widget(Paragraph::new(lines).style(t.panel), inner);
}

// ---------- 11. Vista completa ----------

fn view_completa(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let block = panel(t, "Vista completa", true).title_bottom(hints(
        t,
        &[
            ("espacio", "pausa"),
            ("n/p", "pista"),
            ("←→", "buscar ±5 s"),
            ("z", "aleatorio"),
            ("o", "repetir"),
        ],
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let parts = Layout::vertical([
        Constraint::Length((inner.height / 3).clamp(6, 16)),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(inner);
    let cover_h = parts[0].height;
    let cover_w = (cover_h * 2).min(parts[0].width / 2);
    let top =
        Layout::horizontal([Constraint::Length(cover_w + 2), Constraint::Min(10)]).split(parts[0]);
    draw_now_cover(
        frame,
        app,
        Rect::new(top[0].x + 1, top[0].y, cover_w, cover_h),
    );
    let lines = match app.snapshot.current_item() {
        Some(item) => vec![
            Line::from(gradient_text(
                t,
                &fit(&item.title, top[1].width as usize),
                true,
            )),
            Line::from(Span::styled(
                fit(&item.artist, top[1].width as usize),
                bold(fg(t.text)),
            )),
            Line::from(Span::styled(
                fit(&item.album, top[1].width as usize),
                t.muted,
            )),
        ],
        None => vec![Line::from(Span::styled("Nada en reproducción", t.muted))],
    };
    frame.render_widget(Paragraph::new(lines).style(t.panel), top[1]);
    frame.render_widget(
        Visualizer {
            bands: &app.snapshot.spectrum,
            theme: t,
            style: app.settings.visualizer,
        },
        parts[1],
    );
    let live = app.snapshot.current_item().map(|i| i.live).unwrap_or(false);
    frame.render_widget(
        Paragraph::new(vec![
            progress_line(
                t,
                app.snapshot.position,
                app.snapshot.duration,
                live,
                parts[2].width as usize,
            ),
            controls_line(t, app),
        ])
        .style(t.panel),
        parts[2],
    );
}

// ---------- 12. Ajustes ----------

fn kv(t: &Theme, key: &str, value: String, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("▪ ", fg(t.accent2)),
        Span::styled(format!("{key:<22}"), t.muted),
        Span::styled(value, style),
    ])
}

fn view_ajustes(frame: &mut Frame, app: &mut App, t: &Theme, area: Rect) {
    let cols = Layout::horizontal([Constraint::Length(26), Constraint::Min(30)]).split(area);
    let block = panel(t, "Ajustes y sistema", true).title_bottom(hints(
        t,
        &[
            ("↑↓", "sección/tema"),
            ("←→", "sección/valor"),
            ("Enter", "aplicar tema"),
            ("H", "manual"),
            ("?", "atajos"),
        ],
    ));
    let inner = block.inner(cols[0]);
    frame.render_widget(block, cols[0]);
    let icons = ["▣", "♪", "♫", "⇄", "✦", "★", "◐"];
    let items: Vec<ListItem<'static>> = SETTINGS_SECTIONS
        .iter()
        .enumerate()
        .map(|(i, s)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", icons[i % icons.len()]), fg(t.accent2)),
                Span::raw(s.to_string()),
            ]))
        })
        .collect();
    render_list(frame, t, inner, items, &mut app.settings_cur, true);

    let section = app.settings_cur.index;
    let block = panel(t, SETTINGS_SECTIONS[section], false);
    let inner = block.inner(cols[1]);
    frame.render_widget(block, cols[1]);
    let s = &app.snapshot;
    let plain = Style::default();
    let ok = t.ok;
    let lines: Vec<Line<'static>> = match section {
        0 => vec![
            kv(t, "Idioma", "Español".into(), plain),
            kv(t, "Tema", app.theme.name.clone(), plain),
            kv(
                t,
                "Visualizador",
                format!(
                    "{} (b: cambiar)",
                    crate::widgets::VIZ_NAMES[app.settings.visualizer % 10]
                ),
                plain,
            ),
            kv(t, "Origen del tema", app.theme_source.clone(), plain),
            kv(
                t,
                "Archivo de ajustes",
                config::settings_path().display().to_string(),
                plain,
            ),
            kv(t, "Versión", VERSION.into(), plain),
        ],
        1 => vec![
            kv(t, "Motor de audio", "Rodio".into(), plain),
            kv(t, "Dispositivo", "predeterminado del sistema".into(), plain),
            kv(
                t,
                "Frecuencia",
                if s.sample_rate > 0 {
                    format!("{} Hz", s.sample_rate)
                } else {
                    "—".into()
                },
                plain,
            ),
            kv(
                t,
                "ReplayGain",
                onoff(s.replay_gain).into(),
                if s.replay_gain { ok } else { plain },
            ),
            kv(
                t,
                "Crossfade",
                if s.crossfade_secs == 0 {
                    "desactivado".into()
                } else {
                    format!("{} s", s.crossfade_secs)
                },
                plain,
            ),
            kv(
                t,
                "Ecualizador",
                format!("{} ({})", onoff(s.eq_enabled), eq_preset_name(&app.eq)),
                plain,
            ),
        ],
        2 => vec![
            kv(
                t,
                "Carpetas",
                app.settings
                    .music_dirs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                plain,
            ),
            kv(t, "Canciones", app.tracks.len().to_string(), plain),
            kv(t, "Álbumes", app.albums.len().to_string(), plain),
            kv(t, "Artistas", app.artists.len().to_string(), plain),
            kv(
                t,
                "Escaneo",
                if app.scanning {
                    format!("en curso ({})", app.scan_progress)
                } else {
                    "Incremental".into()
                },
                plain,
            ),
            kv(
                t,
                "Base de datos",
                config::database_path().display().to_string(),
                plain,
            ),
            Line::default(),
            Line::from(Span::styled(
                "Alt+M: añadir/quitar carpetas (incluye discos externos).",
                fg(t.accent),
            )),
            Line::from(Span::styled(
                "Pulsa Enter o u para actualizar la biblioteca.",
                t.muted,
            )),
        ],
        3 => vec![
            kv(
                t,
                "Estado del daemon",
                if app.remote {
                    "Activo (conectado)".into()
                } else {
                    "Integrado (sin daemon)".into()
                },
                if app.remote { ok } else { plain },
            ),
            kv(
                t,
                "Socket IPC",
                config::socket_path().display().to_string(),
                plain,
            ),
            kv(
                t,
                "Reproducción continua",
                if app.remote {
                    "Sí: sigue al cerrar la interfaz".into()
                } else {
                    "No: se detiene al salir".into()
                },
                plain,
            ),
            Line::default(),
            Line::from(Span::styled(
                "Inicia el daemon con: muzoryn --daemon",
                fg(t.accent),
            )),
            Line::from(Span::styled(
                "Controla desde fuera: muzoryn ctl toggle|next|prev|stop|vol+|vol-",
                t.muted,
            )),
        ],
        4 => vec![
            kv(
                t,
                "Plugins (hooks)",
                match &app.settings.hooks_notice {
                    Some(n) => format!("{} activos · {n}", app.settings.hooks.len()),
                    None => format!(
                        "{} activos · se editan con: muzoryn hooks",
                        app.settings.hooks.len()
                    ),
                },
                plain,
            ),
            kv(
                t,
                "Emisoras de radio",
                app.settings.stations.len().to_string(),
                plain,
            ),
            kv(
                t,
                "Podcasts",
                app.settings.podcasts.len().to_string(),
                plain,
            ),
            kv(
                t,
                "Descargas",
                config::data_dir().join("podcasts").display().to_string(),
                plain,
            ),
            kv(
                t,
                "Descargas paralelas",
                format!(
                    "{} de {}  (←/→ para ajustar)",
                    app.dl.parallel_limit(),
                    crate::downloads::MAX_PARALLEL
                ),
                bold(fg(t.accent)),
            ),
            kv(
                t,
                "Exportaciones de listas",
                config::data_dir().join("exports").display().to_string(),
                plain,
            ),
        ],
        5 => vec![
            Line::from(Span::styled("MUZORYN", bold(fg(t.accent)))),
            Line::from(Span::styled(
                "Tu música. Tu control. En la terminal.",
                t.muted,
            )),
            Line::default(),
            kv(t, "Versión", VERSION.into(), plain),
            kv(t, "Plataforma", std::env::consts::OS.into(), plain),
            kv(
                t,
                "Construido con",
                "Rust · Ratatui · Rodio · SQLite".into(),
                plain,
            ),
            kv(t, "Licencia", "Open Source (MIT)".into(), plain),
        ],
        _ => {
            let mut rows = vec![
                Line::from(Span::styled(
                    "↑/↓ selecciona · Enter aplica · ←/→ cambia sección · T cambia al siguiente.",
                    fg(t.accent),
                )),
                Line::default(),
            ];
            rows.extend(
                crate::theme::THEMES
                    .iter()
                    .enumerate()
                    .map(|(i, (name, _))| {
                        let active = app.theme_source.eq_ignore_ascii_case(name);
                        let selected = i == app.theme_cur.index;
                        let marker = if active { "● " } else { "○ " };
                        let style = if selected {
                            t.cursor
                        } else if active {
                            bold(fg(t.accent))
                        } else {
                            fg(t.text)
                        };
                        let mut spans = vec![
                            Span::styled(
                                if selected { "▌ " } else { "  " },
                                if selected { fg(t.accent) } else { t.muted },
                            ),
                            Span::styled(marker, if active { fg(t.accent) } else { t.muted }),
                        ];
                        if let Ok(th) = Theme::resolve(name) {
                            spans.push(Span::styled("██", fg(th.accent)));
                            spans.push(Span::styled("██", fg(th.accent2)));
                            spans.push(Span::styled("██", fg(th.accent3)));
                            spans.push(Span::raw(" "));
                        }
                        spans.push(Span::styled(name.to_string(), style));
                        Line::from(spans)
                    }),
            );
            rows
        }
    };
    frame.render_widget(
        Paragraph::new(lines)
            .style(t.panel)
            .wrap(Wrap { trim: false }),
        inner,
    );
}
