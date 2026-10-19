//! Background text worker: full-document search and reference resolution.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};

use crate::doc::{self, PageText, Rect};
use crate::refs::{self, RefKind};

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub page: usize,
    pub start: usize,
    pub end: usize,
    pub rects: Vec<Rect>,
}

pub enum Request {
    Search { generation: u32, query: String },
    FindRef { id: u64, kind: RefKind, label: String, from_page: usize, from_char: usize },
    Reload,
}

pub enum Response {
    SearchHits { generation: u32, hits: Vec<SearchHit> },
    SearchDone { generation: u32 },
    Ref { id: u64, target: Option<(usize, Rect)> },
}

pub struct TextWorker {
    tx: Sender<Request>,
    pub rx: Receiver<Response>,
    /// Latest search generation; the worker abandons older searches.
    search_gen: Arc<AtomicU32>,
}

impl TextWorker {
    pub fn new(path: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let (tx, req_rx) = crossbeam_channel::unbounded::<Request>();
        let (resp_tx, rx) = crossbeam_channel::unbounded();
        let search_gen = Arc::new(AtomicU32::new(0));
        let latest = search_gen.clone();
        std::thread::Builder::new()
            .name("text".into())
            .spawn(move || {
                let mut index = Index::open(&path);
                for req in req_rx {
                    match req {
                        Request::Reload => index = Index::open(&path),
                        Request::Search { generation, query } => {
                            index.search(&query, generation, &latest, &resp_tx, &wake);
                        }
                        Request::FindRef { id, kind, label, from_page, from_char } => {
                            let target = index.find_ref(kind, &label, from_page, from_char);
                            let _ = resp_tx.send(Response::Ref { id, target });
                            wake();
                        }
                    }
                }
            })
            .expect("spawn text thread");
        Self { tx, rx, search_gen }
    }

    pub fn search(&self, query: String) -> u32 {
        let generation = self.search_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.tx.send(Request::Search { generation, query });
        generation
    }

    pub fn cancel_search(&self) {
        self.search_gen.fetch_add(1, Ordering::SeqCst);
    }

    pub fn send(&self, r: Request) {
        let _ = self.tx.send(r);
    }
}

struct Index {
    doc: Option<Box<dyn doc::Backend>>,
    pages: Vec<Option<Arc<PageText>>>,
}

impl Index {
    fn open(path: &std::path::Path) -> Self {
        let doc = doc::open(path).ok();
        let n = doc.as_ref().map(|d| d.page_count()).unwrap_or(0);
        Self { doc, pages: vec![None; n] }
    }

    fn page(&mut self, i: usize) -> Arc<PageText> {
        if self.pages[i].is_none() {
            let t = self.doc.as_ref().map(|d| d.page_text(i)).unwrap_or_default();
            self.pages[i] = Some(Arc::new(t));
        }
        self.pages[i].clone().unwrap()
    }

    fn search(&mut self, query: &str, generation: u32, current: &AtomicU32, tx: &Sender<Response>, wake: &impl Fn()) {
        let ignore_case = !query.chars().any(char::is_uppercase);
        let needle: Vec<char> = normalize_query(query, ignore_case);
        if needle.is_empty() {
            return;
        }
        let n = self.pages.len();
        for p in 0..n {
            if current.load(Ordering::SeqCst) != generation {
                return;
            }
            let text = self.page(p);
            let hits = find_in_page(&text, p, &needle, ignore_case);
            if !hits.is_empty() {
                let _ = tx.send(Response::SearchHits { generation, hits });
                wake();
            }
        }
        let _ = tx.send(Response::SearchDone { generation });
        wake();
    }

    fn find_ref(&mut self, kind: RefKind, label: &str, from_page: usize, from_char: usize) -> Option<(usize, Rect)> {
        let n = self.pages.len();
        // Scan forward from the reference, then wrap: this finds the *next*
        // bibliography for per-chapter reference lists, and later definitions
        // (appendix figures) as well as earlier ones (equations).
        let order: Vec<usize> = if kind == RefKind::Citation {
            (from_page..n).chain(0..from_page).collect()
        } else {
            (0..n).collect()
        };
        let mut fallback = None;
        let mut biblio_run = 0usize;
        for p in order {
            let text = self.page(p);
            if kind == RefKind::Citation {
                biblio_run = if refs::is_bibliography(&text) { 4 } else { biblio_run.saturating_sub(1) };
            }
            for hit in refs::find_definitions(&text, kind, label) {
                if p == from_page && hit.0 <= from_char && from_char < hit.1 {
                    continue; // that's the reference itself
                }
                let Some(rect) = text.span_rects(hit.0, hit.1).into_iter().reduce(|a, b| a.union(&b)) else { continue };
                if hit.2 {
                    return Some((p, rect));
                }
                if kind == RefKind::Citation && biblio_run == 0 {
                    continue;
                }
                if kind == RefKind::Citation {
                    // The first numbered entry in the next bibliography is the best we have.
                    return Some((p, rect));
                }
                fallback.get_or_insert((p, rect));
            }
        }
        fallback
    }
}

/// Map common ligatures and typographic variants to plain text.
pub fn expand_char(c: char, out: &mut Vec<char>) {
    match c {
        'ﬀ' => out.extend(['f', 'f']),
        'ﬁ' => out.extend(['f', 'i']),
        'ﬂ' => out.extend(['f', 'l']),
        'ﬃ' => out.extend(['f', 'f', 'i']),
        'ﬄ' => out.extend(['f', 'f', 'l']),
        '‘' | '’' | '′' => out.push('\''),
        '“' | '”' => out.push('"'),
        '–' | '—' | '−' => out.push('-'),
        '\n' | '\t' | '\u{a0}' => out.push(' '),
        c => out.push(c),
    }
}

fn normalize_query(q: &str, ignore_case: bool) -> Vec<char> {
    let mut out = Vec::new();
    for c in q.chars() {
        expand_char(c, &mut out);
    }
    if ignore_case {
        out = out.into_iter().flat_map(char::to_lowercase).collect();
    }
    out
}

/// Normalized page text plus, for each normalized char, the index of the source char.
pub fn normalize_page(text: &PageText, ignore_case: bool) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(text.chars.len());
    let mut map = Vec::with_capacity(text.chars.len());
    let mut buf = Vec::new();
    let src = &text.chars;
    let mut i = 0;
    while i < src.len() {
        // "hyphen-\nation" → "hyphenation"
        if src[i] == '-' && src.get(i + 1) == Some(&'\n') {
            i += 2;
            continue;
        }
        buf.clear();
        expand_char(src[i], &mut buf);
        for &c in &buf {
            if c == ' ' && chars.last() == Some(&' ') {
                continue;
            }
            if ignore_case {
                for l in c.to_lowercase() {
                    chars.push(l);
                    map.push(i);
                }
            } else {
                chars.push(c);
                map.push(i);
            }
        }
        i += 1;
    }
    (chars, map)
}

pub fn find_in_page(text: &PageText, page: usize, needle: &[char], ignore_case: bool) -> Vec<SearchHit> {
    let (hay, map) = normalize_page(text, ignore_case);
    let mut hits = Vec::new();
    if needle.len() > hay.len() {
        return hits;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == *needle {
            let start = map[i];
            let end = map[i + needle.len() - 1] + 1;
            hits.push(SearchHit { page, start, end, rects: text.span_rects(start, end) });
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}
