//! Persistent per-document state (position, marks, bookmarks, highlights) and
//! the recent-files list, stored as JSON under `~/.local/share/folio/`.
//!
//! Documents are keyed by canonical path rather than content hash so that a
//! PDF being rebuilt by LaTeX keeps its annotations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::data_dir;
use crate::doc::{DocPos, Rect};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedRect(pub [f32; 4]);

impl From<Rect> for SavedRect {
    fn from(r: Rect) -> Self {
        Self([r.x0, r.y0, r.x1, r.y1])
    }
}

impl From<&SavedRect> for Rect {
    fn from(r: &SavedRect) -> Self {
        Rect::new(r.0[0], r.0[1], r.0[2], r.0[3])
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Highlight {
    pub page: usize,
    pub start: usize,
    pub end: usize,
    pub rects: Vec<SavedRect>,
    pub text: String,
    pub color: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub created: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub pos: DocPos,
    pub label: String,
    #[serde(default)]
    pub created: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DocState {
    pub path: PathBuf,
    #[serde(default)]
    pub pos: Option<DocPos>,
    #[serde(default)]
    pub zoom: Option<f32>,
    #[serde(default)]
    pub fit_width: bool,
    #[serde(default)]
    pub marks: BTreeMap<char, DocPos>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default)]
    pub highlights: Vec<Highlight>,
    /// Clockwise view rotation in degrees.
    #[serde(default)]
    pub rotation: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: PathBuf,
    pub title: String,
    pub opened: u64,
    #[serde(default)]
    pub page: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Global {
    #[serde(default)]
    pub recent: Vec<RecentEntry>,
    /// Uppercase marks work across documents.
    #[serde(default)]
    pub marks: BTreeMap<char, (PathBuf, DocPos)>,
    /// The open tabs when folio last quit, for `restore_tabs`.
    #[serde(default)]
    pub tabs: Vec<PathBuf>,
    #[serde(default)]
    pub active_tab: usize,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// FNV-1a: tiny and stable across Rust versions (unlike `DefaultHasher`).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

fn doc_file(path: &Path) -> PathBuf {
    let key = fnv1a(path.to_string_lossy().as_bytes());
    data_dir().join("docs").join(format!("{key:016x}.json"))
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(tmp, path)
}

impl DocState {
    pub fn load(path: &Path) -> Self {
        std::fs::read(doc_file(path))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| DocState { path: path.to_path_buf(), ..Default::default() })
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self)
            && let Err(e) = write_atomic(&doc_file(&self.path), &json)
        {
            eprintln!("folio: failed to save state: {e}");
        }
    }
}

impl Global {
    fn file() -> PathBuf {
        data_dir().join("global.json")
    }

    pub fn load() -> Self {
        std::fs::read(Self::file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = write_atomic(&Self::file(), &json);
        }
    }

    pub fn touch_recent(&mut self, path: &Path, title: &str, page: usize) {
        self.recent.retain(|r| r.path != path);
        self.recent.insert(0, RecentEntry { path: path.to_path_buf(), title: title.to_string(), opened: now(), page });
        self.recent.truncate(200);
    }
}
