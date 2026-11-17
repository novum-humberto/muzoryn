//! Carátulas reales: etiqueta embebida, imagen de la carpeta o imagen del feed.
//! Se decodifican en un hilo aparte y se guardan como miniaturas pequeñas.
use std::{io::Read, path::{Path, PathBuf}};
use lofty::{file::TaggedFileExt, picture::PictureType};
use crate::{feeds, media::http_agent};

/// Lado máximo de la versión de medios bloques (fallback universal).
pub const HALF: u32 = 96;
/// Lado máximo de la versión de alta resolución (protocolos gráficos).
pub const HI: u32 = 512;
/// Máximo de carátulas en memoria antes de vaciar la caché.
pub const CACHE_MAX: usize = 64;
static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

#[derive(Debug)]
pub struct Thumb {
    /// Identificador único (para los protocolos gráficos).
    pub id: u32,
    /// Dimensiones y PNG de la versión de alta resolución.
    pub hi_w: u32,
    pub hi_h: u32,
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub px: Vec<[u8; 3]>,
}

#[derive(Clone, Debug)]
pub enum CoverSource {
    /// Pistas candidatas: se usa la primera que tenga carátula.
    Files(Vec<PathBuf>),
    /// URL de un feed de podcast.
    Feed(String),
}

pub enum CoverState {
    Loading,
    Missing,
    Ready(std::sync::Arc<Thumb>),
}

pub fn load(src: &CoverSource) -> Option<Thumb> {
    match src {
        CoverSource::Files(paths) => paths.iter().find_map(|p| file_cover(p)),
        CoverSource::Feed(url) => decode(&feed_cover_bytes(url)?),
    }
}

fn decode(bytes: &[u8]) -> Option<Thumb> {
    use image::{imageops::FilterType, DynamicImage, ImageEncoder};
    let img = image::load_from_memory(bytes).ok()?;
    if img.width() == 0 || img.height() == 0 { return None; }
    let hi = if img.width() > HI || img.height() > HI { img.thumbnail(HI, HI).to_rgb8() } else { img.to_rgb8() };
    let (hi_w, hi_h) = hi.dimensions();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(hi.as_raw(), hi_w, hi_h, image::ExtendedColorType::Rgb8).ok()?;
    let small = DynamicImage::ImageRgb8(hi).resize(HALF, HALF, FilterType::Lanczos3).to_rgb8();
    let (w, h) = small.dimensions();
    if w == 0 || h == 0 { return None; }
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Some(Thumb { id, hi_w, hi_h, png, w, h, px: small.pixels().map(|p| p.0).collect() })
}

fn file_cover(path: &Path) -> Option<Thumb> {
    if let Ok(tagged) = lofty::read_from_path(path) {
        let mut fallback: Option<Vec<u8>> = None;
        for tag in tagged.tags() {
            for pic in tag.pictures() {
                if pic.pic_type() == PictureType::CoverFront {
                    if let Some(t) = decode(pic.data()) { return Some(t); }
                } else if fallback.is_none() {
                    fallback = Some(pic.data().to_vec());
                }
            }
        }
        if let Some(t) = fallback.as_deref().and_then(decode) { return Some(t); }
    }
    folder_cover(path.parent()?)
}

fn folder_cover(dir: &Path) -> Option<Thumb> {
    const NAMES: [&str; 6] = ["cover", "folder", "front", "album", "albumart", "artwork"];
    let mut found: Vec<(usize, PathBuf)> = std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| {
        let p = e.path();
        let ext = p.extension()?.to_string_lossy().to_lowercase();
        if !matches!(ext.as_str(), "jpg" | "jpeg" | "png") { return None; }
        let stem = p.file_stem()?.to_string_lossy().to_lowercase();
        NAMES.iter().position(|n| *n == stem).map(|i| (i, p))
    }).collect();
    found.sort();
    found.into_iter().find_map(|(_, p)| decode(&std::fs::read(p).ok()?))
}

fn feed_cover_bytes(feed_url: &str) -> Option<Vec<u8>> {
    let agent = http_agent();
    let body = agent.get(feed_url).call().ok()?.into_string().ok()?;
    let image_url = feeds::channel_image(&body)?;
    let mut bytes = Vec::new();
    agent.get(&image_url).call().ok()?.into_reader().take(8_000_000).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}
