//! Per-frame driver: input routing, workers, panels, page canvas and overlays.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Event, FontId, Key, Modifiers, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, TextureOptions, Vec2,
};

use super::picker::set_cursor_end;
use super::style::{c, radius, with_alpha};
use crate::app::{App, Mode, PromptKind, notify_into, reveal};
use crate::config::PageMode;
use crate::doc::{self, DocPos};
use crate::keymap::Chord;
use crate::render::{TILE, TileKey, tile_rect};
use crate::script::Level;
use crate::session::{HintAction, HintMode, Session, TexEntry};
use crate::text::Response as TextResponse;

const TEXTURE_BUDGET: usize = 320 << 20;

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_all();
            self.emit_quit();
        }
        self.poll_workers(&ctx);
        self.poll_jobs();
        let remote: Vec<String> = self.ipc.as_ref().map(|s| s.rx.try_iter().collect()).unwrap_or_default();
        for line in remote {
            // `;` separates commands, except inside Lua code.
            if crate::commands::split(&line).0 == "lua" {
                self.run_line(&line, None);
            } else {
                for cmd in line.split(';') {
                    self.run_line(cmd, None);
                }
            }
        }
        // A file dropped onto the window opens like `o` would.
        if let Some(path) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
            self.open(&path);
        }
        self.handle_keys(&ctx);
        self.tick_keys();

        if self.show_statusbar {
            egui::Panel::bottom("status")
                .exact_size(28.0)
                .frame(egui::Frame::new().fill(c(self.theme.surface)).inner_margin(egui::Margin::symmetric(10, 0)))
                .show_separator_line(false)
                .show(ui, |ui| self.status_bar(ui));
        }
        if self.prompt.is_some() {
            egui::Panel::bottom("prompt")
                .exact_size(40.0)
                .frame(
                    egui::Frame::new()
                        .fill(c(self.theme.surface))
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .stroke(Stroke::new(1.0, c(self.theme.border))),
                )
                .show_separator_line(false)
                .show(ui, |ui| self.prompt_bar(ui));
        }
        if self.show_sidebar && self.doc.is_some() {
            egui::Panel::left("outline")
                .default_size(290.0)
                .resizable(true)
                .frame(egui::Frame::new().fill(c(self.theme.surface)).inner_margin(egui::Margin::symmetric(8, 10)))
                .show(ui, |ui| self.sidebar(ui));
        }
        if self.text_panel.visible {
            egui::Panel::right("text_panel")
                .default_size(400.0)
                .resizable(true)
                .frame(egui::Frame::new().fill(c(self.theme.surface)).inner_margin(egui::Margin::same(14)))
                .show(ui, |ui| self.text_panel_ui(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui, dt));

        if self.doc.as_ref().is_some_and(|d| d.peek.is_some()) {
            self.peek_window(&ctx);
        }
        if let Some(p) = &mut self.picker {
            let hint = "↑↓ / Ctrl-N Ctrl-P move   ·   Enter select   ·   Tab complete   ·   Esc close";
            let ev = p.show(&ctx, &self.theme, hint);
            self.picker_event(ev);
        }
        self.toasts_ui(&ctx);
        self.request_tiles(&ctx);

        if let Some(d) = &mut self.doc {
            d.evict_textures(TEXTURE_BUDGET);
            if d.dirty && d.last_save.elapsed() > Duration::from_secs(8) {
                d.save_state();
            }
        }
        // Keep polling the file for changes.
        if self.cfg.general.auto_reload && self.doc.is_some() {
            ctx.request_repaint_after(Duration::from_millis(900));
        }
    }

    /// Every exit path, including Cmd-Q and Apple Event quits that never raise `close_requested`.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_all();
        self.emit_quit();
    }
}

/// Translate egui events into key chords.
fn chords(events: &[Event]) -> Vec<Chord> {
    let mut out = Vec::new();
    let mut suppress_text = false;
    for ev in events {
        match ev {
            Event::Key { key, pressed: true, modifiers, .. } => {
                let named = match key {
                    Key::Escape => Some("Esc"),
                    Key::Enter => Some("Enter"),
                    Key::Tab => Some("Tab"),
                    Key::Backspace => Some("Backspace"),
                    Key::Delete => Some("Delete"),
                    Key::Insert => Some("Insert"),
                    Key::Home => Some("Home"),
                    Key::End => Some("End"),
                    Key::PageUp => Some("PageUp"),
                    Key::PageDown => Some("PageDown"),
                    Key::ArrowUp => Some("Up"),
                    Key::ArrowDown => Some("Down"),
                    Key::ArrowLeft => Some("Left"),
                    Key::ArrowRight => Some("Right"),
                    _ => None,
                };
                let fkey = key.name().strip_prefix('F').filter(|n| n.parse::<u8>().is_ok()).map(|_| key.name());
                if let Some(name) = named.or(fkey) {
                    out.push(Chord { key: name.to_string(), ctrl: modifiers.ctrl, alt: modifiers.alt, shift: modifiers.shift });
                } else if modifiers.ctrl || modifiers.alt || modifiers.command {
                    let k = match key {
                        Key::Minus => "-".to_string(),
                        Key::Plus => "+".to_string(),
                        Key::Equals => "=".to_string(),
                        Key::Space => "Space".to_string(),
                        k => {
                            let s = k.symbol_or_name();
                            if s.chars().count() == 1 {
                                if modifiers.shift { s.to_uppercase() } else { s.to_lowercase() }
                            } else {
                                s.to_string()
                            }
                        }
                    };
                    out.push(Chord { key: k, ctrl: modifiers.ctrl || modifiers.command, alt: modifiers.alt, shift: false });
                    suppress_text = true;
                } else if *key == Key::Space && modifiers.shift {
                    out.push(Chord { key: "Space".into(), ctrl: false, alt: false, shift: true });
                    suppress_text = true;
                }
            }
            // egui-winit turns Ctrl/Cmd + X/C/V into clipboard events instead of key presses.
            Event::Cut | Event::Copy | Event::Paste(_) => {
                let key = match ev {
                    Event::Cut => "x",
                    Event::Copy => "c",
                    _ => "v",
                };
                out.push(Chord { key: key.into(), ctrl: true, alt: false, shift: false });
            }
            Event::Text(t) => {
                if suppress_text {
                    suppress_text = false;
                    continue;
                }
                out.extend(t.chars().map(Chord::char));
            }
            _ => {}
        }
    }
    out
}

fn strip_key_events(ctx: &egui::Context) {
    ctx.input_mut(|i| i.events.retain(|e| !matches!(e, Event::Text(_) | Event::Key { .. })));
}

impl App {
    fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.picker.is_some() || self.prompt.is_some() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let keys = chords(&events);
        if keys.is_empty() {
            return;
        }
        for chord in keys {
            // Overlays get first pick.
            if self.doc.as_ref().is_some_and(|d| d.hints.is_some()) {
                self.hint_key(&chord);
                continue;
            }
            if self.doc.as_ref().is_some_and(|d| d.peek.is_some()) && self.peek_key(&chord) {
                continue;
            }
            if self.picker.is_some() || self.prompt.is_some() {
                break;
            }
            self.feed_key(chord);
        }
        // Don't let the keys that opened a text field also type into it.
        strip_key_events(ctx);
    }

    fn hint_key(&mut self, chord: &Chord) {
        let Some(d) = &mut self.doc else { return };
        let selecting = d.hints.as_ref().is_some_and(|h| h.mode == HintMode::Select);
        if selecting && chord.as_char() == Some('v') && !self.cfg.general.hint_chars.contains('v') {
            return self.visual_without_hints();
        }
        match (chord.key.as_str(), chord.as_char()) {
            ("Esc", _) => d.hints = None,
            ("Backspace", _) => {
                if let Some(h) = &mut d.hints {
                    h.typed.pop();
                }
            }
            (_, Some(ch)) if ch.is_alphanumeric() || self.cfg.general.hint_chars.contains(ch.to_ascii_lowercase()) => {
                self.hint_input(ch)
            }
            _ => d.hints = None,
        }
    }

    /// Keys while the peek window is open. Returns true if consumed.
    fn peek_key(&mut self, chord: &Chord) -> bool {
        let Some(d) = &mut self.doc else { return false };
        let Some(p) = &mut d.peek else { return false };
        let ph = d.view.page_size(p.page).1;
        match (chord.key.as_str(), chord.ctrl) {
            ("Esc", _) | ("q", false) => d.peek = None,
            ("j", false) | ("Down", _) => p.y = (p.y + 40.0).min(ph - 40.0),
            ("k", false) | ("Up", _) => p.y = (p.y - 40.0).max(0.0),
            ("d", true) => p.y = (p.y + 200.0).min(ph - 40.0),
            ("u", true) => p.y = (p.y - 200.0).max(0.0),
            ("J", false) if p.page + 1 < d.view.page_count() => {
                p.page += 1;
                p.y = 0.0;
                p.target = None;
            }
            ("K", false) if p.page > 0 => {
                p.page -= 1;
                p.y = 0.0;
                p.target = None;
            }
            ("+", false) | ("=", false) => p.zoom = (p.zoom * 1.15).min(8.0),
            ("-", false) => p.zoom = (p.zoom / 1.15).max(0.3),
            ("Enter", _) | ("o", false) => {
                let dest = DocPos { page: p.page, y: if p.page == p.dest.page { p.dest.y } else { p.y } };
                d.peek = None;
                d.jump(dest, 0.3, false);
            }
            _ => return false,
        }
        true
    }

    // ------------------------------------------------------------------

    fn poll_workers(&mut self, ctx: &egui::Context) {
        let mut ref_results = Vec::new();
        let mut reload_err = None;
        let mut page_changed = None;
        {
            let Some(d) = &mut self.doc else { return };
            let mut uploaded = 0;
            while let Ok(tile) = d.pool.results.try_recv() {
                if tile.key.generation != d.generation {
                    continue;
                }
                let image = egui::ColorImage::from_rgba_premultiplied([tile.width as usize, tile.height as usize], &tile.pixels);
                let k = tile.key;
                let handle =
                    ctx.load_texture(format!("p{}s{}t{}x{}", k.page, k.scale_milli, k.tx, k.ty), image, TextureOptions::LINEAR);
                let bytes = tile.pixels.len();
                if let Some(old) = d.textures.insert(tile.key, TexEntry { handle, last_used: Instant::now(), bytes }) {
                    d.tex_bytes -= old.bytes;
                }
                d.tex_bytes += bytes;
                uploaded += 1;
                if uploaded > 24 {
                    ctx.request_repaint();
                    break;
                }
            }
            while let Ok(resp) = d.text.rx.try_recv() {
                match resp {
                    TextResponse::SearchHits { generation, hits } => {
                        let Some(s) = &mut d.search else { continue };
                        if s.generation != generation {
                            continue;
                        }
                        let base = s.hits.len();
                        s.hits.extend(hits);
                        if s.jump_on_first && s.current.is_none() {
                            let o = s.origin;
                            let pick = if s.backward {
                                None // wait for all results so we can pick the last one before the origin
                            } else {
                                s.hits[base..]
                                    .iter()
                                    .position(|h| (h.page, h.rects.first().map(|r| r.y0).unwrap_or(0.0)) >= (o.page, o.y))
                                    .map(|i| i + base)
                            };
                            if let Some(i) = pick {
                                s.current = Some(i);
                                let h = s.hits[i].clone();
                                reveal(d, h.page, h.rects.first().copied(), false);
                            }
                        }
                    }
                    TextResponse::SearchDone { generation, .. } => {
                        let Some(s) = &mut d.search else { continue };
                        if s.generation != generation {
                            continue;
                        }
                        s.done = true;
                        if s.jump_on_first && s.current.is_none() && !s.hits.is_empty() {
                            let o = s.origin;
                            let i = if s.backward {
                                s.hits
                                    .iter()
                                    .rposition(|h| (h.page, h.rects.first().map(|r| r.y0).unwrap_or(0.0)) < (o.page, o.y))
                                    .unwrap_or(s.hits.len() - 1)
                            } else {
                                0 // wrapped around
                            };
                            s.current = Some(i);
                            let h = s.hits[i].clone();
                            reveal(d, h.page, h.rects.first().copied(), false);
                        }
                    }
                    TextResponse::Ref { id, target } => ref_results.push((id, target)),
                }
            }
            if self.cfg.general.auto_reload && d.file_changed() {
                match d.reload() {
                    Ok(()) => {}
                    Err(e) => reload_err = Some(format!("reload: {e:#}")),
                }
            }
            // Wait for the first frame to restore the saved position before reporting pages.
            let page = d.view.current_page();
            if !d.first_frame && page != d.last_page {
                d.last_page = page;
                page_changed = Some(page);
            }
        }
        for (id, target) in ref_results {
            self.on_ref_result(id, target);
        }
        if let Some(e) = reload_err {
            // Usually a half-written file during a LaTeX build; it'll be retried.
            if !self.toasts.iter().any(|t| t.text.starts_with("reload")) {
                notify_into(&mut self.toasts, e, Level::Warn);
            }
        }
        if let Some(p) = page_changed {
            self.emit("page", (p + 1).to_string());
        }
    }

    fn request_tiles(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let oversample = self.cfg.general.render_oversample.clamp(0.5, 3.0);
        let Some(d) = &mut self.doc else { return };
        let mode = d.page_mode;
        let generation = d.generation;
        let scale = d.view.zoom * ppp * oversample;
        let vis = d.view.visible_pages();
        if vis.is_empty() {
            return;
        }
        let n = d.view.page_count();
        let vh = d.view.viewport.1;
        let mut wanted = Vec::new();
        for p in vis.clone() {
            wanted.push(thumb_key(d, p, mode, generation));
        }
        for p in vis.clone() {
            page_tiles(d, p, scale, (-vh * 0.25, vh * 1.25), mode, generation, &mut wanted);
        }
        let before = vis.start.checked_sub(1);
        let after = (vis.end < n).then_some(vis.end);
        for p in before.into_iter().chain(after) {
            wanted.push(thumb_key(d, p, mode, generation));
            page_tiles(d, p, scale, (-vh, vh * 2.0), mode, generation, &mut wanted);
        }
        if let Some(pk) = &d.peek {
            let (page, zoom, y) = (pk.page, pk.zoom, pk.y);
            wanted.insert(0, thumb_key(d, page, mode, generation));
            let s = zoom * ppp * oversample;
            let sm = (s * 1000.0).round() as u32;
            let (w, h) = d.view.page_size(page);
            let (pw, ph) = ((w * s).ceil() as u32, (h * s).ceil() as u32);
            let y0 = (y * s) as u32;
            let y1 = ((y + vh * 0.5 / zoom) * s) as u32;
            for ty in y0 / TILE..=(y1.min(ph.saturating_sub(1)) / TILE) {
                for tx in 0..pw.div_ceil(TILE) {
                    wanted.insert(1, TileKey { page, scale_milli: sm, tx, ty, mode, generation });
                }
            }
        }
        let now = Instant::now();
        wanted.retain(|k| match d.textures.get_mut(k) {
            Some(t) => {
                t.last_used = now;
                false
            }
            None => true,
        });
        wanted.dedup();
        d.pool.request(wanted);
    }

    // ------------------------------------------------------------------
    // Canvas

    fn canvas(&mut self, ui: &mut egui::Ui, dt: f32) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, c(self.theme.bg));
        let ppp = ui.ctx().pixels_per_point();
        let theme = self.theme.clone();
        let Some(d) = &mut self.doc else {
            self.welcome(ui, rect);
            return;
        };
        d.view.set_viewport(rect.width(), rect.height());
        if d.first_frame {
            d.first_frame = false;
            if self.cfg.general.restore_position
                && let Some(pos) = d.state.pos
            {
                d.view.goto(pos, 0.0, false);
            }
        }

        // Mouse wheel / pinch.
        if resp.hovered() || resp.dragged() {
            let (scroll, zoom, hover) = ui.input(|i| (i.smooth_scroll_delta(), i.zoom_delta(), i.pointer.hover_pos()));
            if zoom != 1.0 {
                let frac = hover.map(|p| (p.y - rect.top()) / rect.height()).unwrap_or(0.5);
                d.view.fit = crate::view::Fit::None;
                let z = d.view.zoom * zoom;
                d.view.set_zoom(z, frac);
                d.dirty = true;
            } else if scroll != Vec2::ZERO {
                d.view.scroll_by(-scroll.y, false);
                d.view.scroll_x += scroll.x;
                d.view.clamp();
                d.dirty = true;
            }
        }
        if d.view.animate(dt) {
            ui.ctx().request_repaint();
        }

        let origin = rect.min;
        let snap = |p: Pos2| Pos2::new((p.x * ppp).round() / ppp, (p.y * ppp).round() / ppp);
        let page_bg = match d.page_mode {
            PageMode::Normal => Color32::WHITE,
            _ => c(theme.page_bg),
        };
        let current_scale_milli =
            ((d.view.zoom * ppp * self.cfg.general.render_oversample.clamp(0.5, 3.0)) * 1000.0).round() as u32;
        let zoom = d.view.zoom;
        let vis = d.view.visible_pages();
        let mut hover_link = false;
        let pointer = ui.input(|i| i.pointer.hover_pos());

        for p in vis.clone() {
            let (x, y, w, h) = d.view.page_rect(p);
            let pr = Rect::from_min_size(snap(origin + Vec2::new(x, y)), Vec2::new(w, h));
            let shadow = egui::Shadow { offset: [0, 3], blur: 18, spread: 0, color: c(theme.page_shadow) };
            painter.add(shadow.as_shape(pr, CornerRadius::same(2)));
            painter.rect_filled(pr, 1.0, page_bg);

            // Tiles: lower resolutions first as placeholders, the current scale on top.
            let mut keys: Vec<TileKey> = d
                .textures
                .keys()
                .filter(|k| k.page == p && k.mode == d.page_mode && k.generation == d.generation)
                .copied()
                .collect();
            keys.sort_by_key(|k| (k.scale_milli == current_scale_milli, k.scale_milli));
            for k in keys {
                let s = k.scale();
                let tex = &d.textures[&k];
                let size = tex.handle.size();
                let min = pr.min + Vec2::new(k.tx as f32 * TILE as f32 / s * zoom, k.ty as f32 * TILE as f32 / s * zoom);
                let tr = Rect::from_min_size(min, Vec2::new(size[0] as f32 / s * zoom, size[1] as f32 / s * zoom));
                painter.image(tex.handle.id(), tr, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }

            let to_screen =
                |r: &doc::Rect| Rect::from_min_max(pr.min + Vec2::new(r.x0, r.y0) * zoom, pr.min + Vec2::new(r.x1, r.y1) * zoom);

            // Highlights.
            for hl in d.state.highlights.iter().filter(|h| h.page == p) {
                let col = c(theme.highlight_color(&hl.color));
                for r in &hl.rects {
                    let sr = to_screen(&doc::Rect::from(r)).expand2(Vec2::new(1.0, 1.5));
                    painter.rect_filled(sr, 2.0, col);
                }
                if !hl.note.is_empty()
                    && let Some(first) = hl.rects.first()
                {
                    let sr = to_screen(&doc::Rect::from(first));
                    painter.circle_filled(Pos2::new(sr.left() - 6.0, sr.center().y), 3.0, c(theme.accent));
                }
            }

            // Search matches.
            if let Some(s) = &d.search {
                for (i, hit) in s.hits.iter().enumerate().filter(|(_, h)| h.page == p) {
                    let current = s.current == Some(i);
                    let col = if current { c(theme.search_current) } else { c(theme.search_match) };
                    for r in &hit.rects {
                        let sr = to_screen(r).expand2(Vec2::new(1.5, 1.5));
                        painter.rect_filled(sr, 3.0, col);
                        if current {
                            painter.rect_stroke(
                                sr,
                                3.0,
                                Stroke::new(1.5, with_alpha(theme.search_current, 255)),
                                StrokeKind::Outside,
                            );
                        }
                    }
                }
            }

            // Selection.
            if let Some(sel) = d.selection.filter(|s| s.page == p) {
                let t = d.page_texts.get(&p).cloned().unwrap_or_else(|| d.page_text(p));
                let (s, e) = sel.range();
                for r in t.span_rects(s, e) {
                    painter.rect_filled(to_screen(&r).expand2(Vec2::new(1.0, 1.5)), 2.0, c(theme.selection));
                }
                if let Some(cr) = t.rects.get(sel.cursor) {
                    let sr = to_screen(cr);
                    painter.line_segment([sr.right_top(), sr.right_bottom()], Stroke::new(2.0, c(theme.accent)));
                }
            }

            // Reading ruler.
            if let Some(r) = d.ruler.filter(|r| r.page == p) {
                let t = d.page_text(p);
                if let Some(&(s, e)) = t.lines().get(r.line)
                    && let Some(lr) = t.span_rects(s, e).into_iter().reduce(|a, b| a.union(&b))
                {
                    let sr = to_screen(&lr);
                    let band = Rect::from_min_max(Pos2::new(pr.left(), sr.top() - 3.0), Pos2::new(pr.right(), sr.bottom() + 3.0));
                    painter.rect_filled(band, 0.0, c(theme.ruler));
                    painter.line_segment(
                        [Pos2::new(pr.left(), band.bottom()), Pos2::new(pr.right(), band.bottom())],
                        Stroke::new(1.5, with_alpha(theme.accent, 160)),
                    );
                    // Dim what's already been read.
                    let dim = Rect::from_min_max(pr.min, Pos2::new(pr.right(), band.top()));
                    painter.rect_filled(dim, 0.0, Color32::from_black_alpha(if theme.dark { 40 } else { 18 }));
                }
            }

            // Hints.
            if let Some(hints) = &d.hints {
                // Choosing a word: dim the page except the chosen row, so labels above it can cover the line before.
                let row = hints
                    .items
                    .iter()
                    .filter(|i| i.page == p && matches!(i.action, HintAction::Word(..)))
                    .map(|i| to_screen(&i.rect))
                    .reduce(|a, b| a.union(b));
                if let Some(row) = row {
                    let row = row.expand2(Vec2::new(0.0, 3.0));
                    let dim = Color32::from_black_alpha(if theme.dark { 90 } else { 60 });
                    painter.rect_filled(Rect::from_min_max(pr.min, Pos2::new(pr.right(), row.top())), 0.0, dim);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(pr.left(), row.bottom()), pr.max), 0.0, dim);
                }
                let mut placed: Vec<Rect> = Vec::new();
                for item in hints.items.iter().filter(|i| i.page == p && i.label.starts_with(&hints.typed)) {
                    let sr = to_screen(&item.rect);
                    let word = matches!(item.action, HintAction::Word(..));
                    if word {
                        // Underline the candidate word; the badge sits just above it, so the line stays readable.
                        let y = sr.bottom() + 1.0;
                        painter.line_segment(
                            [Pos2::new(sr.left(), y), Pos2::new(sr.right(), y)],
                            Stroke::new(1.5, with_alpha(theme.hint_bg, 220)),
                        );
                    } else if !matches!(item.action, HintAction::Line(_)) {
                        painter.rect_stroke(
                            sr.expand(1.0),
                            2.0,
                            Stroke::new(1.0, with_alpha(theme.hint_bg, 200)),
                            StrokeKind::Outside,
                        );
                    }
                    let font = FontId::monospace(if word { 11.0 } else { 12.5 });
                    let rest = &item.label[hints.typed.len()..];
                    let galley = painter.layout_no_wrap(rest.to_uppercase(), font, c(theme.hint_text));
                    let size = galley.size() + Vec2::new(8.0, 2.0);
                    let at = if word {
                        // Centered above the word; lifted a level when it would overlap a neighbor's label.
                        let base = Pos2::new(sr.center().x - size.x * 0.5, sr.top() - size.y - 1.0);
                        let at = (0..4)
                            .map(|level| base - Vec2::new(0.0, level as f32 * (size.y + 1.0)))
                            .find(|at| !placed.iter().any(|r| r.intersects(Rect::from_min_size(*at, size))))
                            .unwrap_or(base);
                        placed.push(Rect::from_min_size(at, size).expand(1.0));
                        at
                    } else {
                        // Left of the target (in the margin for lines), vertically centered.
                        let x = (sr.left() - size.x - 3.0).max(pr.left() + 2.0).min(sr.left());
                        Pos2::new(x, sr.center().y - size.y * 0.5)
                    };
                    let badge = Rect::from_min_size(at, size);
                    painter.rect_filled(badge.translate(Vec2::new(0.0, 1.5)), 4.0, Color32::from_black_alpha(80));
                    painter.rect_filled(badge, 4.0, c(theme.hint_bg));
                    painter.galley(at + Vec2::new(4.0, 1.0), galley, c(theme.hint_text));
                }
            }

            // Link hover cursor.
            if let Some(ptr) = pointer.filter(|ptr| pr.contains(*ptr)) {
                let local = (ptr - pr.min) / zoom;
                if d.page_links(p)
                    .iter()
                    .any(|l| local.x >= l.rect.x0 && local.x <= l.rect.x1 && local.y >= l.rect.y0 && local.y <= l.rect.y1)
                {
                    hover_link = true;
                }
            }
        }

        if hover_link && !resp.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        } else if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }
        self.canvas_mouse(&resp, rect);

        if let Some(d) = &self.doc
            && d.hints.is_some()
        {
            let label = match d.hints.as_ref().unwrap().mode {
                HintMode::Jump => "type a label to jump  ·  SHIFT+label to peek  ·  Esc cancels",
                HintMode::Peek => "type a label to peek  ·  Esc cancels",
                HintMode::Select if d.hints.as_ref().unwrap().items.iter().any(|i| matches!(i.action, HintAction::Word(..))) => {
                    "type a word's label to start selecting there  ·  Esc cancels"
                }
                HintMode::Select => "pick a line, then a word  ·  v starts at the top  ·  Esc cancels",
            };
            banner(&painter, rect, &theme, label);
        }
    }

    fn canvas_mouse(&mut self, resp: &egui::Response, rect: Rect) {
        let Some(d) = &mut self.doc else { return };
        let to_doc = |d: &Session, p: Pos2| d.view.screen_to_doc(p.x - rect.left(), p.y - rect.top());

        if resp.dragged_by(egui::PointerButton::Middle) {
            let delta = resp.drag_delta();
            d.view.scroll_by(-delta.y, false);
            d.view.scroll_x += delta.x;
            d.view.clamp();
            return;
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let origin = resp.ctx.input(|i| i.pointer.press_origin());
            if let Some((page, x, y)) = origin.and_then(|p| to_doc(d, p)) {
                let t = d.page_text(page);
                self.drag_anchor = t.char_at(x, y).map(|ci| (page, ci));
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let (Some((page, anchor)), Some(pos)) = (self.drag_anchor, resp.interact_pointer_pos())
        {
            // Auto-scroll when dragging past the edges.
            if pos.y > rect.bottom() - 20.0 {
                d.view.scroll_by(12.0, false);
            } else if pos.y < rect.top() + 20.0 {
                d.view.scroll_by(-12.0, false);
            }
            if let Some((p2, x, y)) = to_doc(d, pos)
                && p2 == page
            {
                let t = d.page_text(page);
                if let Some(ci) = t.char_at(x, y) {
                    d.selection = Some(crate::session::Selection { page, anchor, cursor: ci });
                    self.mode = Mode::Visual;
                }
            }
        }
        if resp.drag_stopped() {
            self.drag_anchor = None;
            if let Some(t) = self.doc.as_mut().and_then(|d| d.selection_text()) {
                self.emit("selection", t);
            }
            return;
        }
        if resp.double_clicked() {
            if let Some((page, x, y)) = resp.interact_pointer_pos().and_then(|p| to_doc(d, p)) {
                let t = d.page_text(page);
                if let Some(ci) = t.char_at(x, y)
                    && let Some(&(s, e)) = t.words().iter().find(|&&(s, e)| ci >= s && ci < e)
                {
                    d.selection = Some(crate::session::Selection { page, anchor: s, cursor: e - 1 });
                    self.mode = Mode::Visual;
                }
            }
        } else if resp.clicked() {
            let Some((page, x, y)) = resp.interact_pointer_pos().and_then(|p| to_doc(d, p)) else { return };
            let link =
                d.page_links(page).iter().find(|l| x >= l.rect.x0 && x <= l.rect.x1 && y >= l.rect.y0 && y <= l.rect.y1).cloned();
            d.selection = None;
            self.mode = Mode::Normal;
            if let Some(link) = link {
                match link.target {
                    doc::LinkTarget::Page(pos) => d.jump(pos, 0.1, false),
                    doc::LinkTarget::Uri(uri) => self.open_link(&uri),
                    doc::LinkTarget::Other(s) => self.notify(format!("Unsupported link: {s}")),
                }
            }
        } else if resp.secondary_clicked() {
            // Right click previews internal links.
            let Some((page, x, y)) = resp.interact_pointer_pos().and_then(|p| to_doc(d, p)) else { return };
            let link =
                d.page_links(page).iter().find(|l| x >= l.rect.x0 && x <= l.rect.x1 && y >= l.rect.y0 && y <= l.rect.y1).cloned();
            if let Some(doc::LinkTarget::Page(pos)) = link.map(|l| l.target) {
                crate::app::open_peek(d, pos, None, format!("Link → page {}", pos.page + 1));
            }
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        let t = &self.theme;
        let center = rect.center() - Vec2::new(0.0, 60.0);
        painter.text(center, Align2::CENTER_BOTTOM, crate::config::APP_NAME, FontId::proportional(56.0), c(t.text));
        painter.text(
            center + Vec2::new(0.0, 14.0),
            Align2::CENTER_TOP,
            "a keyboard-first reader for technical documents",
            FontId::proportional(15.0),
            c(t.text_dim),
        );
        let rows = [("o", "open a file"), ("O", "recent documents"), (":", "command palette"), ("F1", "all key bindings")];
        for (i, (k, d)) in rows.iter().enumerate() {
            let y = center.y + 70.0 + i as f32 * 28.0;
            let kr = Rect::from_center_size(Pos2::new(center.x - 70.0, y), Vec2::new(40.0, 22.0));
            painter.rect_filled(kr, 5.0, c(t.surface_alt));
            painter.rect_stroke(kr, 5.0, Stroke::new(1.0, c(t.border)), StrokeKind::Inside);
            painter.text(kr.center(), Align2::CENTER_CENTER, *k, FontId::monospace(13.0), c(t.accent));
            painter.text(Pos2::new(center.x - 36.0, y), Align2::LEFT_CENTER, *d, FontId::proportional(14.0), c(t.text_dim));
        }
    }

    // ------------------------------------------------------------------
    // Chrome

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let pending = self.disp.pending_display();
        ui.horizontal_centered(|ui| {
            let (mode, col) = match (&self.doc, self.mode) {
                (Some(d), _) if d.hints.is_some() => ("HINT", t.hint_bg),
                (Some(d), _) if d.peek.is_some() => ("PEEK", t.accent),
                (_, _) if self.prompt.is_some() => ("INPUT", t.accent),
                (_, Mode::Visual) => ("VISUAL", t.search_current),
                (Some(d), _) if d.ruler.is_some() => ("RULER", t.accent),
                _ => ("NORMAL", t.accent),
            };
            let col = egui::Color32::from_rgb(col.0[0], col.0[1], col.0[2]);
            egui::Frame::new().fill(col).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(7, 1)).show(
                ui,
                |ui| {
                    ui.label(RichText::new(mode).size(11.5).strong().color(c(t.accent_text)));
                },
            );
            let Some(d) = &self.doc else {
                ui.label(RichText::new("no document").color(c(t.text_dim)));
                return;
            };
            ui.add_space(4.0);
            let title: String = d.title.chars().take(70).collect();
            ui.label(RichText::new(title).color(c(t.text)));
            if let Some(sec) = d.current_section() {
                let s: String = d.outline[sec].title.chars().take(50).collect();
                ui.label(RichText::new(format!("›  {s}")).color(c(t.text_dim)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let page = d.view.current_page();
                let n = d.view.page_count();
                let label = d.backend.page_label(page).filter(|l| !l.trim().is_empty() && *l != (page + 1).to_string());
                let pct = if n > 1 { page * 100 / (n - 1) } else { 100 };
                ui.label(RichText::new(format!("{pct}%")).color(c(t.text_dim)).monospace());
                let page_text = match label {
                    Some(l) => format!("{} / {}  ({l})", page + 1, n),
                    None => format!("{} / {}", page + 1, n),
                };
                ui.label(RichText::new(page_text).color(c(t.text)).monospace());
                ui.label(RichText::new(format!("{:.0}%", d.view.zoom * 100.0)).color(c(t.text_dim)).monospace());
                match d.page_mode {
                    PageMode::Dark => {
                        ui.label(RichText::new("◐ dark").color(c(t.text_dim)));
                    }
                    PageMode::Custom => {
                        ui.label(RichText::new("◑ tinted").color(c(t.text_dim)));
                    }
                    PageMode::Normal => {}
                }
                if let Some(s) = &d.search {
                    let cur = s.current.map(|c| (c + 1).to_string()).unwrap_or_else(|| "–".into());
                    let busy = if s.done { "" } else { " …" };
                    let msg = if s.done && s.hits.is_empty() {
                        format!("“{}” not found", s.query)
                    } else {
                        format!("“{}”  {cur}/{}{busy}", s.query, s.hits.len())
                    };
                    let col = if s.done && s.hits.is_empty() { t.search_current } else { t.text };
                    ui.label(RichText::new(msg).color(c(col)));
                }
                if let Some((_, _, label)) = &d.pending_ref {
                    ui.label(RichText::new(format!("finding {label}…")).color(c(t.text_dim)));
                }
                if !pending.is_empty() {
                    ui.label(RichText::new(pending).color(c(t.accent)).monospace().strong());
                }
            });
        });
    }

    fn prompt_bar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let mut accept = false;
        let mut cancel = false;
        let mut history = None;
        ui.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::Escape) || i.consume_key(Modifiers::CTRL, Key::OpenBracket) {
                cancel = true;
            } else if i.consume_key(Modifiers::NONE, Key::Enter) {
                accept = true;
            } else if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                history = Some(true);
            } else if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                history = Some(false);
            }
        });
        if cancel {
            return self.prompt_cancel();
        }
        if accept {
            return self.prompt_accept();
        }
        if let Some(up) = history {
            self.prompt_history(up);
        }
        let Some(p) = &mut self.prompt else { return };
        let mut changed = false;
        ui.horizontal_centered(|ui| {
            let label = match &p.kind {
                PromptKind::Search { .. } => p.label.clone(),
                _ => format!("{}:", p.label),
            };
            ui.label(RichText::new(label).color(c(t.accent)).strong().size(15.0));
            let id = egui::Id::unique("prompt_input");
            let edit = egui::TextEdit::singleline(&mut p.text)
                .id(id)
                .desired_width(f32::INFINITY)
                .font(FontId::proportional(15.0))
                .frame(egui::Frame::new())
                .margin(egui::Margin::symmetric(4, 2));
            let resp = ui.add(edit);
            resp.request_focus();
            if history.is_some() {
                set_cursor_end(ui.ctx(), id, &p.text);
            }
            changed = resp.changed();
        });
        if changed {
            self.prompt_changed();
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        let Some(d) = &mut self.doc else { return };
        ui.label(RichText::new("CONTENTS").size(11.0).strong().color(c(t.text_dim)));
        ui.add_space(6.0);
        if d.outline.is_empty() {
            ui.label(RichText::new("No outline").color(c(t.text_dim)));
            return;
        }
        let current = d.current_section();
        let sid = egui::Id::unique("sidebar_section");
        let section_changed = ui.ctx().data_mut(|m| {
            let prev = m.get_temp::<Option<usize>>(sid).flatten();
            m.insert_temp(sid, current);
            prev != current
        });
        let mut goto = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, item) in d.outline.iter().enumerate() {
                let selected = current == Some(i);
                let text = RichText::new(&item.title).size(if item.depth == 0 { 13.5 } else { 12.5 }).color(if selected {
                    c(t.accent)
                } else if item.depth == 0 {
                    c(t.text)
                } else {
                    c(t.text_dim)
                });
                ui.horizontal(|ui| {
                    ui.add_space(item.depth as f32 * 12.0);
                    let r = ui.add(egui::Label::new(text).truncate().sense(Sense::click()));
                    if selected && section_changed {
                        r.scroll_to_me(Some(egui::Align::Center));
                    }
                    if r.clicked() {
                        goto = item.target;
                    }
                    if r.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }
                });
            }
        });
        if let Some(pos) = goto {
            d.jump(pos, 0.03, false);
        }
    }

    fn text_panel_ui(&mut self, ui: &mut egui::Ui) {
        let t = self.theme.clone();
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.text_panel.title).strong().size(15.0).color(c(t.text)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("✕").color(c(t.text_dim))).frame(false)).clicked() {
                    self.text_panel.visible = false;
                }
                if ui.add(egui::Button::new(RichText::new("copy").size(12.0).color(c(t.text_dim))).frame(false)).clicked() {
                    ui.ctx().copy_text(self.text_panel.text.clone());
                }
            });
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            ui.add(egui::Label::new(RichText::new(&self.text_panel.text).size(14.0).color(c(t.text))).wrap());
        });
    }

    fn peek_window(&mut self, ctx: &egui::Context) {
        let t = self.theme.clone();
        let Some(d) = &mut self.doc else { return };
        let Some(pk) = &d.peek else { return };
        let screen = ctx.content_rect();
        let (pw, ph) = d.view.page_size(pk.page);
        let width = (pw * pk.zoom + 24.0).min(screen.width() * 0.9);
        let height = (screen.height() * 0.46).max(200.0);
        let (page, y, zoom, target, title) = (pk.page, pk.y, pk.zoom, pk.target, pk.title.clone());
        let mode = d.page_mode;
        let page_bg = if mode == PageMode::Normal { Color32::WHITE } else { c(t.page_bg) };
        egui::Area::new(egui::Id::unique("peek"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -40.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(c(t.surface))
                    .stroke(Stroke::new(1.0, c(t.border)))
                    .corner_radius(radius(&t))
                    .shadow(ui.visuals().window_shadow)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(width - 20.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&title).strong().color(c(t.accent)));
                            ui.label(RichText::new(format!("page {}", page + 1)).color(c(t.text_dim)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new("j/k scroll · J/K page · Enter go · Esc close").small().color(c(t.text_dim)),
                                );
                            });
                        });
                        let (rect, _) = ui.allocate_exact_size(Vec2::new(width - 20.0, height), Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 4.0, c(t.bg));
                        let page_w = pw * zoom;
                        let left = rect.left() + ((rect.width() - page_w) * 0.5).max(0.0);
                        let pr = Rect::from_min_size(Pos2::new(left, rect.top() - y * zoom), Vec2::new(page_w, ph * zoom));
                        painter.rect_filled(pr, 0.0, page_bg);
                        let mut keys: Vec<TileKey> = d
                            .textures
                            .keys()
                            .filter(|k| k.page == page && k.mode == mode && k.generation == d.generation)
                            .copied()
                            .collect();
                        let want = ((zoom * ctx.pixels_per_point() * self.cfg.general.render_oversample.clamp(0.5, 3.0)) * 1000.0)
                            .round() as u32;
                        keys.sort_by_key(|k| (k.scale_milli == want, k.scale_milli));
                        for k in keys {
                            let s = k.scale();
                            let tex = &d.textures[&k];
                            let size = tex.handle.size();
                            let min =
                                pr.min + Vec2::new(k.tx as f32 * TILE as f32 / s * zoom, k.ty as f32 * TILE as f32 / s * zoom);
                            let tr = Rect::from_min_size(min, Vec2::new(size[0] as f32 / s * zoom, size[1] as f32 / s * zoom));
                            painter.image(
                                tex.handle.id(),
                                tr,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        if let Some(r) = target {
                            let sr =
                                Rect::from_min_max(pr.min + Vec2::new(r.x0, r.y0) * zoom, pr.min + Vec2::new(r.x1, r.y1) * zoom);
                            painter.rect_stroke(sr.expand(3.0), 3.0, Stroke::new(2.0, c(t.accent)), StrokeKind::Outside);
                        }
                    });
            });
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let life = Duration::from_millis(3800);
        self.toasts.retain(|t| t.born.elapsed() < life);
        if self.toasts.is_empty() {
            return;
        }
        let t = self.theme.clone();
        let bottom = if self.show_statusbar { 40.0 } else { 12.0 };
        egui::Area::new(egui::Id::unique("toasts"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-14.0, -bottom))
            .interactable(false)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    for toast in &self.toasts {
                        let age = toast.born.elapsed().as_secs_f32();
                        let alpha = ((life.as_secs_f32() - age) / 0.4).clamp(0.0, 1.0);
                        let bar = match toast.level {
                            Level::Info => t.accent,
                            Level::Warn => t.search_match,
                            Level::Error => t.search_current,
                        };
                        ui.scope(|ui| {
                            ui.set_opacity(alpha);
                            egui::Frame::new()
                                .fill(c(t.surface_alt))
                                .stroke(Stroke::new(1.0, c(t.border)))
                                .corner_radius(radius(&t))
                                .inner_margin(egui::Margin { left: 12, right: 12, top: 7, bottom: 7 })
                                .show(ui, |ui| {
                                    ui.set_max_width(420.0);
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(Vec2::new(3.0, 16.0), Sense::hover());
                                        ui.painter().rect_filled(r, 2.0, with_alpha(bar, 255));
                                        ui.add(egui::Label::new(RichText::new(toast_text(&toast.text)).color(c(t.text))).wrap());
                                    });
                                });
                        });
                        ui.add_space(6.0);
                    }
                });
            });
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

/// At most 4 lines / 240 chars, so a long error can't push the stack off screen.
/// Errors are printed in full to stderr.
fn toast_text(text: &str) -> String {
    let mut out: String = text.lines().take(4).collect::<Vec<_>>().join("\n");
    if out.chars().count() > 240 {
        out = out.chars().take(240).collect();
    }
    if out.len() < text.trim_end().len() {
        out.push('…');
    }
    out
}

fn banner(painter: &egui::Painter, rect: Rect, t: &crate::config::Theme, text: &str) {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(13.0), c(t.text));
    let size = galley.size() + Vec2::new(24.0, 12.0);
    let r = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 24.0), size);
    painter.rect_filled(r, 8.0, with_alpha(t.surface, 235));
    painter.rect_stroke(r, 8.0, Stroke::new(1.0, c(t.border)), StrokeKind::Inside);
    painter.galley(r.min + Vec2::new(12.0, 6.0), galley, c(t.text));
}

fn thumb_key(d: &Session, page: usize, mode: PageMode, generation: u32) -> TileKey {
    let (w, h) = d.view.page_size(page);
    let s = (TILE as f32 / w).min(TILE as f32 / h).min(0.5);
    TileKey { page, scale_milli: (s * 1000.0) as u32, tx: 0, ty: 0, mode, generation }
}

/// Tiles of a page at `scale` intersecting the viewport band `[y0, y1)` (screen units).
fn page_tiles(d: &Session, page: usize, scale: f32, band: (f32, f32), mode: PageMode, generation: u32, out: &mut Vec<TileKey>) {
    let (x, y, w, h) = d.view.page_rect(page);
    let vw = d.view.viewport.0;
    let to_px = scale / d.view.zoom;
    let top = (band.0 - y).max(0.0);
    let bottom = (band.1 - y).min(h);
    let left = (-x).max(0.0);
    let right = (vw - x).min(w);
    if bottom <= top || right <= left {
        return;
    }
    let size = d.view.page_size(page);
    let sm = (scale * 1000.0).round() as u32;
    let ty0 = (top * to_px) as u32 / TILE;
    let ty1 = ((bottom * to_px) as u32).saturating_sub(1) / TILE;
    let tx0 = (left * to_px) as u32 / TILE;
    let tx1 = ((right * to_px) as u32).saturating_sub(1) / TILE;
    // Center-out order so the middle of the screen sharpens first.
    let center = (d.view.viewport.1 * 0.5 - y) * to_px / TILE as f32;
    let mut tiles = Vec::new();
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let (_, _, tw, th) = tile_rect(size, sm as f32 / 1000.0, tx, ty);
            if tw > 0 && th > 0 {
                tiles.push(TileKey { page, scale_milli: sm, tx, ty, mode, generation });
            }
        }
    }
    tiles.sort_by(|a, b| ((a.ty as f32 + 0.5 - center).abs()).partial_cmp(&(b.ty as f32 + 0.5 - center).abs()).unwrap());
    out.extend(tiles);
}
