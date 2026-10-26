//! Background page rendering.
//!
//! Each worker thread owns its own document instance (MuPDF documents are not
//! `Send`). The UI publishes the full set of tiles it currently wants, most
//! important first; workers pick the first one that is not done yet, so stale
//! requests (pages scrolled past, old zoom levels) are dropped for free.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};

use crossbeam_channel::{Receiver, Sender};

use crate::config::{PageMode, Rgba};
use crate::doc;

/// Tiles are square chunks of a page rendered at a given scale.
pub const TILE: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub page: usize,
    /// Device pixels per PDF point, quantized to 1/1000.
    pub scale_milli: u32,
    pub tx: u32,
    pub ty: u32,
    pub mode: PageMode,
    /// Bumped on every document reload so old tiles are never mistaken for new ones.
    pub generation: u32,
}

impl TileKey {
    pub fn scale(&self) -> f32 {
        self.scale_milli as f32 / 1000.0
    }
}

pub struct Tile {
    pub key: TileKey,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct Colors {
    pub fg: Rgba,
    pub bg: Rgba,
}

struct Shared {
    wanted: Vec<TileKey>,
    in_flight: HashSet<TileKey>,
    path: PathBuf,
    opts: doc::OpenOptions,
    generation: u32,
    colors: Colors,
    shutdown: bool,
}

pub struct RenderPool {
    shared: Arc<(Mutex<Shared>, Condvar)>,
    pub results: Receiver<Tile>,
}

impl RenderPool {
    pub fn new(
        path: PathBuf,
        opts: doc::OpenOptions,
        threads: usize,
        colors: Colors,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new((
            Mutex::new(Shared {
                wanted: Vec::new(),
                in_flight: HashSet::new(),
                path,
                opts,
                generation: 0,
                colors,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = crossbeam_channel::unbounded();
        let wake = Arc::new(wake);
        for i in 0..threads.max(1) {
            let shared = shared.clone();
            let tx = tx.clone();
            let wake = wake.clone();
            std::thread::Builder::new()
                .name(format!("render-{i}"))
                .spawn(move || worker(shared, tx, move || wake()))
                .expect("spawn render thread");
        }
        Self { shared, results: rx }
    }

    /// Replace the wanted set. `done` tiles are skipped by the caller already.
    pub fn request(&self, wanted: Vec<TileKey>) {
        let (lock, cv) = &*self.shared;
        let mut s = lock.lock().unwrap();
        if s.wanted != wanted {
            s.wanted = wanted;
            cv.notify_all();
        }
    }

    /// Workers reopen the document (with `opts`) for tiles of the new generation.
    pub fn reload(&self, generation: u32, opts: doc::OpenOptions) {
        let (lock, cv) = &*self.shared;
        let mut s = lock.lock().unwrap();
        s.generation = generation;
        s.opts = opts;
        s.wanted.clear();
        cv.notify_all();
    }

    pub fn set_colors(&self, colors: Colors) {
        self.shared.0.lock().unwrap().colors = colors;
    }
}

impl Drop for RenderPool {
    fn drop(&mut self) {
        let (lock, cv) = &*self.shared;
        lock.lock().unwrap().shutdown = true;
        cv.notify_all();
    }
}

fn worker(shared: Arc<(Mutex<Shared>, Condvar)>, tx: Sender<Tile>, wake: impl Fn()) {
    let (lock, cv) = &*shared;
    let mut document: Option<(u32, Box<dyn doc::Backend>)> = None;
    loop {
        let (key, path, opts, colors) = {
            let mut s = lock.lock().unwrap();
            loop {
                if s.shutdown {
                    return;
                }
                let current = s.generation;
                let pos = s.wanted.iter().position(|k| k.generation == current && !s.in_flight.contains(k));
                if let Some(pos) = pos {
                    let key = s.wanted.remove(pos);
                    s.in_flight.insert(key);
                    break (key, s.path.clone(), s.opts.clone(), s.colors);
                }
                s = cv.wait(s).unwrap();
            }
        };
        if document.as_ref().map(|(g, _)| *g) != Some(key.generation) {
            document = doc::open_with(&path, &opts).ok().map(|d| (key.generation, d));
        }
        let tile = document.as_ref().and_then(|(_, d)| render_tile(d.as_ref(), key, colors));
        lock.lock().unwrap().in_flight.remove(&key);
        if let Some(tile) = tile {
            if tx.send(tile).is_err() {
                return;
            }
            wake();
        }
    }
}

/// Pixel size of the page at a scale, and the size of tile (tx, ty).
pub fn tile_rect(page_size: (f32, f32), scale: f32, tx: u32, ty: u32) -> (i32, i32, u32, u32) {
    let pw = (page_size.0 * scale).ceil() as u32;
    let ph = (page_size.1 * scale).ceil() as u32;
    let x = tx * TILE;
    let y = ty * TILE;
    (x as i32, y as i32, TILE.min(pw.saturating_sub(x)), TILE.min(ph.saturating_sub(y)))
}

fn render_tile(d: &dyn doc::Backend, key: TileKey, colors: Colors) -> Option<Tile> {
    let (x, y, w, h) = tile_rect(d.page_size(key.page), key.scale(), key.tx, key.ty);
    if w == 0 || h == 0 {
        return None;
    }
    let mut bmp = d.render(key.page, key.scale(), x, y, w, h).ok()?;
    recolor(&mut bmp.pixels, key.mode, colors);
    Some(Tile { key, width: bmp.width, height: bmp.height, pixels: bmp.pixels })
}

pub fn recolor(pixels: &mut [u8], mode: PageMode, colors: Colors) {
    match mode {
        PageMode::Normal => {}
        PageMode::Custom => {
            let (fg, bg) = (colors.fg.0, colors.bg.0);
            for px in pixels.as_chunks_mut::<4>().0 {
                // Map by luminance so colored figures stay readable.
                let l = (px[0] as u32 * 54 + px[1] as u32 * 183 + px[2] as u32 * 19) >> 8;
                for c in 0..3 {
                    let tinted = fg[c] as i32 + ((bg[c] as i32 - fg[c] as i32) * l as i32) / 255;
                    // Keep a bit of the original chroma.
                    let chroma = px[c] as i32 - l as i32;
                    px[c] = (tinted + chroma / 2).clamp(0, 255) as u8;
                }
            }
        }
        PageMode::Dark => {
            for px in pixels.as_chunks_mut::<4>().0 {
                // Invert lightness, keep hue: rgb' = rgb + (255 - max - min).
                let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
                let shift = 255 - r.max(g).max(b) - r.min(g).min(b);
                let (bg, fg) = (colors.bg.0, colors.fg.0);
                let inv = [r, g, b].map(|v| (v + shift).clamp(0, 255));
                let inv = lift_dark_colors(inv);
                for (c, inv) in inv.into_iter().enumerate() {
                    // Compress into the theme's [page_bg, page_fg] range to avoid pure black/white.
                    let lo = bg[c].min(fg[c]) as i32;
                    let hi = bg[c].max(fg[c]) as i32;
                    px[c] = (lo + (inv * (hi - lo)) / 255) as u8;
                }
            }
        }
    }
}

/// Lightness inversion keeps saturated colors like hyperref blue (HSL lightness 0.5) as dark as they were, which
/// leaves links nearly invisible on a dark page. Lift strongly colored pixels toward white, keeping their hue,
/// until they reach a readable luminance. Grays (text, rules) are left alone.
fn lift_dark_colors([r, g, b]: [i32; 3]) -> [i32; 3] {
    const MIN_LUMA: i32 = 150;
    let chroma = r.max(g).max(b) - r.min(g).min(b);
    let luma = (r * 54 + g * 183 + b * 19) >> 8;
    if chroma < 64 || luma >= MIN_LUMA {
        return [r, g, b];
    }
    // c' = c + (255 - c) * t raises luma linearly in t.
    let t = ((MIN_LUMA - luma) * 256) / (255 - luma).max(1);
    [r, g, b].map(|c| c + ((255 - c) * t) / 256)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance of an sRGB color.
    fn luminance(c: [u8; 3]) -> f64 {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    }

    fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn dark(rgb: [u8; 3]) -> [u8; 3] {
        let theme = crate::config::Theme::preset("midnight");
        let mut px = [rgb[0], rgb[1], rgb[2], 255];
        recolor(&mut px, PageMode::Dark, Colors { fg: theme.page_fg, bg: theme.page_bg });
        [px[0], px[1], px[2]]
    }

    #[test]
    fn dark_mode_links_stay_readable() {
        let bg = dark([255, 255, 255]);
        for (name, link) in [("hyperref blue", [0, 0, 255]), ("red", [255, 0, 0]), ("magenta", [255, 0, 255])] {
            let c = contrast(dark(link), bg);
            assert!(c >= 4.5, "{name} link has contrast {c:.2} on the dark page");
        }
    }

    #[test]
    fn dark_mode_keeps_text_and_hue() {
        let (bg, text) = (dark([255, 255, 255]), dark([0, 0, 0]));
        assert!(contrast(text, bg) >= 7.0, "body text contrast {:.2}", contrast(text, bg));
        let blue = dark([0, 0, 255]);
        assert!(blue[2] > blue[0] + 60 && blue[2] > blue[1] + 60, "blue should stay blue: {blue:?}");
        // Mid gray is left as the plain inversion.
        let gray = dark([128, 128, 128]);
        assert!((gray[0] as i32 - gray[1] as i32).abs() < 6, "gray should stay neutral: {gray:?}");
    }
}
