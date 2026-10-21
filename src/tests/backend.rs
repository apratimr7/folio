//! The MuPDF backend through the `Backend` trait, on the fixture PDFs.

use super::{TempDir, assert_within_page, fixture, line_strings, open_fixture};
use crate::doc::{self, Backend, Bitmap, LinkTarget};

const REFS_PAGES: usize = 7;
const LETTER: (f32, f32) = (612.0, 792.0);

#[test]
fn page_counts_and_sizes() {
    for (name, pages) in [("refs.pdf", REFS_PAGES), ("rudin_style.pdf", 2), ("nooutline.pdf", 3), ("twocol.pdf", 1)] {
        let d = open_fixture(name);
        assert_eq!(d.page_count(), pages, "{name}");
        for p in 0..pages {
            let (w, h) = d.page_size(p);
            assert!((w - LETTER.0).abs() < 1.0 && (h - LETTER.1).abs() < 1.0, "{name} page {p} is {w}×{h}, expected US letter");
        }
    }
}

#[test]
fn open_reports_missing_file() {
    let err = doc::open(&fixture("does-not-exist.pdf")).err().expect("opening a missing file must fail");
    assert!(format!("{err:#}").contains("does-not-exist.pdf"), "error should name the file: {err:#}");
}

#[test]
fn page_labels_are_roman_then_arabic() {
    let d = open_fixture("refs.pdf");
    let labels: Vec<Option<String>> = (0..d.page_count()).map(|p| d.page_label(p)).collect();
    let expected = [None, Some("i"), Some("ii"), Some("1"), Some("2"), Some("3"), Some("4")];
    assert_eq!(labels, expected.map(|l| l.map(String::from)), "the title page has an empty label, which must be None");
}

#[test]
fn page_labels_are_none_without_a_label_tree() {
    let d = open_fixture("nooutline.pdf");
    // MuPDF falls back to the page number when there is no /PageLabels.
    let label = d.page_label(0);
    assert!(label.is_none() || label.as_deref() == Some("1"), "unexpected label {label:?}");
}

#[test]
fn outline_titles_depths_and_targets() {
    let d = open_fixture("refs.pdf");
    let outline = d.outline();
    let got: Vec<(&str, usize, Option<usize>)> =
        outline.iter().map(|o| (o.title.as_str(), o.depth, o.target.map(|t| t.page))).collect();
    assert_eq!(got, [("Introduction", 0, Some(3)), ("Motivation", 1, Some(3)), ("Results", 0, Some(4)), ("Proofs", 0, Some(5))]);
}

#[test]
fn outline_targets_point_just_above_their_heading() {
    let d = open_fixture("refs.pdf");
    for item in d.outline() {
        let target = item.target.expect("every outline item has a target");
        let text = d.page_text(target.page);
        let heading = text
            .lines()
            .into_iter()
            .find(|&(s, e)| text.span_text(s, e).ends_with(&item.title))
            .unwrap_or_else(|| panic!("heading {:?} not on page {}", item.title, target.page));
        let y0 = text.span_rects(heading.0, heading.1)[0].y0;
        assert!(
            target.y <= y0 + 2.0 && target.y > y0 - 40.0,
            "{:?}: target y {} should be just above the heading at y {y0}",
            item.title,
            target.y
        );
    }
}

#[test]
fn outline_is_empty_without_bookmarks() {
    assert!(open_fixture("nooutline.pdf").outline().is_empty());
}

#[test]
fn internal_links_resolve_to_pages() {
    let d = open_fixture("refs.pdf");
    let links = d.links(3);
    let pages: Vec<usize> = links
        .iter()
        .filter_map(|l| match l.target {
            LinkTarget::Page(p) => Some(p.page),
            _ => None,
        })
        .collect();
    // [1], [2], [3], [4], [6] (the cite package links the ends of a range), Table 2, Figure 1, Theorem 1.2,
    // Lemma 3.1, Section 2.
    assert_eq!(pages, [6, 6, 6, 6, 6, 4, 3, 3, 5, 4]);
    for l in &links {
        assert_within_page(&d, 3, l.rect, "link rect");
        if let LinkTarget::Page(p) = l.target {
            let (_, h) = d.page_size(p.page);
            assert!((0.0..=h).contains(&p.y), "link target y {} outside the page", p.y);
        }
    }
}

#[test]
fn external_links_keep_their_uri() {
    let d = open_fixture("refs.pdf");
    let links = d.links(3);
    let uris: Vec<&str> = links
        .iter()
        .filter_map(|l| match &l.target {
            LinkTarget::Uri(u) => Some(u.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(uris, ["https://example.org/folio"]);
}

#[test]
fn file_links_are_not_treated_as_web_uris() {
    let d = open_fixture("refs.pdf");
    let other: Vec<String> = d
        .links(3)
        .into_iter()
        .filter_map(|l| match l.target {
            LinkTarget::Other(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(other, ["file:notes.txt"]);
}

#[test]
fn heading_number_and_title_form_one_line() {
    let d = open_fixture("refs.pdf");
    let lines = line_strings(&d.page_text(3));
    assert_eq!(lines.first().map(String::as_str), Some("1 Introduction"), "lines: {lines:#?}");
    assert!(lines.iter().any(|l| l == "1.1 Motivation"), "lines: {lines:#?}");
}

#[test]
fn equation_and_its_number_form_one_line() {
    let lines = line_strings(&open_fixture("refs.pdf").page_text(4));
    assert!(lines.iter().any(|l| l.starts_with("E = mc") && l.ends_with("(2.1)")), "lines: {lines:#?}");
}

#[test]
fn page_text_has_one_rect_per_char_inside_the_page() {
    let d = open_fixture("refs.pdf");
    for p in 0..d.page_count() {
        let t = d.page_text(p);
        assert_eq!(t.chars.len(), t.rects.len(), "page {p}");
        for (c, r) in t.chars.iter().zip(&t.rects) {
            if *c != '\n' {
                assert_within_page(&d, p, *r, &format!("box of {c:?}"));
            }
        }
    }
}

#[test]
fn page_text_lines_run_top_to_bottom() {
    let d = open_fixture("refs.pdf");
    let t = d.page_text(6);
    let ys: Vec<f32> = t.lines().into_iter().map(|(s, e)| t.span_rects(s, e)[0].y0).collect();
    assert!(ys.windows(2).all(|w| w[0] < w[1]), "bibliography lines out of order: {ys:?}");
}

#[test]
fn two_columns_stay_on_separate_lines() {
    let lines = line_strings(&open_fixture("twocol.pdf").page_text(0));
    let left = lines.iter().filter(|l| l.contains("alpha")).count();
    let right = lines.iter().filter(|l| l.contains("omega")).count();
    assert!(left > 5 && right > 5, "expected both columns, got {left} left and {right} right lines");
    let mixed: Vec<&String> = lines.iter().filter(|l| l.contains("alpha") && l.contains("omega")).collect();
    assert!(mixed.is_empty(), "lines mixing both columns: {mixed:#?}");
}

#[test]
fn metadata_has_the_title() {
    let m = open_fixture("refs.pdf").metadata();
    assert_eq!(m.title.as_deref(), Some("Folio Test Paper"));
    assert_eq!(m.author.as_deref(), Some("Folio Tests"));
}

#[test]
fn metadata_title_is_none_when_unset() {
    assert_eq!(open_fixture("twocol.pdf").metadata().title, None);
}

fn pixel(b: &Bitmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * b.width + x) * 4) as usize;
    b.pixels[i..i + 4].try_into().unwrap()
}

#[test]
fn render_has_white_background_and_dark_text() {
    let d = open_fixture("refs.pdf");
    let (w, h) = d.page_size(3);
    let b = d.render(3, 1.0, 0, 0, w as u32, h as u32).expect("render");
    assert_eq!((b.width, b.height), (w as u32, h as u32));
    assert_eq!(b.pixels.len(), (b.width * b.height * 4) as usize);
    assert_eq!(pixel(&b, 5, 5), [255, 255, 255, 255], "page margin should be white");
    // The heading "1 Introduction" sits around x 126–236, y 131–144.
    let dark = (131..144).flat_map(|y| (126..236).map(move |x| (x, y))).filter(|&(x, y)| pixel(&b, x, y)[0] < 100).count();
    assert!(dark > 100, "expected dark heading pixels, found {dark}");
}

#[test]
fn render_tiles_match_one_large_render() {
    let d = open_fixture("refs.pdf");
    let (x, y, w, h) = (240, 300, 200, 120);
    let scale = 1.5;
    let whole = d.render(3, scale, x, y, w, h).unwrap();
    let left = d.render(3, scale, x, y, w / 2, h).unwrap();
    let right = d.render(3, scale, x + w as i32 / 2, y, w / 2, h).unwrap();
    let mut mismatches = 0;
    for row in 0..h {
        for col in 0..w {
            let tile = if col < w / 2 { pixel(&left, col, row) } else { pixel(&right, col - w / 2, row) };
            if tile != pixel(&whole, col, row) {
                mismatches += 1;
            }
        }
    }
    assert_eq!(mismatches, 0, "tiles differ from the single render in {mismatches} pixels");
    assert!(whole.pixels.chunks(4).any(|p| p[0] < 100), "the region should contain text");
}

#[test]
fn render_of_a_missing_page_is_an_error() {
    let d = open_fixture("refs.pdf");
    assert!(d.render(REFS_PAGES, 1.0, 0, 0, 10, 10).is_err());
}

#[test]
fn out_of_range_pages_are_empty_not_panics() {
    let d = open_fixture("refs.pdf");
    assert!(d.page_text(REFS_PAGES + 5).chars.is_empty());
    assert!(d.links(REFS_PAGES + 5).is_empty());
    assert_eq!(d.page_label(REFS_PAGES + 5), None);
}

#[test]
fn encrypted_pdf_asks_for_a_password() {
    let err = doc::open(&fixture("encrypted.pdf")).err().expect("an encrypted PDF must not open without a password");
    assert!(err.to_string().contains("password"), "error should mention the password: {err:#}");
}

/// Exercise everything a damaged file can reach. Errors are fine; a panic or a crash is the bug.
/// Returns how many pages could be read.
fn poke(path: &std::path::Path) -> usize {
    let Ok(d) = doc::open(path) else { return 0 };
    let _ = d.outline();
    let _ = d.metadata();
    for p in 0..d.page_count() {
        let _ = d.page_label(p);
        let _ = d.page_text(p);
        let _ = d.links(p);
        let _ = d.render(p, 0.5, 0, 0, 100, 100);
    }
    (0..d.page_count()).filter(|&p| !d.page_text(p).chars.is_empty()).count()
}

#[test]
fn truncated_files_never_crash() {
    let dir = TempDir::new();
    for name in ["refs.pdf", "nooutline.pdf", "twocol.pdf"] {
        let bytes = std::fs::read(fixture(name)).unwrap();
        for percent in [0, 10, 50, 90, 99] {
            let path = dir.path().join(format!("{percent}-{name}"));
            std::fs::write(&path, &bytes[..bytes.len() * percent / 100]).unwrap();
            // pdflatex keeps the page tree in an object stream at the end, so these mostly open with no pages.
            poke(&path);
        }
    }
}

#[test]
fn files_with_zeroed_chunks_never_crash() {
    let dir = TempDir::new();
    for name in ["refs.pdf", "nooutline.pdf", "twocol.pdf"] {
        let original = std::fs::read(fixture(name)).unwrap();
        let mut readable = 0;
        for percent in [5, 25, 50, 75, 95] {
            let mut bytes = original.clone();
            let at = bytes.len() * percent / 100;
            let end = (at + 2048).min(bytes.len());
            bytes[at..end].fill(0);
            let path = dir.path().join(format!("zeroed-{percent}-{name}"));
            std::fs::write(&path, &bytes).unwrap();
            readable += poke(&path);
        }
        // Make sure the damage gets past `open`, so page-level code is exercised too.
        assert!(readable > 0, "no damaged copy of {name} had readable pages; the test exercises nothing");
    }
}

#[test]
fn garbage_files_never_crash() {
    let dir = TempDir::new();
    let path = dir.path().join("garbage.pdf");
    std::fs::write(&path, b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF")
        .unwrap();
    poke(&path);
    std::fs::write(&path, b"not a pdf at all").unwrap();
    poke(&path);
}

/// Writes `tests/fixtures/encrypted.pdf` (user password "folio") from `twocol.pdf`; there's no qpdf on every
/// machine, so the fixture is made with MuPDF itself. The output differs on every run (random AES IV), so it
/// only writes with `FOLIO_REGENERATE_FIXTURES=1 cargo test generate_encrypted_fixture -- --ignored`.
#[test]
#[ignore = "generator: rewrites tests/fixtures/encrypted.pdf"]
fn generate_encrypted_fixture() {
    use mupdf::pdf::document::Encryption;
    use mupdf::pdf::{PdfDocument, PdfWriteOptions};
    if std::env::var_os("FOLIO_REGENERATE_FIXTURES").is_none() {
        eprintln!("set FOLIO_REGENERATE_FIXTURES=1 to rewrite tests/fixtures/encrypted.pdf");
        return;
    }
    let src = PdfDocument::open(fixture("twocol.pdf").to_str().unwrap()).unwrap();
    let mut opts = PdfWriteOptions::default();
    opts.set_encryption(Encryption::Aes128).set_user_password("folio").set_owner_password("folio-owner");
    src.save_with_options(fixture("encrypted.pdf").to_str().unwrap(), opts).unwrap();
    assert!(doc::open(&fixture("encrypted.pdf")).is_err());
}
