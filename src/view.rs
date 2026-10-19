//! Continuous-scroll layout and camera.
//!
//! Pages are stacked vertically and centered horizontally. "Screen units" are
//! egui points; `zoom` is screen units per PDF point.

use crate::doc::DocPos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    None,
    Width,
    Page,
}

pub struct View {
    pub zoom: f32,
    /// Distance from the top of the document to the top of the viewport (screen units).
    pub scroll_y: f32,
    /// Where `scroll_y` is animating to.
    pub target_y: f32,
    /// Horizontal offset of the page column (screen units, 0 = centered).
    pub scroll_x: f32,
    pub fit: Fit,
    pub viewport: (f32, f32),
    pub gap: f32,
    sizes: Vec<(f32, f32)>,
    tops: Vec<f32>,
    total: f32,
    max_w: f32,
}

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 12.0;

impl View {
    pub fn new(sizes: Vec<(f32, f32)>, gap: f32) -> Self {
        let mut v = Self {
            zoom: 1.0,
            scroll_y: 0.0,
            target_y: 0.0,
            scroll_x: 0.0,
            fit: Fit::Width,
            viewport: (800.0, 600.0),
            gap,
            sizes,
            tops: Vec::new(),
            total: 0.0,
            max_w: 0.0,
        };
        v.relayout();
        v
    }

    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    pub fn page_size(&self, page: usize) -> (f32, f32) {
        self.sizes.get(page).copied().unwrap_or((612.0, 792.0))
    }

    fn relayout(&mut self) {
        self.tops.clear();
        let mut y = self.gap;
        self.max_w = 0.0;
        for &(w, h) in &self.sizes {
            self.tops.push(y);
            y += h * self.zoom + self.gap;
            self.max_w = self.max_w.max(w);
        }
        self.total = y;
    }

    /// Top of a page in document screen units.
    pub fn page_top(&self, page: usize) -> f32 {
        self.tops.get(page).copied().unwrap_or(0.0)
    }

    /// Page rect relative to the viewport's top-left corner: (x, y, w, h).
    pub fn page_rect(&self, page: usize) -> (f32, f32, f32, f32) {
        let (w, h) = self.page_size(page);
        let (w, h) = (w * self.zoom, h * self.zoom);
        let x = (self.viewport.0 - w) * 0.5 + self.scroll_x;
        (x, self.page_top(page) - self.scroll_y, w, h)
    }

    /// The page at a document y coordinate (screen units).
    pub fn page_at(&self, y: f32) -> usize {
        match self.tops.binary_search_by(|t| t.partial_cmp(&y).unwrap()) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
        .min(self.sizes.len().saturating_sub(1))
    }

    pub fn visible_pages(&self) -> std::ops::Range<usize> {
        if self.sizes.is_empty() {
            return 0..0;
        }
        let first = self.page_at(self.scroll_y);
        let mut last = self.page_at(self.scroll_y + self.viewport.1);
        last = last.max(first);
        first..last + 1
    }

    /// Convert a viewport-relative point to (page, x, y) in PDF points.
    pub fn screen_to_doc(&self, sx: f32, sy: f32) -> Option<(usize, f32, f32)> {
        let page = self.page_at(sy + self.scroll_y);
        let (x, y, w, h) = self.page_rect(page);
        if sx < x || sx > x + w || sy < y || sy > y + h {
            return None;
        }
        Some((page, (sx - x) / self.zoom, (sy - y) / self.zoom))
    }

    pub fn doc_to_screen(&self, page: usize, x: f32, y: f32) -> (f32, f32) {
        let (px, py, _, _) = self.page_rect(page);
        (px + x * self.zoom, py + y * self.zoom)
    }

    /// The position at a fraction of the viewport height (0 = top).
    pub fn pos_at(&self, frac: f32) -> DocPos {
        let y = self.scroll_y + self.viewport.1 * frac;
        let page = self.page_at(y);
        let local = ((y - self.page_top(page)) / self.zoom).clamp(0.0, self.page_size(page).1);
        DocPos { page, y: local }
    }

    /// The page considered "current": the one covering the upper part of the screen.
    pub fn current_page(&self) -> usize {
        self.pos_at(0.3).page
    }

    fn max_scroll(&self) -> f32 {
        (self.total - self.viewport.1).max(0.0)
    }

    pub fn clamp(&mut self) {
        let max = self.max_scroll();
        self.target_y = self.target_y.clamp(0.0, max);
        self.scroll_y = self.scroll_y.clamp(0.0, max);
        let overflow = ((self.max_w * self.zoom - self.viewport.0) * 0.5).max(0.0);
        self.scroll_x = self.scroll_x.clamp(-overflow, overflow);
    }

    /// Scroll so that `pos` lands at `frac` of the viewport height.
    pub fn goto(&mut self, pos: DocPos, frac: f32, animate: bool) {
        let page = pos.page.min(self.sizes.len().saturating_sub(1));
        self.target_y = self.page_top(page) + pos.y * self.zoom - self.viewport.1 * frac;
        if !animate {
            self.scroll_y = self.target_y;
        }
        self.clamp();
        if !animate {
            self.scroll_y = self.target_y;
        }
    }

    pub fn scroll_by(&mut self, dy: f32, animate: bool) {
        self.target_y += dy;
        if !animate {
            self.scroll_y = self.target_y;
        }
        self.clamp();
    }

    /// Advance the scroll animation. Returns true while still moving.
    pub fn animate(&mut self, dt: f32) -> bool {
        let d = self.target_y - self.scroll_y;
        if d.abs() < 0.5 {
            self.scroll_y = self.target_y;
            return false;
        }
        self.scroll_y += d * (1.0 - (-dt * 22.0).exp());
        true
    }

    /// Change zoom keeping `anchor_frac` of the viewport fixed on the document.
    pub fn set_zoom(&mut self, zoom: f32, anchor_frac: f32) {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let anchor = self.pos_at(anchor_frac);
        // Anchor the *target* position, so zooming mid-animation doesn't jump.
        let target_anchor = {
            let y = self.target_y + self.viewport.1 * anchor_frac;
            let page = self.page_at(y);
            DocPos { page, y: ((y - self.page_top(page)) / self.zoom).clamp(0.0, self.page_size(page).1) }
        };
        let ratio = zoom / self.zoom;
        self.zoom = zoom;
        self.scroll_x *= ratio;
        self.relayout();
        self.goto(anchor, anchor_frac, false);
        let animating_to = self.page_top(target_anchor.page) + target_anchor.y * self.zoom - self.viewport.1 * anchor_frac;
        self.target_y = animating_to;
        self.clamp();
    }

    /// Recompute the zoom for fit modes (call when the viewport size changes).
    pub fn apply_fit(&mut self) {
        let page = self.current_page();
        let (w, h) = self.page_size(page);
        let margin = 24.0;
        let zoom = match self.fit {
            Fit::None => return,
            Fit::Width => (self.viewport.0 - margin) / w.max(1.0),
            Fit::Page => ((self.viewport.0 - margin) / w.max(1.0)).min((self.viewport.1 - self.gap * 2.0) / h.max(1.0)),
        };
        if (zoom - self.zoom).abs() > 1e-4 {
            self.set_zoom(zoom, 0.0);
        }
        self.scroll_x = 0.0;
    }

    pub fn set_viewport(&mut self, w: f32, h: f32) {
        if (w, h) != self.viewport {
            let pos = self.pos_at(0.0);
            self.viewport = (w, h);
            self.apply_fit();
            self.goto(pos, 0.0, false);
        }
    }

    /// Replace page sizes after a reload, keeping the position.
    pub fn set_sizes(&mut self, sizes: Vec<(f32, f32)>) {
        let pos = self.pos_at(0.0);
        self.sizes = sizes;
        self.relayout();
        self.goto(pos, 0.0, false);
    }

    pub fn set_gap(&mut self, gap: f32) {
        let pos = self.pos_at(0.0);
        self.gap = gap;
        self.relayout();
        self.goto(pos, 0.0, false);
    }

    /// Is the document-space y range of a page region on screen?
    pub fn is_visible(&self, page: usize, y0: f32, y1: f32) -> bool {
        let top = self.page_top(page) - self.target_y;
        let a = top + y0 * self.zoom;
        let b = top + y1 * self.zoom;
        a >= 0.0 && b <= self.viewport.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_roundtrip() {
        let mut v = View::new(vec![(600.0, 800.0); 10], 10.0);
        v.fit = Fit::None;
        v.set_viewport(1000.0, 700.0);
        v.goto(DocPos { page: 3, y: 100.0 }, 0.0, false);
        let p = v.pos_at(0.0);
        assert_eq!(p.page, 3);
        assert!((p.y - 100.0).abs() < 0.01);
        v.set_zoom(2.0, 0.0);
        let p = v.pos_at(0.0);
        assert_eq!(p.page, 3);
        assert!((p.y - 100.0).abs() < 0.01);
        let (x, y) = v.doc_to_screen(3, 10.0, 100.0);
        let (pg, dx, dy) = v.screen_to_doc(x, y).unwrap();
        assert_eq!(pg, 3);
        assert!((dx - 10.0).abs() < 0.01 && (dy - 100.0).abs() < 0.01);
    }

    #[test]
    fn fit_width() {
        let mut v = View::new(vec![(500.0, 700.0); 3], 10.0);
        v.set_viewport(1024.0, 768.0);
        assert!((v.zoom - 2.0).abs() < 0.01);
    }
}
