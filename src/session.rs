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

/// Keyboard/mouse text selection within one page. `anchor` and `cursor` are
/// inclusive char indices.
#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub page: usize,
    pub anchor: usize,
    pub cursor: usize,
}

impl Selection {
    pub fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor) + 1)
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
    Ref { kind: RefKind, label: String, page: usize, char_idx: usize },
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

pub struct Session {
    pub path: PathBuf,
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
    /// Outstanding reference lookup: (request id, open as peek, display label).
    pub pending_ref: Option<(u64, bool, String)>,
    pub next_ref_id: u64,
    pub last_page: usize,
    pub first_frame: bool,
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub fn render_colors(theme: &Theme) -> Colors {
    Colors { fg: theme.page_fg, bg: theme.page_bg }
}

impl Session {
    pub fn open(path: &Path, cfg: &Config, theme: &Theme, ctx: &egui::Context) -> Result<Self> {
        let path = std::fs::canonicalize(path)?;
        let backend: Rc<dyn Backend> = Rc::from(doc::open(&path)?);
        let sizes = (0..backend.page_count()).map(|i| backend.page_size(i)).collect();
        let meta = backend.metadata();
        let file_name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let title =
            meta.title.filter(|t| t.len() > 3 && !t.ends_with(".dvi") && !t.contains("Microsoft Word")).unwrap_or(file_name);
        let wake = {
            let ctx = ctx.clone();
            move || ctx.request_repaint()
        };
        let pool = RenderPool::new(path.clone(), cfg.general.render_threads, render_colors(theme), wake.clone());
        let text = TextWorker::new(path.clone(), wake);
        let state = DocState::load(&path);
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
            pending_ref: None,
            next_ref_id: 1,
            last_page: usize::MAX,
            first_frame: true,
            path,
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
        let backend: Rc<dyn Backend> = Rc::from(doc::open(&self.path)?);
        let sizes = (0..backend.page_count()).map(|i| backend.page_size(i)).collect();
        self.view.set_sizes(sizes);
        self.outline = Rc::new(backend.outline());
        self.backend = backend;
        self.generation += 1;
        self.pool.reload(self.generation);
        self.text.send(crate::text::Request::Reload);
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

    /// True if the file changed on disk and looks complete.
    pub fn file_changed(&mut self) -> bool {
        if self.last_mtime_check.elapsed().as_millis() < 700 {
            return false;
        }
        self.last_mtime_check = Instant::now();
        let m = file_mtime(&self.path);
        m.is_some() && m != self.mtime
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
        let text = self.page_text(sel.page);
        let (s, e) = sel.range();
        Some(text.span_text(s, e))
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
