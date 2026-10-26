//! Document model shared by the UI, the render workers and the scripting layer.
//!
//! Everything here is expressed in PDF points with a top-left origin. Backends
//! are deliberately *not* `Send`: every thread that needs document access opens
//! its own instance through [`open`].

mod mupdf;

use std::path::Path;

use anyhow::Result;

pub use self::mupdf::MupdfDoc;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn center_y(&self) -> f32 {
        (self.y0 + self.y1) * 0.5
    }
    pub fn union(&self, o: &Rect) -> Rect {
        Rect::new(self.x0.min(o.x0), self.y0.min(o.y0), self.x1.max(o.x1), self.y1.max(o.y1))
    }
    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// A location inside the document: a page and a vertical offset on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DocPos {
    pub page: usize,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub enum LinkTarget {
    Page(DocPos),
    Uri(String),
    /// Something we can't follow (launch actions, javascript, remote files…).
    Other(String),
}

#[derive(Clone, Debug)]
pub struct Link {
    pub rect: Rect,
    pub target: LinkTarget,
}

#[derive(Clone, Debug)]
pub struct OutlineItem {
    pub title: String,
    pub depth: usize,
    pub target: Option<DocPos>,
}

/// Text of a page with one bounding box per `char`.
#[derive(Clone, Debug, Default)]
pub struct PageText {
    pub chars: Vec<char>,
    pub rects: Vec<Rect>,
}

impl PageText {
    pub fn span_text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.chars.len());
        let mut s = String::new();
        let mut i = start.min(end);
        while i < end {
            let c = self.chars[i];
            // Re-join words hyphenated across a line break.
            if c == '-' && self.chars.get(i + 1) == Some(&'\n') {
                i += 2;
                continue;
            }
            s.push(if c == '\n' { ' ' } else { c });
            i += 1;
        }
        s
    }

    /// Bounding boxes for a char span, merged per text line.
    pub fn span_rects(&self, start: usize, end: usize) -> Vec<Rect> {
        let mut out: Vec<Rect> = Vec::new();
        for i in start..end.min(self.rects.len()) {
            if self.chars[i] == '\n' {
                continue;
            }
            let r = self.rects[i];
            if r.is_empty() && r.width() <= 0.0 {
                continue;
            }
            match out.last_mut() {
                Some(last)
                    if (last.center_y() - r.center_y()).abs() < last.height().max(r.height()) * 0.5 && r.x0 >= last.x0 - 1.0 =>
                {
                    *last = last.union(&r)
                }
                _ => out.push(r),
            }
        }
        out
    }

    /// Char ranges `[start, end)` of each visual line on the page.
    pub fn lines(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, &c) in self.chars.iter().enumerate() {
            if c == '\n' {
                if i > start {
                    out.push((start, i));
                }
                start = i + 1;
            }
        }
        if start < self.chars.len() {
            out.push((start, self.chars.len()));
        }
        out
    }

    /// Char ranges of words (maximal runs of non-whitespace).
    pub fn words(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut start: Option<usize> = None;
        for (i, &c) in self.chars.iter().enumerate() {
            if c.is_whitespace() {
                if let Some(s) = start.take() {
                    out.push((s, i));
                }
            } else if start.is_none() {
                start = Some(i);
            }
        }
        if let Some(s) = start {
            out.push((s, self.chars.len()));
        }
        out
    }

    /// Index of the char whose box contains (or is nearest to) a point.
    pub fn char_at(&self, x: f32, y: f32) -> Option<usize> {
        let mut best = None;
        let mut best_d = f32::MAX;
        for (i, r) in self.rects.iter().enumerate() {
            if self.chars[i] == '\n' {
                continue;
            }
            let dx = if x < r.x0 {
                r.x0 - x
            } else if x > r.x1 {
                x - r.x1
            } else {
                0.0
            };
            let dy = if y < r.y0 {
                r.y0 - y
            } else if y > r.y1 {
                y - r.y1
            } else {
                0.0
            };
            // Weigh vertical distance more so we stick to the line under the cursor.
            let d = dx + dy * 4.0;
            if d < best_d {
                best_d = d;
                best = Some(i);
            }
        }
        best
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub title: Option<String>,
    #[allow(dead_code)] // exposed for scripting / AI context later
    pub author: Option<String>,
}

/// An RGBA8 bitmap (premultiplied, but pages are opaque).
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A document as the app sees it: page sizes, rendering, text with per-char boxes, links and the outline, in page
/// points with a top-left origin. One instance per thread (`open_with`), since MuPDF documents aren't `Send`.
pub trait Backend {
    fn page_count(&self) -> usize;
    /// Size of a page in points.
    fn page_size(&self, page: usize) -> (f32, f32);
    fn page_label(&self, page: usize) -> Option<String>;
    /// Render the region `[x, x+w) × [y, y+h)` in device pixels of the page rendered at `scale` px/pt.
    fn render(&self, page: usize, scale: f32, x: i32, y: i32, w: u32, h: u32) -> Result<Bitmap>;
    fn page_text(&self, page: usize) -> PageText;
    fn links(&self, page: usize) -> Vec<Link>;
    fn outline(&self) -> Vec<OutlineItem>;
    fn metadata(&self) -> Metadata;
}

/// `std::fs::canonicalize`, minus the `\\?\` prefix Windows adds (`\\?\C:\a.pdf` → `C:\a.pdf`), so paths stay
/// readable in titles and stable as state keys.
pub fn canonicalize(path: &Path) -> std::io::Result<std::path::PathBuf> {
    let p = std::fs::canonicalize(path)?;
    if cfg!(windows)
        && let Some(rest) = p.to_str().and_then(|s| s.strip_prefix(r"\\?\"))
        && rest.as_bytes().get(1) == Some(&b':')
    {
        return Ok(rest.into());
    }
    Ok(p)
}

/// Opening failures the UI handles specially (by asking for a password).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    NeedsPassword,
    WrongPassword,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OpenError::NeedsPassword => "the document is password-protected",
            OpenError::WrongPassword => "wrong password",
        })
    }
}

impl std::error::Error for OpenError {}

/// How a document is opened. Every thread opens its own instance with the same options.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenOptions {
    /// Clockwise view rotation in degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    pub password: Option<String>,
}

pub fn open(path: &Path) -> Result<Box<dyn Backend>> {
    open_with(path, &OpenOptions::default())
}

pub fn open_with(path: &Path, opts: &OpenOptions) -> Result<Box<dyn Backend>> {
    Ok(Box::new(MupdfDoc::open(path, opts)?))
}
