//! MuPDF backend (through the `mupdf` crate).
//!
//! MuPDF keeps one context per thread (cloned from a shared, locked base context with a common resource
//! store), so opening one `MupdfDoc` per thread matches the rest of the app. Page space is MuPDF's: points with
//! a top-left origin, offset so every page starts at (0, 0).

use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};
use mupdf::pdf::PdfDocument;
use mupdf::text_page::TextBlockType;
use mupdf::{Colorspace, DestinationKind, Device, Document, IRect, Matrix, MetadataName, Pixmap, TextPageFlags};

use super::{Backend, Bitmap, DocPos, Link, LinkTarget, Metadata, OpenError, OpenOptions, OutlineItem, PageText, Rect};

pub struct MupdfDoc {
    doc: Document,
    /// Set for PDFs: page labels live in the PDF catalog.
    pdf: Option<PdfDocument>,
    /// Page bounds in MuPDF space; `x0`/`y0` are usually 0 but not always (crop boxes).
    bounds: Vec<Rect>,
    /// Clockwise view rotation in degrees (0, 90, 180, 270), applied to everything the backend returns.
    rotation: u16,
}

impl MupdfDoc {
    pub fn open(path: &Path, opts: &OpenOptions) -> Result<Self> {
        let abs = super::canonicalize(path).with_context(|| format!("cannot open {}", path.display()))?;
        // MuPDF takes raw bytes on Unix (any file name works) but UTF-8 on Windows.
        #[cfg(unix)]
        let file = abs.as_path();
        #[cfg(not(unix))]
        let file = abs.to_str().ok_or_else(|| anyhow!("{}: the path isn't valid Unicode", abs.display()))?;
        let mut doc = Document::open(file).map_err(|e| anyhow!("failed to load document: {e}"))?;
        if doc.needs_password().unwrap_or(false) {
            match &opts.password {
                None => return Err(OpenError::NeedsPassword.into()),
                Some(pw) if !doc.authenticate(pw).unwrap_or(false) => return Err(OpenError::WrongPassword.into()),
                Some(_) => {}
            }
        }
        let n = doc.page_count().map_err(|e| anyhow!("{e}"))?.max(0) as usize;
        if n == 0 {
            // pdflatex writes the page tree last, so a file that's still being written opens with no pages.
            bail!("no pages (the file is incomplete or damaged)");
        }
        let bounds = match &opts.page_bounds {
            Some(b) if b.len() == n => b.to_vec(),
            _ => (0..n)
                .map(|i| {
                    doc.load_page(i as i32)
                        .and_then(|p| p.bounds())
                        .map(|b| Rect::new(b.x0, b.y0, b.x1, b.y1))
                        .unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0))
                })
                .collect(),
        };
        let pdf = if doc.is_pdf() { PdfDocument::try_from(doc.clone()).ok() } else { None };
        Ok(Self { doc, pdf, bounds, rotation: opts.rotation % 360 })
    }

    fn origin(&self, page: usize) -> (f32, f32) {
        self.bounds.get(page).map(|b| (b.x0, b.y0)).unwrap_or_default()
    }

    /// A rect relative to the page origin (unrotated), in the rotated view space the app uses.
    fn rotate(&self, page: usize, r: Rect) -> Rect {
        let (w, h) = self.bounds.get(page).map(|b| (b.width(), b.height())).unwrap_or_default();
        match self.rotation {
            90 => Rect::new(h - r.y1, r.x0, h - r.y0, r.x1),
            180 => Rect::new(w - r.x1, h - r.y1, w - r.x0, h - r.y0),
            270 => Rect::new(r.y0, w - r.x1, r.y1, w - r.x0),
            _ => r,
        }
    }

    /// A rect in MuPDF page space, relative to the page origin but not rotated.
    fn unrotated(&self, page: usize, r: mupdf::Rect) -> Rect {
        let (ox, oy) = self.origin(page);
        Rect::new(r.x0 - ox, r.y0 - oy, r.x1 - ox, r.y1 - oy)
    }

    /// A rect in MuPDF page space, in view space.
    fn page_rect(&self, page: usize, r: mupdf::Rect) -> Rect {
        self.rotate(page, self.unrotated(page, r))
    }

    /// A link or outline destination as a page position.
    fn dest_pos(&self, dest: &mupdf::link::LinkDestination) -> Option<DocPos> {
        let page = dest.loc.page_number as usize;
        let b = *self.bounds.get(page)?;
        let top = match dest.kind {
            DestinationKind::XYZ { top, .. } | DestinationKind::FitH { top } | DestinationKind::FitBH { top } => top,
            DestinationKind::FitR { top, bottom, .. } => Some(top.min(bottom)),
            _ => None,
        };
        let top = top.map(|t| t.clamp(b.y0, b.y1));
        let y = match (self.rotation, top) {
            // Sideways, a destination's top is a horizontal position; land at the page top.
            (90 | 270, _) | (_, None) => 0.0,
            // Upside down, the content below the destination's top is above it on screen: land a screenful higher.
            (180, Some(t)) => (self.rotate(page, Rect::new(0.0, t - b.y0, 0.0, t - b.y0)).y0 - 300.0).max(0.0),
            (_, Some(t)) => self.rotate(page, Rect::new(0.0, t - b.y0, 0.0, t - b.y0)).y0,
        };
        Some(DocPos { page, y })
    }

    fn walk_outline(&self, items: &[mupdf::Outline], depth: usize, out: &mut Vec<OutlineItem>) {
        for item in items {
            let target = item.dest.as_ref().and_then(|d| self.dest_pos(d));
            out.push(OutlineItem { title: item.title.split_whitespace().collect::<Vec<_>>().join(" "), depth, target });
            self.walk_outline(&item.down, depth + 1, out);
        }
    }
}

impl Backend for MupdfDoc {
    fn page_count(&self) -> usize {
        self.bounds.len()
    }

    fn page_size(&self, page: usize) -> (f32, f32) {
        let (w, h) = self.bounds.get(page).map(|b| (b.width(), b.height())).unwrap_or((612.0, 792.0));
        if self.rotation % 180 == 90 { (h, w) } else { (w, h) }
    }

    fn page_label(&self, page: usize) -> Option<String> {
        self.pdf.as_ref()?.page_label(page).ok().filter(|s| !s.is_empty())
    }

    fn render(&self, page: usize, scale: f32, x: i32, y: i32, w: u32, h: u32) -> Result<Bitmap> {
        let p = self.doc.load_page(page as i32).map_err(|e| anyhow!("page {page}: {e}"))?;
        let (ox, oy) = self.origin(page);
        let (pw, ph) = self.bounds.get(page).map(|b| (b.width(), b.height())).unwrap_or_default();
        let area = IRect { x0: x, y0: y, x1: x + w as i32, y1: y + h as i32 };
        let mut pix = Pixmap::new_with_rect(&Colorspace::device_rgb(), area, false).map_err(|e| anyhow!("{e}"))?;
        pix.clear_with(255).map_err(|e| anyhow!("{e}"))?;
        {
            let dev = Device::from_pixmap(&pix).map_err(|e| anyhow!("{e}"))?;
            let mut ctm = Matrix::new_translate(-ox, -oy);
            if self.rotation != 0 {
                // Rotate about the origin, then move the page back into the positive quadrant.
                let (tx, ty) = match self.rotation {
                    90 => (ph, 0.0),
                    180 => (pw, ph),
                    _ => (0.0, pw),
                };
                ctm.concat(Matrix::new_rotate(self.rotation as f32));
                ctm.concat(Matrix::new_translate(tx, ty));
            }
            ctm.concat(Matrix::new_scale(scale, scale));
            p.run(&dev, &ctm).map_err(|e| anyhow!("page {page}: {e}"))?;
            // Dropping the device closes it, which flushes pending drawing into the pixmap.
        }
        let n = pix.n() as usize;
        let stride = pix.stride() as usize;
        let samples = pix.samples();
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h as usize {
            for px in samples[row * stride..row * stride + w as usize * n].chunks_exact(n) {
                pixels.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        Ok(Bitmap { width: w, height: h, pixels })
    }

    fn page_text(&self, page: usize) -> PageText {
        let Ok(p) = self.doc.load_page(page as i32) else { return PageText::default() };
        let Ok(tp) = p.to_text_page(TextPageFlags::empty()) else { return PageText::default() };
        let mut lines: Vec<(Vec<char>, Vec<Rect>)> = Vec::new();
        for block in tp.blocks() {
            if block.r#type() != TextBlockType::Text {
                continue;
            }
            for line in block.lines() {
                let (mut chars, mut rects) = (Vec::new(), Vec::new());
                for ch in line.chars() {
                    let Some(c) = ch.char() else { continue };
                    let q = ch.quad();
                    let xs = [q.ul.x, q.ur.x, q.ll.x, q.lr.x];
                    let ys = [q.ul.y, q.ur.y, q.ll.y, q.lr.y];
                    let r = mupdf::Rect {
                        x0: xs.iter().copied().fold(f32::MAX, f32::min),
                        y0: ys.iter().copied().fold(f32::MAX, f32::min),
                        x1: xs.iter().copied().fold(f32::MIN, f32::max),
                        y1: ys.iter().copied().fold(f32::MIN, f32::max),
                    };
                    chars.push(if c.is_whitespace() { ' ' } else { c });
                    rects.push(self.unrotated(page, r));
                }
                if !chars.is_empty() {
                    lines.push((chars, rects));
                }
            }
        }
        // Lines are joined in the page's own orientation, so the text doesn't depend on the view rotation.
        let mut text = join_visual_lines(lines);
        if self.rotation != 0 {
            for r in text.rects.iter_mut().filter(|r| **r != Rect::default()) {
                *r = self.rotate(page, *r);
            }
        }
        text
    }

    fn links(&self, page: usize) -> Vec<Link> {
        let Ok(p) = self.doc.load_page(page as i32) else { return Vec::new() };
        // The crate unwraps link resolution internally; a damaged link must not take the app down.
        let links = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.links().map(|it| it.collect::<Vec<_>>())));
        let Ok(Ok(links)) = links else { return Vec::new() };
        links
            .into_iter()
            .filter_map(|l| {
                let target = match &l.dest {
                    Some(d) => LinkTarget::Page(self.dest_pos(d)?),
                    None if l.uri.contains("://") || l.uri.starts_with("mailto:") => LinkTarget::Uri(l.uri.clone()),
                    None => LinkTarget::Other(l.uri.clone()),
                };
                Some(Link { rect: self.page_rect(page, l.bounds), target })
            })
            .collect()
    }

    fn outline(&self) -> Vec<OutlineItem> {
        let mut out = Vec::new();
        if let Ok(items) = self.doc.outlines() {
            self.walk_outline(&items, 0, &mut out);
        }
        out
    }

    fn page_bounds(&self) -> Vec<Rect> {
        self.bounds.clone()
    }

    fn metadata(&self) -> Metadata {
        let get = |name| self.doc.metadata(name).ok().filter(|s: &String| !s.trim().is_empty());
        Metadata { title: get(MetadataName::Title), author: get(MetadataName::Author) }
    }
}

/// MuPDF starts a new line at wide horizontal gaps ("3.5   Generalizations   29", an equation and its number).
/// Join consecutive pieces that share a baseline into one visual line, with a space between them, so line-based
/// features (ruler, line motions, headings, equation numbers) see what the reader sees. Pieces from different
/// columns aren't consecutive in MuPDF's reading order, so columns stay apart.
fn join_visual_lines(lines: Vec<(Vec<char>, Vec<Rect>)>) -> PageText {
    let bbox = |rects: &[Rect]| rects.iter().copied().reduce(|a, b| a.union(&b)).unwrap_or_default();
    let mut out = PageText::default();
    let mut prev: Option<Rect> = None;
    for (chars, rects) in lines {
        let b = bbox(&rects);
        let same_line = prev.is_some_and(|p| {
            let h = p.height().min(b.height()).max(1.0);
            (p.center_y() - b.center_y()).abs() < h * 0.5 && b.x0 >= p.x1 - 1.0
        });
        if same_line {
            let p = prev.unwrap();
            out.chars.push(' ');
            out.rects.push(Rect::new(p.x1, p.y0.min(b.y0), b.x0.max(p.x1), p.y1.max(b.y1)));
            prev = Some(p.union(&b));
        } else {
            if !out.chars.is_empty() {
                out.chars.push('\n');
                out.rects.push(Rect::default());
            }
            prev = Some(b);
        }
        out.chars.extend(chars);
        out.rects.extend(rects);
    }
    out
}
