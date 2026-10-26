//! Editing highlights: pick one (hint labels on screen, a right click, or the `gh` list), then change its color,
//! edit its note, copy or delete it. The same actions are commands taking the highlight's number, for Lua and
//! `--remote`.

use crate::app::{App, PromptKind};
use crate::session::{HintAction, HintItem, HintMode};
use crate::store::Highlight;
use crate::ui::picker::{HighlightAction, PickData, PickItem, Picker, PickerKind};

impl App {
    /// Label the highlights on screen; picking one opens its menu. A single one opens right away.
    pub fn start_highlight_hints(&mut self) {
        let Some(d) = &mut self.doc else { return };
        let items: Vec<HintItem> = d
            .state
            .highlights
            .iter()
            .enumerate()
            .filter_map(|(i, h)| {
                let rect = h.rects.iter().map(crate::doc::Rect::from).reduce(|a, b| a.union(&b))?;
                d.view.is_visible(h.page, rect.y0, rect.y1).then(|| {
                    let first = h.rects.first().map(crate::doc::Rect::from).unwrap_or(rect);
                    HintItem { label: String::new(), page: h.page, rect: first, action: HintAction::Highlight(i) }
                })
            })
            .collect();
        if let [only] = items.as_slice() {
            let HintAction::Highlight(i) = only.action else { return };
            return self.open_highlight_menu(i);
        }
        self.show_hints(HintMode::Edit, items, "No highlights on screen");
    }

    /// The highlight under a point on a page, if any.
    pub fn highlight_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let d = self.doc.as_ref()?;
        d.state.highlights.iter().position(|h| {
            h.page == page
                && h.rects
                    .iter()
                    .map(crate::doc::Rect::from)
                    .any(|r| x >= r.x0 - 1.0 && x <= r.x1 + 1.0 && y >= r.y0 - 1.5 && y <= r.y1 + 1.5)
        })
    }

    pub fn open_highlight_menu(&mut self, i: usize) {
        let Some(h) = self.doc.as_ref().and_then(|d| d.state.highlights.get(i)) else { return };
        let excerpt: String = h.text.chars().take(60).collect();
        let mut items = vec![
            PickItem::new("Edit note", PickData::HighlightAction(i, HighlightAction::Note)).detail(if h.note.is_empty() {
                "add a note".to_string()
            } else {
                h.note.chars().take(60).collect()
            }),
            PickItem::new("Copy text", PickData::HighlightAction(i, HighlightAction::Copy)),
            PickItem::new("Delete", PickData::HighlightAction(i, HighlightAction::Delete)),
        ];
        for (name, _) in &self.theme.highlights {
            let current = *name == h.color;
            items.push(
                PickItem::new(format!("Color: {name}"), PickData::HighlightAction(i, HighlightAction::Color(name.clone())))
                    .hint(if current { "current" } else { "" }),
            );
        }
        let mut p = Picker::new(PickerKind::HighlightMenu, format!("Highlight  ·  “{excerpt}”"), items);
        p.keep_order = true;
        self.picker = Some(p);
    }

    pub fn highlight_action(&mut self, i: usize, action: HighlightAction) {
        match action {
            HighlightAction::Note => {
                let note = self.doc.as_ref().and_then(|d| d.state.highlights.get(i)).map(|h| h.note.clone()).unwrap_or_default();
                self.ask("note", PromptKind::HighlightNote(i), note);
            }
            HighlightAction::Copy => {
                if let Some(h) = self.doc.as_ref().and_then(|d| d.state.highlights.get(i)) {
                    let text = h.text.clone();
                    self.ctx.copy_text(text);
                    self.notify("Highlight copied");
                }
            }
            HighlightAction::Delete => self.edit_highlight(i, |_| None),
            HighlightAction::Color(c) => self.edit_highlight(i, |h| Some(Highlight { color: c, ..h })),
        }
    }

    /// Replace (or with `None`, delete) highlight `i` and save.
    pub fn edit_highlight(&mut self, i: usize, f: impl FnOnce(Highlight) -> Option<Highlight>) {
        let Some(d) = &mut self.doc else { return };
        if i >= d.state.highlights.len() {
            return self.warn(format!("No highlight {}", i + 1));
        }
        let h = d.state.highlights.remove(i);
        let deleted = match f(h) {
            Some(h) => {
                d.state.highlights.insert(i, h);
                false
            }
            None => true,
        };
        d.save_state();
        if deleted {
            self.notify("Highlight deleted");
        }
    }

    /// `highlight_color N COLOR`, `highlight_note N TEXT`, `delete_highlight N`: N counts from 1 in page order,
    /// as in the `gh` list.
    pub fn highlight_command(&mut self, name: &str, args: &str) {
        let (num, rest) = crate::commands::split(args);
        let index = match num.parse::<usize>() {
            Ok(n) if n >= 1 => n - 1,
            _ => {
                return self.warn(format!("{name}: give the highlight's number (1 is the first; `gh` lists them)"));
            }
        };
        match name {
            "delete_highlight" => self.edit_highlight(index, |_| None),
            "highlight_color" => {
                if !self.theme.highlights.iter().any(|(n, _)| n == rest) {
                    let have: Vec<&str> = self.theme.highlights.iter().map(|(n, _)| n.as_str()).collect();
                    return self.warn(format!("unknown highlight color '{rest}' (have: {})", have.join(", ")));
                }
                let color = rest.to_string();
                self.edit_highlight(index, |h| Some(Highlight { color, ..h }));
            }
            "highlight_note" => {
                let note = rest.to_string();
                self.edit_highlight(index, |h| Some(Highlight { note, ..h }));
            }
            _ => {}
        }
    }
}
