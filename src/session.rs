//! An open document: backend, view, workers, caches and per-document state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Instant, SystemTime};

use anyhow::Result;
use eframe::egui;

use crate::config::{Config, PageMode, Theme};
use crate::doc::{self, Backend, DocPos, Link, OutlineItem, PageText, Rect};
use crate::refs::RefKind;
use crate::render::{Colors, RenderPool, TileKey};
use crate::store::DocState;
use crate::text::{SearchHit, TextWorker};
use crate::view::{Fit, View};

pub struct TexEntry {
    pub handle: egui::TextureHandle,
    pub last_used: Instant,
    pub bytes: usize,
}

pub struct SearchState {
    pub query: String,
    pub generation: u32,
    pub hits: Vec<SearchHit>,
    pub current: Option<usize>,
    pub done: bool,
    pub backward: bool,
    /// Where the search started; the first jump goes to the first hit after it.
    pub origin: DocPos,
    pub jump_on_first: bool,
}

/// A place in the text: a page and a char index on it, ordered by page, then char.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPos {
    pub page: usize,
    pub idx: usize,
}

/// Keyboard/mouse text selection. `anchor` and `cursor` are inclusive and can be on different pages.
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub anchor: TextPos,
    pub cursor: TextPos,
}

impl Selection {
    pub fn on_page(page: usize, anchor: usize, cursor: usize) -> Self {
        Self { anchor: TextPos { page, idx: anchor }, cursor: TextPos { page, idx: cursor } }
    }

    /// First and last selected positions.
    pub fn bounds(&self) -> (TextPos, TextPos) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    /// The selected chars `[start, end)` on `page`, which has `len` chars, if the selection reaches it.
    pub fn range_on(&self, page: usize, len: usize) -> Option<(usize, usize)> {
        let (a, b) = self.bounds();
        if page < a.page || page > b.page {
            return None;
        }
        let start = if page == a.page { a.idx } else { 0 };
        let end = if page == b.page { b.idx + 1 } else { len };
        Some((start.min(len), end.min(len)))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ruler {
    pub page: usize,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub enum HintAction {
    Link(doc::LinkTarget),
    Ref {
        kind: RefKind,
        label: String,
        page: usize,
        char_idx: usize,
    },
    /// Visual-mode start, first step: a text line (index into `PageText::lines`).
    Line(usize),
    /// Visual-mode start, second step: a word, as a char range `[start, end)`.
    Word(usize, usize),
    /// A highlight to edit (index into the document's highlights).
    Highlight(usize),
}

#[derive(Clone, Debug)]
pub struct HintItem {
    pub label: String,
    pub page: usize,
    pub rect: Rect,
    pub action: HintAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintMode {
    Jump,
    Peek,
    /// Choosing where a visual-mode selection starts: a line, then a word on it.
    Select,
    /// Choosing a highlight to edit.
    Edit,
}

pub struct Hints {
    pub mode: HintMode,
    pub items: Vec<HintItem>,
    pub typed: String,
}

pub struct Peek {
    pub page: usize,
    /// Top of the peek window, in page points.
    pub y: f32,
    pub zoom: f32,
    pub target: Option<Rect>,
    pub title: String,
    pub dest: DocPos,
}

/// One open document: its backend, view, render and text workers, caches, and per-document state (position,
/// search, selection, hints, peek, saved highlights and marks). The active tab's session is `App::doc`.
pub struct Session {
    pub path: PathBuf,
    /// Rotation and password, for every thread that opens the document. The password is never saved.
    pub opts: doc::OpenOptions,
    pub title: String,
    pub backend: Rc<dyn Backend>,
    pub view: View,
    pub pool: RenderPool,
    pub text: TextWorker,
    pub generation: u32,
    pub textures: HashMap<TileKey, TexEntry>,
    pub tex_bytes: usize,
    pub page_texts: HashMap<usize, Rc<PageText>>,
    pub links: HashMap<usize, Rc<Vec<Link>>>,
    pub outline: Rc<Vec<OutlineItem>>,
    pub state: DocState,
    pub dirty: bool,
    pub last_save: Instant,
    pub history: Vec<DocPos>,
    pub hist_idx: usize,
    pub search: Option<SearchState>,
    pub selection: Option<Selection>,
    pub ruler: Option<Ruler>,
    pub hints: Option<Hints>,
    pub peek: Option<Peek>,
    pub page_mode: PageMode,
    pub mtime: Option<SystemTime>,
    pub last_mtime_check: Instant,
    /// Size and mtime seen at the last poll after a change, to wait until the file stops growing.
    pub pending_change: Option<(u64, SystemTime)>,
    /// Outstanding reference lookup: (request id, open as peek, display label).
    pub pending_ref: Option<(u64, bool, String)>,
    pub next_ref_id: u64,
    pub last_page: usize,
    pub first_frame: bool,
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Does the file end like a finished PDF (`%%EOF` in its last ~1 KB)?
fn ends_with_eof(path: &Path, len: u64) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    let mut buf = Vec::with_capacity(1024);
    if f.seek(SeekFrom::Start(len.saturating_sub(1024))).is_err() || f.take(1024).read_to_end(&mut buf).is_err() {
        return false;
    }
    buf.windows(5).any(|w| w == b"%%EOF")
}

/// A title for documents without one in their metadata (most arXiv preprints): the largest text in the top
/// half of the first page, with the lines that continue it, if it stands out from the body text.
pub fn title_from_text(text: &PageText, page_height: f32) -> Option<String> {
    let median = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied()
    };
    let lines: Vec<(usize, usize, f32, Rect)> = text
        .lines()
        .into_iter()
        .filter_map(|(s, e)| {
            let heights = (s..e).filter(|&i| !text.chars[i].is_whitespace()).map(|i| text.rects[i].height()).collect();
            let rect = text.span_rects(s, e).into_iter().reduce(|a, b| a.union(&b))?;
            Some((s, e, median(heights)?, rect))
        })
        .collect();
    let body = median(lines.iter().map(|l| l.2).collect())?;
    let is_candidate = |&(s, e, _, r): &(usize, usize, f32, Rect)| {
        let t = text.span_text(s, e);
        r.y0 < page_height * 0.5 && t.chars().filter(|c| c.is_alphabetic()).count() >= 3 && !t.contains("arXiv")
    };
    let top = lines.iter().filter(|l| is_candidate(l)).map(|l| l.2).fold(0.0, f32::max);
    if top < body * 1.2 {
        return None;
    }
    let first = lines.iter().position(|l| is_candidate(l) && l.2 >= top * 0.95)?;
    let mut title = Vec::new();
    let mut prev: Option<Rect> = None;
    for l in lines[first..].iter().take_while(|l| l.2 >= top * 0.9) {
        // A continuation line sits right below the previous one.
        if prev.is_some_and(|p| l.3.y0 - p.y1 > l.2 * 1.5) {
            break;
        }
        title.push(text.span_text(l.0, l.1).trim().to_string());
        prev = Some(l.3);
    }
    let title: String = title.join(" ").chars().take(150).collect();
    (title.chars().filter(|c| c.is_alphabetic()).count() >= 4).then_some(title)
}

pub fn render_colors(theme: &Theme) -> Colors {
    Colors { fg: theme.page_fg, bg: theme.page_bg }
}

impl Session {
    pub fn open(path: &Path, password: Option<String>, cfg: &Config, theme: &Theme, ctx: &egui::Context) -> Result<Self> {
        let path = doc::canonicalize(path)?;
        let state = DocState::load(&path);
        let opts = doc::OpenOptions { rotation: state.rotation, password };
        let backend: Rc<dyn Backend> = Rc::from(doc::open_with(&path, &opts)?);
        let sizes = (0..backend.page_count()).map(|i| backend.page_size(i)).collect();
        let meta = backend.metadata();
        let file_name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let title = meta
            .title
            .filter(|t| t.len() > 3 && !t.ends_with(".dvi") && !t.contains("Microsoft Word"))
            .or_else(|| title_from_text(&backend.page_text(0), backend.page_size(0).1))
            .unwrap_or(file_name);
        let wake = {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        let pool = RenderPool::new(path.clone(), opts.clone(), cfg.general.render_threads, render_colors(theme), wake.clone());
        let text = TextWorker::new(path.clone(), opts.clone(), wake);
        let mut view = View::new(sizes, cfg.general.page_gap);
        if let Some(z) = state.zoom.filter(|_| !state.fit_width) {
            view.fit = Fit::None;
            view.zoom = z;
            view.set_zoom(z, 0.0);
        }
        let outline = Rc::new(backend.outline());
        Ok(Self {
            title,
            backend,
            view,
            pool,
            text,
            generation: 0,
            textures: HashMap::new(),
            tex_bytes: 0,
            page_texts: HashMap::new(),
            links: HashMap::new(),
            outline,
            state,
            dirty: false,
            last_save: Instant::now(),
            history: Vec::new(),
            hist_idx: 0,
            search: None,
            selection: None,
            ruler: None,
            hints: None,
            peek: None,
            page_mode: cfg.general.page_mode,
            mtime: file_mtime(&path),
            last_mtime_check: Instant::now(),
            pending_change: None,
            pending_ref: None,
            next_ref_id: 1,
            last_page: usize::MAX,
            first_frame: true,
            path,
            opts,
        })
    }

    pub fn page_text(&mut self, page: usize) -> Rc<PageText> {
        if let Some(t) = self.page_texts.get(&page) {
            return t.clone();
        }
        let t = Rc::new(self.backend.page_text(page));
        self.page_texts.insert(page, t.clone());
        t
    }

    pub fn page_links(&mut self, page: usize) -> Rc<Vec<Link>> {
        if let Some(l) = self.links.get(&page) {
            return l.clone();
        }
        let l = Rc::new(self.backend.links(page));
        self.links.insert(page, l.clone());
        l
    }

    /// Reopen the file after it changed on disk (e.g. LaTeX rebuild).
    pub fn reload(&mut self) -> Result<()> {
        let backend: Rc<dyn Backend> = Rc::from(doc::open_with(&self.path, &self.opts)?);
        let sizes = (0..backend.page_count()).map(|i| backend.page_size(i)).collect();
        self.view.set_sizes(sizes);
        self.outline = Rc::new(backend.outline());
        self.backend = backend;
        self.generation += 1;
        self.pool.reload(self.generation, self.opts.clone());
        self.text.send(crate::text::Request::Reload(self.opts.clone()));
        self.page_texts.clear();
        self.links.clear();
        self.selection = None;
        self.hints = None;
        self.mtime = file_mtime(&self.path);
        if let Some(s) = self.search.take() {
            self.start_search(s.query, s.backward, self.view.pos_at(0.0), false);
        }
        Ok(())
    }

    /// Turn the view by `degrees` (a multiple of 90, clockwise), keeping the current page in view. Highlight boxes
    /// are recomputed from their characters, since they're stored in view coordinates.
    pub fn rotate(&mut self, degrees: i32) -> Result<()> {
        let page = self.view.current_page();
        self.opts.rotation = (self.opts.rotation as i32 + degrees).rem_euclid(360) as u16;
        self.state.rotation = self.opts.rotation;
        self.reload()?;
        let mut highlights = std::mem::take(&mut self.state.highlights);
        for h in &mut highlights {
            let t = self.page_text(h.page);
            h.rects = t.span_rects(h.start, h.end).into_iter().map(Into::into).collect();
        }
        self.state.highlights = highlights;
        self.ruler = None;
        self.view.goto(DocPos { page, y: 0.0 }, 0.0, false);
        self.save_state();
        Ok(())
    }

    /// True if the file changed on disk and looks complete: size and mtime unchanged since the
    /// previous poll, and a `%%EOF` near the end. A file still being written waits for a later poll.
    pub fn file_changed(&mut self) -> bool {
        if self.last_mtime_check.elapsed().as_millis() < 700 {
            return false;
        }
        self.last_mtime_check = Instant::now();
        let Ok(meta) = std::fs::metadata(&self.path) else { return false };
        let Ok(m) = meta.modified() else { return false };
        if Some(m) == self.mtime {
            self.pending_change = None;
            return false;
        }
        let seen = (meta.len(), m);
        if self.pending_change != Some(seen) {
            self.pending_change = Some(seen);
            return false;
        }
        ends_with_eof(&self.path, meta.len())
    }

    pub fn start_search(&mut self, query: String, backward: bool, origin: DocPos, jump: bool) {
        if query.is_empty() {
            self.search = None;
            self.text.cancel_search();
            return;
        }
        let generation = self.text.search(query.clone());
        self.search = Some(SearchState {
            query,
            generation,
            hits: Vec::new(),
            current: None,
            done: false,
            backward,
            origin,
            jump_on_first: jump,
        });
    }

    pub fn push_history(&mut self) {
        let pos = self.view.pos_at(0.0);
        self.history.truncate(self.hist_idx);
        if self.history.last().is_none_or(|p| p.page != pos.page || (p.y - pos.y).abs() > 20.0) {
            self.history.push(pos);
        }
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.hist_idx = self.history.len();
    }

    /// Jump to a position, recording it in the jump list.
    pub fn jump(&mut self, pos: DocPos, frac: f32, animate: bool) {
        self.push_history();
        self.view.goto(pos, frac, animate);
    }

    pub fn save_state(&mut self) {
        self.state.pos = Some(self.view.pos_at(0.0));
        self.state.zoom = Some(self.view.zoom);
        self.state.fit_width = self.view.fit == Fit::Width;
        self.state.save();
        self.dirty = false;
        self.last_save = Instant::now();
    }

    /// The deepest outline entry at or before the current position.
    pub fn current_section(&self) -> Option<usize> {
        let pos = self.view.pos_at(0.3);
        let mut best = None;
        for (i, item) in self.outline.iter().enumerate() {
            if let Some(t) = item.target
                && (t.page, t.y as i64) <= (pos.page, pos.y as i64 + 1)
            {
                best = Some(i);
            }
        }
        best
    }

    pub fn selection_text(&mut self) -> Option<String> {
        let sel = self.selection?;
        let (a, b) = sel.bounds();
        let mut parts = Vec::new();
        for page in a.page..=b.page {
            let text = self.page_text(page);
            if let Some((s, e)) = sel.range_on(page, text.chars.len()) {
                parts.push(text.span_text(s, e));
            }
        }
        Some(parts.join(" "))
    }

    pub fn evict_textures(&mut self, budget: usize) {
        let now = Instant::now();
        let stale: Vec<TileKey> = self
            .textures
            .iter()
            .filter(|(k, v)| k.generation != self.generation || now.duration_since(v.last_used).as_secs_f32() > 4.0)
            .map(|(k, _)| *k)
            .collect();
        for k in stale {
            if let Some(t) = self.textures.remove(&k) {
                self.tex_bytes -= t.bytes;
            }
        }
        if self.tex_bytes > budget {
            let mut by_age: Vec<(TileKey, Instant)> = self.textures.iter().map(|(k, v)| (*k, v.last_used)).collect();
            by_age.sort_by_key(|(_, t)| *t);
            for (k, _) in by_age {
                if self.tex_bytes <= budget * 3 / 4 {
                    break;
                }
                if let Some(t) = self.textures.remove(&k) {
                    self.tex_bytes -= t.bytes;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_ranges_per_page() {
        let at = |page, idx| TextPos { page, idx };
        // Backwards across three pages: page 2 char 5 back to page 0 char 40.
        let sel = Selection { anchor: at(2, 5), cursor: at(0, 40) };
        assert_eq!(sel.bounds(), (at(0, 40), at(2, 5)));
        assert_eq!(sel.range_on(0, 100), Some((40, 100)));
        assert_eq!(sel.range_on(1, 80), Some((0, 80)));
        assert_eq!(sel.range_on(2, 90), Some((0, 6)));
        assert_eq!(sel.range_on(3, 90), None);
        assert_eq!(Selection::on_page(4, 7, 3).range_on(4, 50), Some((3, 8)));
    }

    #[test]
    fn eof_marker_near_end() {
        let path = std::env::temp_dir().join(format!("folio-eof-test-{}.pdf", std::process::id()));
        let mut body = b"%PDF-1.5\n".to_vec();
        body.extend(std::iter::repeat_n(b'x', 4000));
        std::fs::write(&path, &body).unwrap();
        assert!(!ends_with_eof(&path, body.len() as u64));
        body.extend(b"\nstartxref\n123\n%%EOF\n");
        std::fs::write(&path, &body).unwrap();
        assert!(ends_with_eof(&path, body.len() as u64));
        let _ = std::fs::remove_file(path);
    }
}
