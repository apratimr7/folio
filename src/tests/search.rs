//! Search normalization and matching (`text::find_in_page`, `normalize_page`, `expand_char`).

use super::{open_fixture, synthetic_text};
use crate::doc::{Backend, PageText};
use crate::text::{SearchHit, expand_char, find_in_page, normalize_page};

/// Search like the app does: smart case, normalized query.
fn search(text: &PageText, query: &str) -> Vec<SearchHit> {
    let ignore_case = !query.chars().any(char::is_uppercase);
    let mut needle = Vec::new();
    for c in query.chars() {
        expand_char(c, &mut needle);
    }
    if ignore_case {
        needle = needle.into_iter().flat_map(char::to_lowercase).collect();
    }
    find_in_page(text, 0, &needle, ignore_case)
}

fn matched(text: &PageText, hit: &SearchHit) -> String {
    text.chars[hit.start..hit.end].iter().collect()
}

#[test]
fn lowercase_query_ignores_case() {
    let t = synthetic_text(&["Theorem and theorem and THEOREM"]);
    assert_eq!(search(&t, "theorem").len(), 3);
}

#[test]
fn query_with_uppercase_is_case_sensitive() {
    let t = synthetic_text(&["Theorem and theorem and THEOREM"]);
    let hits = search(&t, "Theorem");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].start, 0);
}

#[test]
fn ligature_in_text_matches_plain_query() {
    let t = synthetic_text(&["the ﬁeld and the ﬄuent"]);
    let hits = search(&t, "field");
    assert_eq!(hits.len(), 1);
    assert_eq!(matched(&t, &hits[0]), "ﬁeld", "the hit covers the ligature char itself");
    assert_eq!(search(&t, "ffluent").len(), 1);
}

#[test]
fn ligature_in_query_matches_plain_text() {
    let t = synthetic_text(&["the field"]);
    assert_eq!(search(&t, "ﬁeld").len(), 1);
}

#[test]
fn word_hyphenated_across_lines_matches() {
    let t = synthetic_text(&["the word renormaliza-", "tion is split"]);
    let hits = search(&t, "renormalization");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rects.len(), 2, "one highlight rect per line: {:?}", hits[0].rects);
}

#[test]
fn line_break_matches_a_space() {
    let t = synthetic_text(&["black", "hole"]);
    assert_eq!(search(&t, "black hole").len(), 1);
}

#[test]
fn typographic_quotes_and_dashes_match_ascii() {
    let t = synthetic_text(&["its “quoted” folklore – and ‘single’ — or x − y"]);
    assert_eq!(search(&t, "\"quoted\"").len(), 1);
    assert_eq!(search(&t, "folklore - and").len(), 1);
    assert_eq!(search(&t, "'single' - or").len(), 1);
    assert_eq!(search(&t, "x - y").len(), 1);
}

#[test]
fn multiple_hits_have_their_own_rects() {
    let t = synthetic_text(&["ab ab", "ab"]);
    let hits = search(&t, "ab");
    let spans: Vec<(usize, usize)> = hits.iter().map(|h| (h.start, h.end)).collect();
    assert_eq!(spans, [(0, 2), (3, 5), (6, 8)]);
    let rects: Vec<_> = hits.iter().map(|h| h.rects.clone()).collect();
    assert!(rects.iter().all(|r| r.len() == 1), "{rects:?}");
    assert_eq!(rects[0][0].x0, 0.0);
    assert_eq!(rects[1][0].x0, 15.0);
    assert_eq!(rects[2][0].y0, 20.0, "third hit is on the second line");
}

#[test]
fn hits_do_not_overlap() {
    let t = synthetic_text(&["aaaa"]);
    assert_eq!(search(&t, "aa").len(), 2);
}

#[test]
fn no_hits_for_absent_or_too_long_query() {
    let t = synthetic_text(&["short"]);
    assert!(search(&t, "missing").is_empty());
    assert!(search(&t, "short and longer").is_empty());
}

#[test]
fn normalize_page_maps_back_to_source_chars() {
    let t = synthetic_text(&["ﬁ  x-", "y"]);
    let (chars, map) = normalize_page(&t, false);
    assert_eq!(chars.iter().collect::<String>(), "fi xy", "ligature expanded, spaces collapsed, hyphen joined");
    assert_eq!(map, [0, 0, 1, 3, 6]);
}

#[test]
fn normalize_page_lowercases_when_ignoring_case() {
    let t = synthetic_text(&["ABC"]);
    assert_eq!(normalize_page(&t, true).0, ['a', 'b', 'c']);
    assert_eq!(normalize_page(&t, false).0, ['A', 'B', 'C']);
}

#[test]
fn expand_char_table() {
    let cases = [
        ('ﬀ', "ff"),
        ('ﬁ', "fi"),
        ('ﬂ', "fl"),
        ('ﬃ', "ffi"),
        ('ﬄ', "ffl"),
        ('’', "'"),
        ('′', "'"),
        ('“', "\""),
        ('—', "-"),
        ('−', "-"),
        ('\u{a0}', " "),
        ('\n', " "),
        ('x', "x"),
    ];
    for (c, want) in cases {
        let mut out = Vec::new();
        expand_char(c, &mut out);
        assert_eq!(out.iter().collect::<String>(), want, "expand_char({c:?})");
    }
}

#[test]
fn fixture_search_finds_hyphenated_word_and_quotes() {
    let d = open_fixture("refs.pdf");
    let t = d.page_text(3);
    let hits = search(&t, "renormalization");
    assert_eq!(hits.len(), 1, "renormaliza-/tion is split across two lines in refs.pdf");
    assert_eq!(hits[0].rects.len(), 2);
    assert_eq!(search(&t, "\"quoted\" folklore - and").len(), 1);
    assert_eq!(search(&t, "field").len(), 1);
}

#[test]
fn fixture_search_counts_hits_per_page() {
    let d = open_fixture("refs.pdf");
    let counts: Vec<usize> = (0..d.page_count()).map(|p| search(&d.page_text(p), "theorem").len()).collect();
    // p3: "Theorem 1.2" in the text and both theorem headers; p5: "Theorem 1.1" in the proof.
    assert_eq!(counts, [0, 0, 0, 3, 0, 1, 0]);
}
