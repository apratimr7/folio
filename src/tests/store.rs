//! Persistent state (`store.rs`). Every test runs against a temp `XDG_DATA_HOME`.

use std::path::{Path, PathBuf};

use super::IsolatedEnv;
use crate::doc::{DocPos, Rect};
use crate::store::{Bookmark, DocState, Global, Highlight};

fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}

fn sample_state(path: &Path) -> DocState {
    let mut s = DocState { path: path.to_path_buf(), ..Default::default() };
    s.pos = Some(DocPos { page: 12, y: 345.5 });
    s.zoom = Some(1.75);
    s.fit_width = true;
    s.marks.insert('a', DocPos { page: 3, y: 10.0 });
    s.marks.insert('z', DocPos { page: 40, y: 0.0 });
    s.bookmarks.push(Bookmark { pos: DocPos { page: 7, y: 99.0 }, label: "2.1 Setup".into(), created: 1_700_000_000 });
    s.highlights.push(Highlight {
        page: 5,
        start: 10,
        end: 42,
        rects: vec![Rect::new(1.0, 2.0, 3.0, 4.0).into()],
        text: "a “quoted” ﬁeld".into(),
        color: "yellow".into(),
        note: "check this".into(),
        created: 1_700_000_001,
    });
    s
}

/// The only state file under `$XDG_DATA_HOME/folio/docs`.
fn only_doc_file(env: &IsolatedEnv) -> PathBuf {
    let dir = env.data.path().join("folio/docs");
    let files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    assert_eq!(files.len(), 1, "expected one state file in {}: {files:?}", dir.display());
    files.into_iter().next().unwrap()
}

#[test]
fn doc_state_round_trips() {
    let _env = IsolatedEnv::new();
    let path = Path::new("/papers/round-trip.pdf");
    let saved = sample_state(path);
    saved.save();
    assert_eq!(json(&DocState::load(path)), json(&saved));
}

#[test]
fn doc_state_is_written_under_xdg_data_home() {
    let env = IsolatedEnv::new();
    sample_state(Path::new("/papers/where.pdf")).save();
    let file = only_doc_file(&env);
    assert_eq!(file.extension().and_then(|e| e.to_str()), Some("json"));
    assert_eq!(file.file_stem().unwrap().len(), 16, "file name is a 64-bit hex hash");
}

#[test]
fn doc_states_of_different_files_are_separate() {
    let _env = IsolatedEnv::new();
    let (a, b) = (Path::new("/papers/a.pdf"), Path::new("/papers/b.pdf"));
    let mut sa = DocState { path: a.into(), ..Default::default() };
    sa.pos = Some(DocPos { page: 1, y: 0.0 });
    let mut sb = DocState { path: b.into(), ..Default::default() };
    sb.pos = Some(DocPos { page: 2, y: 0.0 });
    sa.save();
    sb.save();
    assert_eq!(DocState::load(a).pos.map(|p| p.page), Some(1));
    assert_eq!(DocState::load(b).pos.map(|p| p.page), Some(2));
}

#[test]
fn missing_doc_state_loads_defaults() {
    let _env = IsolatedEnv::new();
    let path = Path::new("/papers/never-opened.pdf");
    let s = DocState::load(path);
    assert_eq!(s.path, path);
    assert!(s.pos.is_none() && s.zoom.is_none() && s.marks.is_empty() && s.bookmarks.is_empty() && s.highlights.is_empty());
}

#[test]
fn corrupt_doc_state_loads_defaults() {
    let env = IsolatedEnv::new();
    let path = Path::new("/papers/corrupt.pdf");
    sample_state(path).save();
    std::fs::write(only_doc_file(&env), b"{ not json").unwrap();
    let s = DocState::load(path);
    assert_eq!(s.path, path);
    assert!(s.pos.is_none() && s.highlights.is_empty());
}

#[test]
fn doc_state_with_only_a_path_loads() {
    let env = IsolatedEnv::new();
    let path = Path::new("/papers/minimal.pdf");
    sample_state(path).save();
    std::fs::write(only_doc_file(&env), br#"{ "path": "/papers/minimal.pdf" }"#).unwrap();
    let s = DocState::load(path);
    assert_eq!(s.path, path);
    assert!(s.pos.is_none() && !s.fit_width && s.marks.is_empty());
}

#[test]
fn global_state_round_trips() {
    let _env = IsolatedEnv::new();
    let mut g = Global::default();
    g.touch_recent(Path::new("/papers/one.pdf"), "One", 3);
    g.touch_recent(Path::new("/papers/two.pdf"), "Two", 9);
    g.marks.insert('A', (PathBuf::from("/papers/one.pdf"), DocPos { page: 3, y: 50.0 }));
    g.save();
    assert_eq!(json(&Global::load()), json(&g));
}

#[test]
fn touch_recent_moves_entry_to_front_without_duplicates() {
    let mut g = Global::default();
    g.touch_recent(Path::new("/a.pdf"), "A", 1);
    g.touch_recent(Path::new("/b.pdf"), "B", 2);
    g.touch_recent(Path::new("/a.pdf"), "A again", 5);
    let recent: Vec<(&str, usize)> = g.recent.iter().map(|r| (r.title.as_str(), r.page)).collect();
    assert_eq!(recent, [("A again", 5), ("B", 2)]);
}

#[test]
fn missing_global_state_loads_defaults() {
    let _env = IsolatedEnv::new();
    let g = Global::load();
    assert!(g.recent.is_empty() && g.marks.is_empty());
}

#[test]
fn corrupt_global_state_loads_defaults() {
    let env = IsolatedEnv::new();
    let dir = env.data.path().join("folio");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("global.json"), b"\0\xff garbage").unwrap();
    let g = Global::load();
    assert!(g.recent.is_empty() && g.marks.is_empty());
}
