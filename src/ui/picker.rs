//! A fuzzy picker overlay used for the command palette, TOC, files, bookmarks…

use std::path::PathBuf;

use eframe::egui::{self, Align2, FontId, Key, RichText, Stroke};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

use super::style::{c, radius, with_alpha};
use crate::config::Theme;
use crate::doc::DocPos;

#[derive(Clone, Debug)]
pub enum PickData {
    Command(String),
    Pos(DocPos),
    Path(PathBuf),
    Highlight(usize),
    Bookmark(usize),
    Mark(char),
    Theme(String),
    /// Index into the tab list.
    Tab(usize),
    Lua(usize, usize),
    None,
}

#[derive(Clone, Debug)]
pub struct PickItem {
    pub label: String,
    pub detail: String,
    /// Right-aligned hint (key binding, page number…).
    pub hint: String,
    pub depth: usize,
    pub data: PickData,
}

impl PickItem {
    pub fn new(label: impl Into<String>, data: PickData) -> Self {
        Self { label: label.into(), detail: String::new(), hint: String::new(), depth: 0, data }
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = d.into();
        self
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = h.into();
        self
    }
    pub fn depth(mut self, d: usize) -> Self {
        self.depth = d;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerKind {
    Commands,
    Toc,
    Files,
    Recent,
    Bookmarks,
    Highlights,
    Marks,
    Themes,
    Help,
    Tabs,
    Lua,
}

pub struct Picker {
    pub kind: PickerKind,
    pub title: String,
    pub query: String,
    pub items: Vec<PickItem>,
    /// Indices into `items`, best first.
    pub filtered: Vec<usize>,
    pub selected: usize,
    /// Keep document order instead of sorting by score (TOC).
    pub keep_order: bool,
    pub scroll_to_selected: bool,
    pub placeholder: String,
    /// Move the text cursor to the end on the next frame (after programmatic edits).
    pub move_cursor_end: bool,
    matcher: Matcher,
}

pub enum PickerEvent {
    None,
    Close,
    Accept,
    /// Shift-Enter: open in a new tab (file pickers); elsewhere the same as Enter.
    AcceptAlt,
    /// Ctrl-D on an item (delete bookmark / highlight).
    Delete,
    /// Tab: complete (used by the file picker).
    Complete,
    QueryChanged,
}

impl Picker {
    pub fn new(kind: PickerKind, title: impl Into<String>, items: Vec<PickItem>) -> Self {
        let mut p = Self {
            kind,
            title: title.into(),
            query: String::new(),
            items,
            filtered: Vec::new(),
            selected: 0,
            keep_order: false,
            scroll_to_selected: true,
            placeholder: "Type to filter…".into(),
            move_cursor_end: false,
            matcher: Matcher::new(nucleo_matcher::Config::DEFAULT),
        };
        p.refilter();
        p
    }

    pub fn set_items(&mut self, items: Vec<PickItem>) {
        self.items = items;
        self.refilter();
    }

    pub fn select_item(&mut self, item_index: usize) {
        if let Some(i) = self.filtered.iter().position(|&f| f == item_index) {
            self.selected = i;
            self.scroll_to_selected = true;
        }
    }

    pub fn current(&self) -> Option<&PickItem> {
        self.filtered.get(self.selected).map(|&i| &self.items[i])
    }

    /// The part of the query used for matching (the file picker matches basenames).
    fn match_query(&self) -> &str {
        if self.kind == PickerKind::Files {
            self.query.rsplit(['/', '\\']).next().unwrap_or("")
        } else {
            self.query.split_whitespace().next().filter(|_| self.kind == PickerKind::Commands).unwrap_or(&self.query)
        }
    }

    pub fn refilter(&mut self) {
        let q = self.match_query().to_string();
        if q.trim().is_empty() {
            self.filtered = (0..self.items.len()).collect();
        } else {
            let pattern = Pattern::parse(&q, CaseMatching::Smart, Normalization::Smart);
            let mut buf = Vec::new();
            let mut scored: Vec<(usize, u32)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, it)| {
                    let hay = if it.detail.is_empty() || self.kind == PickerKind::Files {
                        it.label.clone()
                    } else {
                        format!("{} {}", it.label, it.detail)
                    };
                    pattern.score(Utf32Str::new(&hay, &mut buf), &mut self.matcher).map(|s| (i, s))
                })
                .collect();
            if !self.keep_order {
                scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            }
            if self.kind == PickerKind::Commands {
                // The exact command first, then shorter prefix matches: `zoo` → zoom, zoom_in, zoom_out.
                let q = q.trim();
                let items = &self.items;
                let rank = |i: usize| match &items[i].data {
                    PickData::Command(name) if name == q => (0, 0),
                    PickData::Command(name) if name.starts_with(q) => (1, name.len()),
                    _ => (2, 0),
                };
                scored.sort_by_key(|&(i, _)| rank(i));
            }
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        if !q.trim().is_empty() {
            self.selected = 0;
        }
        self.scroll_to_selected = true;
    }

    fn move_by(&mut self, d: i32) {
        let n = self.filtered.len() as i32;
        if n == 0 {
            return;
        }
        self.selected = (self.selected as i32 + d).rem_euclid(n) as usize;
        self.scroll_to_selected = true;
    }

    /// Consume navigation keys before the text field sees them.
    pub fn handle_keys(&mut self, ctx: &egui::Context) -> PickerEvent {
        use egui::Modifiers as M;
        let mut ev = PickerEvent::None;
        ctx.input_mut(|i| {
            if i.consume_key(M::NONE, Key::Escape) || i.consume_key(M::CTRL, Key::OpenBracket) {
                ev = PickerEvent::Close;
            } else if i.consume_key(M::NONE, Key::Enter) {
                ev = PickerEvent::Accept;
            } else if i.consume_key(M::SHIFT, Key::Enter) {
                ev = PickerEvent::AcceptAlt;
            } else if i.consume_key(M::NONE, Key::ArrowDown) || i.consume_key(M::CTRL, Key::N) || i.consume_key(M::CTRL, Key::J) {
                self.move_by(1);
            } else if i.consume_key(M::NONE, Key::ArrowUp) || i.consume_key(M::CTRL, Key::P) || i.consume_key(M::CTRL, Key::K) {
                self.move_by(-1);
            } else if i.consume_key(M::NONE, Key::PageDown) || i.consume_key(M::CTRL, Key::F) {
                self.move_by(10);
            } else if i.consume_key(M::NONE, Key::PageUp) || i.consume_key(M::CTRL, Key::B) {
                self.move_by(-10);
            } else if i.consume_key(M::CTRL, Key::D) {
                ev = PickerEvent::Delete;
            } else if i.consume_key(M::NONE, Key::Tab) {
                ev = PickerEvent::Complete;
            }
        });
        ev
    }

    pub fn show(&mut self, ctx: &egui::Context, theme: &Theme, keymap_hint: &str) -> PickerEvent {
        let mut ev = self.handle_keys(ctx);
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.62).clamp(380.0, 760.0);
        let max_rows = ((screen.height() * 0.6) / 30.0).floor().clamp(4.0, 16.0) as usize;
        egui::Area::new(egui::Id::unique("picker"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, (screen.height() * 0.09).min(90.0)))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(c(theme.surface))
                    .stroke(Stroke::new(1.0, c(theme.border)))
                    .corner_radius(radius(theme))
                    .shadow(ui.visuals().window_shadow)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&self.title).color(c(theme.accent)).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{}/{}", self.filtered.len(), self.items.len()))
                                        .small()
                                        .color(c(theme.text_dim)),
                                );
                            });
                        });
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text(&self.placeholder)
                            .desired_width(f32::INFINITY)
                            .font(FontId::proportional(16.0))
                            .margin(egui::Margin::symmetric(8, 6))
                            .frame(egui::Frame::new().fill(c(theme.surface_alt)).corner_radius(radius(theme)))
                            .id(egui::Id::unique("picker_input"));
                        let resp = ui.add(edit);
                        resp.request_focus();
                        if self.move_cursor_end {
                            set_cursor_end(ui.ctx(), resp.id, &self.query);
                            self.move_cursor_end = false;
                        }
                        if resp.changed() {
                            self.refilter();
                            if matches!(ev, PickerEvent::None) {
                                ev = PickerEvent::QueryChanged;
                            }
                        }
                        ui.add_space(6.0);
                        let row_h = 28.0;
                        egui::ScrollArea::vertical().max_height(row_h * max_rows as f32).auto_shrink([false, true]).show(
                            ui,
                            |ui| {
                                for (row, &idx) in self.filtered.iter().enumerate() {
                                    let item = &self.items[idx];
                                    let selected = row == self.selected;
                                    let (rect, resp) =
                                        ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
                                    if selected {
                                        ui.painter().rect_filled(rect, radius(theme), with_alpha(theme.accent, 38));
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(
                                                rect.min + egui::vec2(0.0, 6.0),
                                                egui::vec2(3.0, row_h - 12.0),
                                            ),
                                            2,
                                            c(theme.accent),
                                        );
                                        if self.scroll_to_selected {
                                            resp.scroll_to_me(None);
                                        }
                                    } else if resp.hovered() {
                                        ui.painter().rect_filled(rect, radius(theme), with_alpha(theme.text, 10));
                                    }
                                    let x = rect.left() + 12.0 + item.depth as f32 * 16.0;
                                    let painter = ui.painter_at(rect);
                                    let label_color = if selected { c(theme.text) } else { with_alpha(theme.text, 220) };
                                    let font = FontId::proportional(if item.depth == 0 && self.kind == PickerKind::Toc {
                                        14.5
                                    } else {
                                        14.0
                                    });
                                    let hint_galley =
                                        painter.layout_no_wrap(item.hint.clone(), FontId::monospace(12.0), c(theme.text_dim));
                                    let hint_w = hint_galley.size().x;
                                    let label = painter.layout_no_wrap(item.label.clone(), font, label_color);
                                    let label_w = label.size().x;
                                    let label_clip = egui::Rect::from_min_max(
                                        rect.min,
                                        egui::pos2(rect.right() - hint_w - 18.0, rect.bottom()),
                                    );
                                    painter.with_clip_rect(label_clip).galley(
                                        egui::pos2(x, rect.center().y - label.size().y / 2.0),
                                        label,
                                        label_color,
                                    );
                                    if !item.detail.is_empty() {
                                        let dx = x + label_w + 10.0;
                                        let room = rect.right() - hint_w - 20.0 - dx;
                                        if room > 30.0 {
                                            let detail = painter.layout(
                                                item.detail.clone(),
                                                FontId::proportional(12.5),
                                                c(theme.text_dim),
                                                room,
                                            );
                                            let first_row_h = detail.rows.first().map(|r| r.height()).unwrap_or(14.0);
                                            painter
                                                .with_clip_rect(egui::Rect::from_min_max(
                                                    egui::pos2(dx, rect.top()),
                                                    egui::pos2(dx + room, rect.bottom()),
                                                ))
                                                .galley(
                                                    egui::pos2(dx, rect.center().y - first_row_h / 2.0),
                                                    detail,
                                                    c(theme.text_dim),
                                                );
                                        }
                                    }
                                    if !item.hint.is_empty() {
                                        painter.galley(
                                            egui::pos2(
                                                rect.right() - hint_w - 10.0,
                                                rect.center().y - hint_galley.size().y / 2.0,
                                            ),
                                            hint_galley,
                                            c(theme.text_dim),
                                        );
                                    }
                                    if resp.clicked() {
                                        self.selected = row;
                                        ev = PickerEvent::Accept;
                                    }
                                }
                                if self.filtered.is_empty() {
                                    ui.add_space(8.0);
                                    ui.label(RichText::new("  No matches").color(c(theme.text_dim)));
                                    ui.add_space(8.0);
                                }
                            },
                        );
                        self.scroll_to_selected = false;
                        ui.add_space(4.0);
                        ui.label(RichText::new(keymap_hint).small().color(c(theme.text_dim)));
                    });
            });
        ev
    }
}

/// Put the text cursor of a `TextEdit` at the end of its contents.
pub fn set_cursor_end(ctx: &egui::Context, id: egui::Id, text: &str) {
    if let Some(mut state) = egui::TextEdit::load_state(ctx, id) {
        let end = egui::text::CCursor::new(text.chars().count());
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette(names: &[&str], query: &str) -> Vec<String> {
        let items = names.iter().map(|n| PickItem::new(*n, PickData::Command(n.to_string()))).collect();
        let mut p = Picker::new(PickerKind::Commands, "Command", items);
        p.query = query.into();
        p.refilter();
        p.filtered.iter().map(|&i| p.items[i].label.clone()).collect()
    }

    #[test]
    fn exact_and_short_prefix_first() {
        let names = ["zoom_in", "zoom_out", "actual_size", "zoom", "toggle_zoom"];
        let got = palette(&names, "zoo");
        assert_eq!(&got[..3], ["zoom", "zoom_in", "zoom_out"]);
        assert_eq!(palette(&names, "zoom_in")[0], "zoom_in");
        assert_eq!(palette(&names, "zoom 150")[0], "zoom");
    }
}
