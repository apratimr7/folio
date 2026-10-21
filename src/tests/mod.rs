//! Unit tests that need the fixture PDFs in `tests/fixtures/` or touch several modules. See `docs/TESTING.md`.

mod backend;
mod commands;
mod config;
mod hints;
mod refs;
mod script;
mod search;
mod store;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::doc::{Backend, MupdfDoc, PageText, Rect};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

pub fn open_fixture(name: &str) -> MupdfDoc {
    MupdfDoc::open(&fixture(name)).unwrap_or_else(|e| panic!("cannot open fixture {name}: {e:#}"))
}

/// The visual lines of a page as strings.
pub fn line_strings(text: &PageText) -> Vec<String> {
    text.lines().into_iter().map(|(s, e)| text.chars[s..e].iter().collect()).collect()
}

/// The text of the visual line that contains the center of `rect`, as `span_text` gives it.
pub fn line_at(text: &PageText, rect: Rect) -> Option<String> {
    let (x, y) = ((rect.x0 + rect.x1) * 0.5, rect.center_y());
    text.lines().into_iter().find_map(|(s, e)| {
        let b = text.span_rects(s, e).into_iter().reduce(|a, b| a.union(&b))?;
        (b.x0 <= x && x <= b.x1 && b.y0 <= y && y <= b.y1).then(|| text.span_text(s, e))
    })
}

/// Synthetic page text: one char box of 5×10 pt per char, one line per entry.
pub fn synthetic_text(lines: &[&str]) -> PageText {
    let mut t = PageText::default();
    for (row, line) in lines.iter().enumerate() {
        if row > 0 {
            t.chars.push('\n');
            t.rects.push(Rect::default());
        }
        let y = 20.0 * row as f32;
        for (col, c) in line.chars().enumerate() {
            let x = 5.0 * col as f32;
            t.chars.push(c);
            t.rects.push(Rect::new(x, y, x + 5.0, y + 10.0));
        }
    }
    t
}

/// A directory under the system temp dir, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("folio-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `XDG_CONFIG_HOME` and `XDG_DATA_HOME` pointed at fresh temp dirs for as long as this lives. Tests that read or
/// write config or state files hold one, which also serializes them (the environment is process-wide).
pub struct IsolatedEnv {
    pub config: TempDir,
    pub data: TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl IsolatedEnv {
    pub fn new() -> Self {
        static LOCK: Mutex<()> = Mutex::new(());
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (config, data) = (TempDir::new(), TempDir::new());
        // SAFETY: every test that reads or writes these variables holds `LOCK`.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", config.path());
            std::env::set_var("XDG_DATA_HOME", data.path());
        }
        Self { config, data, _lock: lock }
    }

    /// `$XDG_CONFIG_HOME/folio`, created.
    pub fn config_dir(&self) -> PathBuf {
        let dir = self.config.path().join("folio");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

/// Page size check shared by the backend tests.
pub fn assert_within_page(doc: &dyn Backend, page: usize, r: Rect, what: &str) {
    let (w, h) = doc.page_size(page);
    let tol = 1.0;
    assert!(
        r.x0 >= -tol && r.y0 >= -tol && r.x1 <= w + tol && r.y1 <= h + tol,
        "{what} {r:?} lies outside page {page} ({w}×{h})"
    );
}
