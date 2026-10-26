//! Tabs: several open documents in one window.
//!
//! The active document stays in `App::doc`, so everything that works on "the document" is unchanged. The tab list
//! holds the other documents: loaded ones keep their state and workers (their textures are dropped to save
//! memory), restored ones are only a path until they're first shown.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::app::{App, Mode, PromptKind};
use crate::config;
use crate::session::Session;
use crate::ui::picker::{PickData, PickItem, Picker, PickerKind};

pub enum Tab {
    /// The document in `App::doc`.
    Active,
    Loaded(Box<Session>),
    /// Restored from the last session, opened when first shown.
    Lazy(PathBuf),
}

impl Tab {
    /// The tab's file. The active tab's is the open document's, or the one being opened into it.
    fn path<'a>(&'a self, active: Option<&'a Session>, pending: Option<&'a Path>) -> Option<&'a Path> {
        match self {
            Tab::Active => active.map(|d| d.path.as_path()).or(pending),
            Tab::Loaded(s) => Some(&s.path),
            Tab::Lazy(p) => Some(p),
        }
    }
}

/// The most recently closed tabs, for `reopen_tab`.
const CLOSED_KEPT: usize = 20;

impl App {
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Paths of all tabs, in order.
    pub fn tab_paths(&self) -> Vec<PathBuf> {
        self.tabs
            .iter()
            .filter_map(|t| t.path(self.doc.as_ref(), self.active_pending.as_deref()).map(Path::to_path_buf))
            .collect()
    }

    /// Titles of all tabs, in order (file names for tabs not loaded yet).
    pub fn tab_titles(&self) -> Vec<String> {
        self.tabs
            .iter()
            .map(|t| match t {
                Tab::Active => match (&self.doc, &self.active_pending) {
                    (Some(d), _) => d.title.clone(),
                    (None, Some(p)) => format!("Opening {}…", p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()),
                    (None, None) => String::new(),
                },
                Tab::Loaded(s) => s.title.clone(),
                Tab::Lazy(p) => self
                    .global
                    .recent
                    .iter()
                    .find(|r| &r.path == p)
                    .map(|r| r.title.clone())
                    .unwrap_or_else(|| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            })
            .collect()
    }

    pub(crate) fn tab_of(&self, path: &Path) -> Option<usize> {
        let path = crate::doc::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.tabs.iter().position(|t| t.path(self.doc.as_ref(), self.active_pending.as_deref()) == Some(path.as_path()))
    }

    /// Put a newly opened session in the current tab (or the first one).
    pub(crate) fn place_in_current_tab(&mut self, s: Session) {
        if self.tabs.is_empty() {
            self.tabs.push(Tab::Active);
            self.active_tab = 0;
        }
        self.tabs[self.active_tab] = Tab::Active;
        self.active_pending = None;
        self.doc = Some(s);
    }

    /// Make room for a new active tab right after the current one.
    pub(crate) fn insert_tab_after_active(&mut self) {
        self.stash_active();
        let i = (self.active_tab + 1).min(self.tabs.len());
        self.tabs.insert(i, Tab::Active);
        self.active_tab = i;
    }

    /// If the document is open in another tab, switch to that tab. Returns true if it did.
    pub(crate) fn switch_if_open(&mut self, path: &Path) -> bool {
        match self.tab_of(path) {
            // Already being opened into the active tab.
            Some(i) if i == self.active_tab && self.doc.is_none() => true,
            Some(i) if i != self.active_tab => {
                self.switch_tab(i);
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    /// Open a document in a new tab after the current one (or switch to it if it's open already).
    pub fn open_in_new_tab(&mut self, path: &Path) {
        if !self.switch_if_open(path) {
            self.request_open(path, None, crate::loader::Target::NewTab, None);
        }
    }

    /// Move the active document into its tab slot, so another one can become active.
    fn stash_active(&mut self) {
        // A tab still being opened goes back to waiting; its document lands there when it's read.
        if self.doc.is_none()
            && let Some(p) = self.active_pending.take()
            && let Some(slot @ Tab::Active) = self.tabs.get_mut(self.active_tab)
        {
            *slot = Tab::Lazy(p);
        }
        if let Some(mut d) = self.doc.take() {
            d.save_state();
            d.pool.request(Vec::new());
            d.textures.clear();
            d.tex_bytes = 0;
            d.hints = None;
            d.peek = None;
            self.tabs[self.active_tab] = Tab::Loaded(Box::new(d));
        }
    }

    pub fn switch_tab(&mut self, i: usize) {
        if i >= self.tabs.len() || (i == self.active_tab && self.doc.is_some()) {
            return;
        }
        self.stash_active();
        self.activate(i);
    }

    /// Make tab `j` the active one (nothing is active now). A restored tab is opened; if that fails, the tab is
    /// dropped and its neighbor is shown instead.
    pub(crate) fn activate(&mut self, j: usize) {
        if self.tabs.is_empty() {
            self.active_tab = 0;
            self.doc = None;
            self.after_switch();
            return;
        }
        let j = j.min(self.tabs.len() - 1);
        self.active_tab = j;
        match std::mem::replace(&mut self.tabs[j], Tab::Active) {
            Tab::Loaded(s) => {
                self.doc = Some(*s);
                // It may have changed on disk while in the background.
                if let Some(d) = &mut self.doc
                    && std::fs::metadata(&d.path).and_then(|m| m.modified()).ok() != d.mtime
                    && let Err(e) = d.reload()
                {
                    self.warn(format!("reload: {e:#}"));
                }
                self.after_switch();
            }
            Tab::Lazy(path) => {
                self.active_pending = Some(path.clone());
                self.after_switch();
                self.request_open(&path, None, crate::loader::Target::Lazy, None);
            }
            Tab::Active => self.after_switch(),
        }
    }

    pub(crate) fn after_switch(&mut self) {
        self.mode = Mode::Normal;
        self.picker = None;
        // Prompts about the previous document (a note, a search, a page) would act on this one.
        if self.prompt.as_ref().is_some_and(|p| !matches!(p.kind, PromptKind::Password(_) | PromptKind::Lua(_))) {
            self.prompt = None;
        }
        self.disp.reset();
        let title = match &self.doc {
            Some(d) => format!("{} — {}", d.title, config::APP_NAME),
            None => config::APP_NAME.to_string(),
        };
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        self.sync_script_doc();
        if let Some(d) = &mut self.doc {
            d.dirty = true;
            d.last_page = usize::MAX;
        }
        self.ctx.request_repaint();
    }

    pub fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if let Some(p) = self.tabs[i].path(self.doc.as_ref(), self.active_pending.as_deref()).map(Path::to_path_buf) {
            self.closed_tabs.retain(|c| c != &p);
            self.closed_tabs.push(p);
            if self.closed_tabs.len() > CLOSED_KEPT {
                self.closed_tabs.remove(0);
            }
        }
        if i != self.active_tab {
            if let Tab::Loaded(mut s) = self.tabs.remove(i) {
                s.save_state();
            }
            if i < self.active_tab {
                self.active_tab -= 1;
            }
            return;
        }
        if let Some(mut d) = self.doc.take() {
            d.save_state();
            let page = d.view.current_page();
            self.global.touch_recent(&d.path, &d.title, page);
        }
        self.active_pending = None;
        self.tabs.remove(i);
        // The tab to the right takes its place, like in a browser.
        self.activate(i);
    }

    pub fn reopen_tab(&mut self) {
        match self.closed_tabs.pop() {
            Some(p) => self.open_in_new_tab(&p),
            None => self.notify("No closed tab to reopen"),
        }
    }

    /// Save every loaded tab's state and the tab list (for `restore_tabs`).
    pub fn save_tabs(&mut self) {
        for t in &mut self.tabs {
            if let Tab::Loaded(s) = t {
                s.save_state();
            }
        }
        self.global.tabs = self.tab_paths();
        self.global.active_tab = self.active_tab.min(self.tabs.len().saturating_sub(1));
    }

    /// Recreate the last session's tabs; only the active one is opened now.
    pub fn restore_tabs(&mut self) -> bool {
        let saved_active = self.global.tabs.get(self.global.active_tab).cloned();
        let paths: Vec<PathBuf> = self.global.tabs.iter().filter(|p| p.exists()).cloned().collect();
        if paths.is_empty() {
            return false;
        }
        // The saved active tab, found by path (tabs before it may have gone missing).
        let active = paths.iter().position(|p| Some(p) == saved_active.as_ref()).unwrap_or(0);
        self.tabs = paths.into_iter().map(Tab::Lazy).collect();
        self.activate(active);
        self.doc.is_some()
    }

    /// Run `f` on every loaded document (the active one and background tabs).
    pub fn for_each_session(&mut self, mut f: impl FnMut(&mut Session)) {
        if let Some(d) = &mut self.doc {
            f(d);
        }
        for t in &mut self.tabs {
            if let Tab::Loaded(s) = t {
                f(s);
            }
        }
    }

    pub fn open_tabs_picker(&mut self) {
        if self.tabs.is_empty() {
            return self.notify("No open documents");
        }
        let items: Vec<PickItem> = self
            .tab_titles()
            .into_iter()
            .zip(self.tab_paths())
            .enumerate()
            .map(|(i, (title, path))| {
                let dir = path.parent().map(crate::app::tilde).unwrap_or_default();
                PickItem::new(title, PickData::Tab(i)).detail(dir).hint(if i < 9 {
                    format!("Alt-{}", i + 1)
                } else {
                    String::new()
                })
            })
            .collect();
        let mut p = Picker::new(PickerKind::Tabs, "Tabs  ·  Ctrl-D closes", items);
        p.select_item(self.active_tab);
        self.picker = Some(p);
    }

    /// The tab bar along the top of the window.
    pub fn tab_bar(&mut self, ui: &mut egui::Ui) {
        use crate::ui::style::{c, with_alpha};
        use egui::{Align2, CornerRadius, FontId, Sense, Stroke, Vec2};
        let t = self.theme.clone();
        let titles = self.tab_titles();
        let n = titles.len().max(1);
        let full = ui.available_rect_before_wrap();
        let tab_w = ((full.width() - 8.0) / n as f32).clamp(90.0, 240.0);
        let mut switch = None;
        let mut close = None;
        for (i, title) in titles.iter().enumerate() {
            let rect = egui::Rect::from_min_size(
                full.min + Vec2::new(4.0 + i as f32 * tab_w, 4.0),
                Vec2::new(tab_w - 4.0, full.height() - 4.0),
            );
            if rect.left() > full.right() {
                break;
            }
            let resp = ui.interact(rect, egui::Id::unique(("tab", i)), Sense::click());
            let active = i == self.active_tab;
            let painter = ui.painter_at(full);
            let fill = if active {
                c(t.bg)
            } else if resp.hovered() {
                with_alpha(t.text, 12)
            } else {
                egui::Color32::TRANSPARENT
            };
            painter.rect_filled(rect, CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, fill);
            if active {
                painter.line_segment(
                    [rect.left_top() + Vec2::new(6.0, 0.0), rect.right_top() - Vec2::new(6.0, 0.0)],
                    Stroke::new(2.0, c(t.accent)),
                );
            }
            let close_rect = egui::Rect::from_center_size(rect.right_center() - Vec2::new(12.0, 0.0), Vec2::splat(16.0));
            let close_resp = ui.interact(close_rect, egui::Id::unique(("tab_close", i)), Sense::click());
            let label_rect =
                egui::Rect::from_min_max(rect.min + Vec2::new(10.0, 0.0), egui::pos2(close_rect.left() - 4.0, rect.bottom()));
            let col = if active { c(t.text) } else { c(t.text_dim) };
            let galley = painter.layout_no_wrap(title.clone(), FontId::proportional(13.0), col);
            painter.with_clip_rect(label_rect).galley(
                egui::pos2(label_rect.left(), rect.center().y - galley.size().y / 2.0),
                galley,
                col,
            );
            if resp.hovered() || active {
                let xc = if close_resp.hovered() { c(t.text) } else { c(t.text_dim) };
                painter.text(close_rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(14.0), xc);
            }
            if close_resp.clicked() || resp.middle_clicked() {
                close = Some(i);
            } else if resp.clicked() {
                switch = Some(i);
            }
            resp.on_hover_text(self.tab_paths().get(i).map(|p| crate::app::tilde(p)).unwrap_or_default());
        }
        if let Some(i) = close {
            self.close_tab(i);
        } else if let Some(i) = switch {
            self.switch_tab(i);
        }
    }
}
