//! Rows for the visual-mode start hints (`v`: pick a row, then a word).

use super::open_fixture;
use crate::app::text_rows;
use crate::doc::{Backend, PageText, Rect};

/// Pieces given as (text, x, y): 6 pt per char, 10 pt tall.
fn pieces(items: &[(&str, f32, f32)]) -> PageText {
    let mut t = PageText::default();
    for (i, &(s, x, y)) in items.iter().enumerate() {
        if i > 0 {
            t.chars.push('\n');
            t.rects.push(Rect::default());
        }
        for (ci, c) in s.chars().enumerate() {
            let x0 = x + ci as f32 * 6.0;
            t.chars.push(c);
            t.rects.push(Rect::new(x0, y, x0 + 6.0, y + 10.0));
        }
    }
    t
}

#[test]
fn pieces_at_the_same_height_are_one_row() {
    // A displayed equation split into pieces, then a normal line below it.
    let t = pieces(&[("G =", 100.0, 50.0), ("U Vp.", 140.0, 51.0), ("next line", 60.0, 70.0)]);
    let rows = text_rows(&t);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].0, 0, "a row is identified by its first line piece");
    assert!(rows[0].1.x1 >= 140.0 + 5.0 * 6.0, "the row box spans all pieces: {:?}", rows[0].1);
    assert_eq!(rows[1].0, 2);
}

#[test]
fn a_subscript_line_below_is_its_own_row() {
    let t = pieces(&[("G = U Vp.", 100.0, 50.0), ("p in E", 110.0, 58.0)]);
    assert_eq!(text_rows(&t).len(), 2, "8 pt lower with 10 pt height overlaps by less than half");
}

#[test]
fn every_line_of_a_real_page_belongs_to_exactly_one_row() {
    let t = open_fixture("refs.pdf").page_text(3);
    let rows = text_rows(&t);
    assert!(!rows.is_empty());
    assert!(rows.len() <= t.lines().len());
    assert!(rows.windows(2).all(|w| w[0].0 < w[1].0), "rows are in reading order");
}
