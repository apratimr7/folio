#![allow(unsafe_op_in_unsafe_fn)]
//! Poppler (poppler-glib) backend.
//!
//! The safe `poppler-rs` bindings cover rendering and metadata; the text
//! layout, outline actions and link mappings go through raw FFI.

use std::ffi::CStr;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use glib::translate::ToGlibPtr;
use poppler::ffi;

use super::{Backend, Bitmap, DocPos, Link, LinkTarget, Metadata, OutlineItem, PageText, Rect};

pub struct PopplerDoc {
    doc: poppler::Document,
    sizes: Vec<(f32, f32)>,
}

impl PopplerDoc {
    pub fn open(path: &Path) -> Result<Self> {
        let abs = std::fs::canonicalize(path).with_context(|| format!("cannot open {}", path.display()))?;
        let uri = glib::filename_to_uri(&abs, None).map_err(|e| anyhow!("{e}"))?;
        let doc = poppler::Document::from_file(&uri, None).map_err(|e| anyhow!("failed to load PDF: {e}"))?;
        let n = doc.n_pages().max(0) as usize;
        let sizes = (0..n)
            .map(|i| doc.page(i as i32).map(|p| p.size()).map(|(w, h)| (w as f32, h as f32)).unwrap_or((612.0, 792.0)))
            .collect();
        Ok(Self { doc, sizes })
    }

    fn page(&self, page: usize) -> Option<poppler::Page> {
        self.doc.page(page as i32)
    }

    /// Resolve a destination to a page position. Takes ownership of nothing.
    unsafe fn resolve_dest(&self, dest: *mut ffi::PopplerDest) -> Option<DocPos> {
        if dest.is_null() {
            return None;
        }
        let d = &*dest;
        if d.type_ == ffi::POPPLER_DEST_NAMED {
            if d.named_dest.is_null() {
                return None;
            }
            let named = ffi::poppler_document_find_dest(self.doc.to_glib_none().0, d.named_dest);
            if named.is_null() {
                return None;
            }
            let pos = self.resolve_concrete_dest(&*named);
            ffi::poppler_dest_free(named);
            return pos;
        }
        self.resolve_concrete_dest(d)
    }

    fn resolve_concrete_dest(&self, d: &ffi::PopplerDest) -> Option<DocPos> {
        if d.page_num < 1 {
            return None;
        }
        let page = (d.page_num - 1) as usize;
        let h = self.sizes.get(page)?.1;
        // PDF destinations use a bottom-left origin; only some types carry a `top`.
        let y = match d.type_ {
            ffi::POPPLER_DEST_XYZ | ffi::POPPLER_DEST_FITH | ffi::POPPLER_DEST_FITBH | ffi::POPPLER_DEST_FITR => {
                (h - d.top as f32).clamp(0.0, h)
            }
            _ => 0.0,
        };
        Some(DocPos { page, y })
    }

    unsafe fn resolve_action(&self, action: *mut ffi::PopplerAction) -> Option<LinkTarget> {
        if action.is_null() {
            return None;
        }
        let a = &*action;
        match a.type_ {
            ffi::POPPLER_ACTION_GOTO_DEST => self.resolve_dest(a.goto_dest.dest).map(LinkTarget::Page),
            ffi::POPPLER_ACTION_URI => Some(LinkTarget::Uri(cstr(a.uri.uri))),
            ffi::POPPLER_ACTION_NAMED => Some(LinkTarget::Other(format!("named:{}", cstr(a.named.named_dest)))),
            ffi::POPPLER_ACTION_GOTO_REMOTE => Some(LinkTarget::Other(format!("file:{}", cstr(a.goto_remote.file_name)))),
            _ => None,
        }
    }

    unsafe fn walk_outline(&self, iter: *mut ffi::PopplerIndexIter, depth: usize, out: &mut Vec<OutlineItem>) {
        loop {
            let action = ffi::poppler_index_iter_get_action(iter);
            if !action.is_null() {
                let title = cstr((*action).any.title);
                let target = match self.resolve_action(action) {
                    Some(LinkTarget::Page(p)) => Some(p),
                    _ => None,
                };
                ffi::poppler_action_free(action);
                out.push(OutlineItem { title: title.split_whitespace().collect::<Vec<_>>().join(" "), depth, target });
            }
            let child = ffi::poppler_index_iter_get_child(iter);
            if !child.is_null() {
                self.walk_outline(child, depth + 1, out);
                ffi::poppler_index_iter_free(child);
            }
            if ffi::poppler_index_iter_next(iter) == 0 {
                break;
            }
        }
    }
}

unsafe fn cstr(p: *const std::os::raw::c_char) -> String {
    if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() }
}

impl Backend for PopplerDoc {
    fn page_count(&self) -> usize {
        self.sizes.len()
    }

    fn page_size(&self, page: usize) -> (f32, f32) {
        self.sizes.get(page).copied().unwrap_or((612.0, 792.0))
    }

    fn page_label(&self, page: usize) -> Option<String> {
        self.page(page)?.label().map(|s| s.to_string())
    }

    fn render(&self, page: usize, scale: f32, x: i32, y: i32, w: u32, h: u32) -> Result<Bitmap> {
        let p = self.page(page).ok_or_else(|| anyhow!("no page {page}"))?;
        let (pw, ph) = self.page_size(page);
        let mut surface = cairo::ImageSurface::create(cairo::Format::Rgb24, w as i32, h as i32)?;
        {
            let cr = cairo::Context::new(&surface)?;
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint()?;
            cr.translate(-x as f64, -y as f64);
            cr.scale(scale as f64, scale as f64);
            cr.rectangle(0.0, 0.0, pw as f64, ph as f64);
            cr.clip();
            p.render(&cr);
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().map_err(|e| anyhow!("{e}"))?;
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h as usize {
            let line = &data[row * stride..row * stride + w as usize * 4];
            for px in line.as_chunks::<4>().0 {
                // Cairo stores native-endian xRGB, i.e. BGRx in memory on little-endian.
                let v = u32::from_ne_bytes([px[0], px[1], px[2], px[3]]);
                pixels.extend_from_slice(&[(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]);
            }
        }
        Ok(Bitmap { width: w, height: h, pixels })
    }

    fn page_text(&self, page: usize) -> PageText {
        let Some(p) = self.page(page) else { return PageText::default() };
        let chars: Vec<char> = p.text().map(|s| s.chars().collect()).unwrap_or_default();
        let mut rects = Vec::with_capacity(chars.len());
        unsafe {
            let mut raw: *mut ffi::PopplerRectangle = std::ptr::null_mut();
            let mut n: std::os::raw::c_uint = 0;
            if ffi::poppler_page_get_text_layout(p.to_glib_none().0, &mut raw, &mut n) != 0 && !raw.is_null() {
                let slice = std::slice::from_raw_parts(raw, n as usize);
                rects.extend(slice.iter().map(|r| Rect::new(r.x1 as f32, r.y1 as f32, r.x2 as f32, r.y2 as f32)));
                glib::ffi::g_free(raw as *mut _);
            }
        }
        // The layout should be 1:1 with chars; guard against mismatches anyway.
        rects.resize(chars.len(), Rect::default());
        PageText { chars, rects }
    }

    fn links(&self, page: usize) -> Vec<Link> {
        let Some(p) = self.page(page) else { return Vec::new() };
        let h = self.page_size(page).1;
        let mut out = Vec::new();
        unsafe {
            let list = ffi::poppler_page_get_link_mapping(p.to_glib_none().0);
            let mut node = list;
            while !node.is_null() {
                let m = (*node).data as *mut ffi::PopplerLinkMapping;
                if !m.is_null() {
                    let a = &(*m).area;
                    // Link areas are in PDF space (bottom-left origin).
                    let rect = Rect::new(a.x1 as f32, h - a.y2 as f32, a.x2 as f32, h - a.y1 as f32);
                    if let Some(target) = self.resolve_action((*m).action) {
                        out.push(Link { rect, target });
                    }
                }
                node = (*node).next;
            }
            if !list.is_null() {
                ffi::poppler_page_free_link_mapping(list);
            }
        }
        out
    }

    fn outline(&self) -> Vec<OutlineItem> {
        let mut out = Vec::new();
        unsafe {
            let iter = ffi::poppler_index_iter_new(self.doc.to_glib_none().0);
            if !iter.is_null() {
                self.walk_outline(iter, 0, &mut out);
                ffi::poppler_index_iter_free(iter);
            }
        }
        out
    }

    fn metadata(&self) -> Metadata {
        Metadata {
            title: self.doc.title().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()),
            author: self.doc.author().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()),
        }
    }
}
