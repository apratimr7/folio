//! Smart references: detection on fixture pages and resolution through the text worker, as
//! `folio --debug findref` does it.

use std::time::Duration;

use super::{fixture, line_at, open_fixture, synthetic_text};
use crate::doc::Backend;
use crate::refs::{self, RefKind, Reference};
use crate::text::{Request, Response, TextWorker};

/// `(kind, label, matched text)` of every reference detected on a page.
fn detected(name: &str, page: usize) -> Vec<(RefKind, String, String)> {
    let t = open_fixture(name).page_text(page);
    refs::detect(&t).into_iter().map(|r| (r.kind, r.label, t.span_text(r.start, r.end))).collect()
}

fn find(name: &str, page: usize, kind: RefKind, label: &str) -> Reference {
    let t = open_fixture(name).page_text(page);
    refs::detect(&t)
        .into_iter()
        .find(|r| r.kind == kind && r.label == label)
        .unwrap_or_else(|| panic!("{kind:?} {label:?} not detected on {name} page {page}"))
}

/// Resolve a reference detected on `page` like the app does; returns the target page and the line there.
fn resolve(name: &str, page: usize, kind: RefKind, label: &str) -> Option<(usize, String)> {
    let r = find(name, page, kind, label);
    let worker = TextWorker::new(fixture(name), || {});
    worker.send(Request::FindRef { id: 7, kind, label: label.into(), from_page: page, from_char: r.start });
    let Ok(Response::Ref { id: 7, target }) = worker.rx.recv_timeout(Duration::from_secs(20)) else {
        panic!("no answer from the text worker")
    };
    let (p, rect) = target?;
    let line = line_at(&open_fixture(name).page_text(p), rect).unwrap_or_default();
    Some((p, line))
}

fn assert_resolves(name: &str, page: usize, kind: RefKind, label: &str, want_page: usize, want_line: &str) {
    let got = resolve(name, page, kind, label);
    assert!(
        got.as_ref().is_some_and(|(p, l)| *p == want_page && l.starts_with(want_line)),
        "{kind:?} {label:?} on {name} page {page}: expected page {want_page} line starting {want_line:?}, got {got:?}"
    );
}

#[test]
fn detects_citations_with_lists_and_ranges() {
    let refs = detected("refs.pdf", 3);
    let cites: Vec<(&str, &str)> =
        refs.iter().filter(|r| r.0 == RefKind::Citation).map(|r| (r.1.as_str(), r.2.as_str())).collect();
    assert_eq!(cites, [("1", "[1]"), ("2", "[2, 3]"), ("4", "[4–6]")]);
}

#[test]
fn detects_figure_table_theorem_and_section_references() {
    let refs = detected("refs.pdf", 3);
    for (kind, label) in [
        (RefKind::Table, "2"),
        (RefKind::Figure, "1"),
        (RefKind::Theorem, "Theorem 1.2"),
        (RefKind::Theorem, "Lemma 3.1"),
        (RefKind::Section, "2"),
    ] {
        assert!(refs.iter().any(|r| r.0 == kind && r.1 == label), "{kind:?} {label:?} not detected in {refs:#?}");
    }
}

#[test]
fn detects_equation_references_but_not_equation_numbers() {
    let eqs: Vec<(String, String)> =
        detected("refs.pdf", 4).into_iter().filter(|r| r.0 == RefKind::Equation).map(|r| (r.1, r.2)).collect();
    // "Eq. (2.1)" and the bare "(2.1)" in the text; the display numbers (2.1) and (2.2) end their lines.
    assert_eq!(eqs, [("2.1".into(), "Eq. (2.1)".into()), ("2.1".into(), "(2.1)".into())]);
}

#[test]
fn detected_references_are_sorted_and_disjoint() {
    let t = open_fixture("refs.pdf").page_text(3);
    let refs = refs::detect(&t);
    assert!(refs.windows(2).all(|w| w[0].end <= w[1].start), "{refs:#?}");
}

#[test]
fn citation_resolves_to_bibliography_entry() {
    assert_resolves("refs.pdf", 3, RefKind::Citation, "1", 6, "[1] A. Alpha");
}

#[test]
fn citation_range_resolves_to_its_first_entry() {
    assert_resolves("refs.pdf", 3, RefKind::Citation, "4", 6, "[4] D. Delta");
}

#[test]
fn figure_resolves_to_caption() {
    assert_resolves("refs.pdf", 3, RefKind::Figure, "1", 3, "Figure 1: The experimental setup.");
}

#[test]
fn table_resolves_to_its_own_caption() {
    assert_resolves("refs.pdf", 3, RefKind::Table, "2", 4, "Table 2: Measured data.");
}

#[test]
fn equation_resolves_to_numbered_display() {
    assert_resolves("refs.pdf", 4, RefKind::Equation, "2.1", 4, "E = mc");
}

#[test]
fn equation_on_a_later_page_resolves_backwards() {
    assert_resolves("refs.pdf", 5, RefKind::Equation, "2.2", 4, "F = ma.");
}

#[test]
fn theorem_resolves_to_its_header() {
    assert_resolves("refs.pdf", 3, RefKind::Theorem, "Theorem 1.2", 3, "Theorem 1.2. The main estimate");
}

#[test]
fn lemma_resolves_to_a_later_page() {
    assert_resolves("refs.pdf", 3, RefKind::Theorem, "Lemma 3.1", 5, "Lemma 3.1. The key estimate");
}

#[test]
fn bracketed_equation_number_resolves_in_number_first_style() {
    assert_resolves("rudin_style.pdf", 1, RefKind::Equation, "3.21", 1, "ak (3.21)");
}

#[test]
#[ignore = "pending: bug 3 in TODO.md"]
fn number_first_theorem_resolves() {
    assert_resolves("rudin_style.pdf", 1, RefKind::Theorem, "Theorem 2.36", 0, "2.36 Theorem.");
}

#[test]
#[ignore = "pending: bug 3 in TODO.md"]
fn number_first_definition_resolves() {
    assert_resolves("rudin_style.pdf", 1, RefKind::Theorem, "Definition 3.9", 1, "3.9 Definition.");
}

#[test]
fn section_fallback_resolves_subsection_heading() {
    assert_resolves("nooutline.pdf", 1, RefKind::Section, "3.2.1", 2, "3.2.1 Details");
}

#[test]
#[ignore = "pending: bug 13 in TODO.md"]
fn section_fallback_skips_table_of_contents() {
    assert_resolves("nooutline.pdf", 1, RefKind::Section, "3.2", 2, "3.2 Results");
}

#[test]
#[ignore = "pending: bug 12 in TODO.md"]
fn captions_and_theorem_headers_are_not_references() {
    let refs = detected("refs.pdf", 3);
    let selfrefs: Vec<_> = refs
        .iter()
        .filter(|r| r.2 == "Figure 1" && r.0 == RefKind::Figure || r.1 == "Theorem 1.1" || r.1 == "Theorem 1.2")
        .collect();
    // Only "Figure 1" and "Theorem 1.2" in the running text may remain.
    assert_eq!(selfrefs.len(), 2, "caption or header detected as a reference to itself: {refs:#?}");
    let tables = detected("refs.pdf", 4).into_iter().filter(|r| r.0 == RefKind::Table).count();
    assert_eq!(tables, 0, "table captions detected as references");
}

#[test]
fn bibliography_page_is_recognized() {
    let d = open_fixture("refs.pdf");
    assert!(refs::is_bibliography(&d.page_text(6)));
    assert!(!refs::is_bibliography(&d.page_text(3)));
}

#[test]
fn find_definitions_strong_and_weak_citations() {
    let t = synthetic_text(&["[12] A. Author, Title.", "12. B. Author, Other.", "12 Results look good"]);
    let defs = refs::find_definitions(&t, RefKind::Citation, "12");
    let strong: Vec<bool> = defs.iter().map(|d| d.2).collect();
    assert_eq!(strong, [true, false, false]);
}

#[test]
fn find_definitions_caption_punctuation_is_strong() {
    let t = synthetic_text(&["Figure 3: A plot.", "Figure 3 shows a plot.", "Figure 31: Another."]);
    let defs = refs::find_definitions(&t, RefKind::Figure, "3");
    assert_eq!(defs.iter().map(|d| d.2).collect::<Vec<_>>(), [true, false], "Figure 31 must not match label 3");
}

#[test]
fn find_definitions_equation_number_at_line_end() {
    let t = synthetic_text(&["a = b (1.2)", "see (1.2) above"]);
    let defs = refs::find_definitions(&t, RefKind::Equation, "1.2");
    assert_eq!(defs.len(), 1);
    assert_eq!(t.span_text(defs[0].0, defs[0].1), "(1.2)");
}

#[test]
fn find_definitions_appendix_and_chapter() {
    let t = synthetic_text(&["Appendix B Proofs", "Chapter 4 Waves"]);
    assert_eq!(refs::find_definitions(&t, RefKind::Section, "B").len(), 1);
    assert_eq!(refs::find_definitions(&t, RefKind::Section, "4").len(), 1);
}
