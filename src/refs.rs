//! "Smart" references: things like `[12]`, `Fig. 3`, `Eq. (2.4)` or `Theorem 1.2`
//! in the running text, and heuristics to find where they are defined.
//! This is what makes jumping around papers without hyperref links possible.

use std::sync::OnceLock;

use regex::Regex;

use crate::doc::PageText;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefKind {
    Citation,
    Figure,
    Table,
    Equation,
    Theorem,
    Section,
}

impl RefKind {
    pub fn name(self) -> &'static str {
        match self {
            RefKind::Citation => "citation",
            RefKind::Figure => "figure",
            RefKind::Table => "table",
            RefKind::Equation => "equation",
            RefKind::Theorem => "theorem",
            RefKind::Section => "section",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub kind: RefKind,
    /// What to look for, e.g. "12", "3.1", "Lemma 2".
    pub label: String,
    /// Char span in the page text.
    pub start: usize,
    pub end: usize,
}

struct Patterns {
    cite: Regex,
    figure: Regex,
    table: Regex,
    equation: Regex,
    bare_eq: Regex,
    theorem: Regex,
    section: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        cite: Regex::new(r"\[(\d{1,4})(?:\s*[,–-]\s*\d{1,4})*\]").unwrap(),
        figure: Regex::new(r"\b(?:Figs?\.|Figures?)\s*(\d+(?:\.\d+)*[a-z]?)").unwrap(),
        table: Regex::new(r"\b(?:Tabs?\.|Tables?)\s*(\d+(?:\.\d+)*|[IVX]+)").unwrap(),
        equation: Regex::new(r"\b(?:Eqs?\.|Equations?|eqs?\.)\s*\(?(\d+(?:\.\d+)*[a-z]?)\)?").unwrap(),
        bare_eq: Regex::new(r"\((\d+\.\d+[a-z]?)\)").unwrap(),
        theorem: Regex::new(
            r"\b(Theorem|Lemma|Proposition|Corollary|Definition|Remark|Example|Conjecture|Assumption|Algorithm|Claim)\s+(\d+(?:\.\d+)*)",
        )
        .unwrap(),
        section: Regex::new(r"(?:\bSections?|\bSecs?\.|§|\bAppendix|\bChapter|\bCh\.)\s*([A-Z]?\d+(?:\.\d+)*|[A-Z]\b)").unwrap(),
    })
}

/// Text of the page with a byte → char index map.
fn page_string(text: &PageText) -> (String, Vec<usize>) {
    let mut s = String::with_capacity(text.chars.len());
    let mut byte_to_char = Vec::with_capacity(text.chars.len());
    for (i, &c) in text.chars.iter().enumerate() {
        let c = if c == '\n' { ' ' } else { c };
        for _ in 0..c.len_utf8() {
            byte_to_char.push(i);
        }
        s.push(c);
    }
    byte_to_char.push(text.chars.len());
    (s, byte_to_char)
}

/// Find every reference on a page.
pub fn detect(text: &PageText) -> Vec<Reference> {
    let p = patterns();
    let (s, b2c) = page_string(text);
    let mut out: Vec<Reference> = Vec::new();
    // A caption or theorem header ("Fig. 1.", "Theorem 2.3:") at the start of a line defines the
    // thing rather than referring to it.
    let is_header = |start: usize, end: usize| {
        let (start, end) = (b2c[start], b2c[end]);
        let line_start = text.chars[..start].iter().rev().find(|c| !matches!(c, ' ' | '\t')).is_none_or(|&c| c == '\n');
        line_start && matches!(text.chars.get(end), Some('.' | ':'))
    };
    let mut push = |kind, label: String, start: usize, end: usize| {
        let (start, end) = (b2c[start], b2c[end]);
        if !out.iter().any(|r| r.start < end && start < r.end) {
            out.push(Reference { kind, label, start, end });
        }
    };
    for c in p.theorem.captures_iter(&s) {
        let m = c.get(0).unwrap();
        if is_header(m.start(), m.end()) {
            continue;
        }
        push(RefKind::Theorem, format!("{} {}", &c[1], &c[2]), m.start(), m.end());
    }
    for c in p.figure.captures_iter(&s) {
        let m = c.get(0).unwrap();
        if is_header(m.start(), m.end()) {
            continue;
        }
        push(RefKind::Figure, c[1].to_string(), m.start(), m.end());
    }
    for c in p.table.captures_iter(&s) {
        let m = c.get(0).unwrap();
        if is_header(m.start(), m.end()) {
            continue;
        }
        push(RefKind::Table, c[1].to_string(), m.start(), m.end());
    }
    for c in p.equation.captures_iter(&s) {
        let m = c.get(0).unwrap();
        push(RefKind::Equation, c[1].to_string(), m.start(), m.end());
    }
    for c in p.section.captures_iter(&s) {
        let m = c.get(0).unwrap();
        push(RefKind::Section, c[1].to_string(), m.start(), m.end());
    }
    for c in p.cite.captures_iter(&s) {
        let m = c.get(0).unwrap();
        push(RefKind::Citation, c[1].to_string(), m.start(), m.end());
    }
    for c in p.bare_eq.captures_iter(&s) {
        let m = c.get(0).unwrap();
        // Skip equation numbers themselves (they sit at the end of a line).
        let after = text.chars.get(b2c[m.end()]).copied();
        if after == Some('\n') || after.is_none() {
            continue;
        }
        push(RefKind::Equation, c[1].to_string(), m.start(), m.end());
    }
    out.sort_by_key(|r| r.start);
    out
}

/// Does this page look like part of a bibliography?
pub fn is_bibliography(text: &PageText) -> bool {
    text.lines().iter().take(6).any(|&(s, e)| {
        let line: String = text.chars[s..e].iter().collect::<String>().to_lowercase();
        let line = line.trim();
        line.starts_with("references") || line.starts_with("bibliography") || line.ends_with("references")
    })
}

/// A heading followed by a page number (`3.5 Generalizations 29`, `2 Strings . . . 7`).
fn is_toc_line(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let Some(last) = words.next_back() else { return false };
    let title_words = words.skip(1).filter(|w| w.chars().any(char::is_alphabetic)).count();
    title_words > 0 && last.len() <= 4 && last.chars().all(|c| c.is_ascii_digit())
}

/// Candidate definitions of a reference on a page: `(start, end, strong)`.
/// A strong match is one that very likely is *the* definition.
pub fn find_definitions(text: &PageText, kind: RefKind, label: &str) -> Vec<(usize, usize, bool)> {
    let mut out = Vec::new();
    let lines = text.lines();
    for &(ls, le) in &lines {
        let line: String = text.chars[ls..le].iter().collect();
        let trimmed = line.trim_start();
        let lead = line.len() - trimmed.len();
        let lead_chars = line[..lead].chars().count();
        match kind {
            RefKind::Citation => {
                let tag = format!("[{label}]");
                if trimmed.starts_with(&tag) {
                    out.push((ls + lead_chars, le, true));
                } else if let Some(after) = trimmed.strip_prefix(label) {
                    // "12. A. Author, …" style bibliographies (weak: could be any numbered list).
                    if after.starts_with(". ") || after.starts_with(' ') && after.trim_start().starts_with(char::is_uppercase) {
                        out.push((ls + lead_chars, le, false));
                    }
                }
            }
            RefKind::Figure | RefKind::Table => {
                let prefixes: &[&str] = if kind == RefKind::Figure {
                    &["Figure", "Fig.", "FIG.", "FIGURE", "Fig"]
                } else {
                    &["Table", "TABLE", "Tab."]
                };
                for pre in prefixes {
                    if let Some(rest) = trimmed.strip_prefix(pre) {
                        let rest = rest.trim_start();
                        if let Some(after) = rest.strip_prefix(label) {
                            let next = after.chars().next();
                            let strong = matches!(next, Some(':' | '.' | '|' | '—' | '-'));
                            if next.is_none_or(|c| !c.is_ascii_digit()) {
                                out.push((ls + lead_chars, le, strong));
                            }
                            break;
                        }
                    }
                }
            }
            RefKind::Equation => {
                let tag = format!("({label})");
                let t = line.trim_end();
                if t.ends_with(&tag) {
                    let end = ls + t.chars().count();
                    out.push((end - tag.chars().count(), end, true));
                }
            }
            RefKind::Theorem => {
                if let Some(after) = trimmed.strip_prefix(label) {
                    let next = after.chars().next();
                    if next.is_none_or(|c| !c.is_ascii_digit()) {
                        out.push((ls + lead_chars, le, matches!(next, Some('.' | ' ' | '(' | ':'))));
                    }
                } else if let Some((word, num)) = label.split_once(' ')
                    && let Some(after) = trimmed.strip_prefix(num)
                    && after.starts_with(char::is_whitespace)
                {
                    // Number-first style (Rudin, Apostol): "2.36 Theorem. …".
                    let after = after.trim_start();
                    let rest = after.strip_prefix(word).or_else(|| after.strip_prefix(word.to_uppercase().as_str()));
                    if let Some(rest) = rest
                        && matches!(rest.chars().next(), Some('.' | ' ' | '(' | ':') | None)
                    {
                        out.push((ls + lead_chars, le, true));
                    }
                }
            }
            RefKind::Section => {
                let l = label.trim_end_matches('.');
                // A table-of-contents line ends with a page number: "3.5 Generalizations 29".
                let strong = !is_toc_line(trimmed);
                if let Some(after) = trimmed.strip_prefix(l) {
                    let after_t = after.trim_start_matches('.');
                    // "3.2 Results" — the label followed by a capitalized title.
                    if after_t.starts_with(' ') && after_t.trim_start().chars().next().is_some_and(char::is_uppercase) {
                        out.push((ls + lead_chars, le, strong));
                    }
                }
                for pre in ["Appendix ", "APPENDIX ", "Chapter ", "CHAPTER "] {
                    if trimmed.strip_prefix(pre).is_some_and(|r| r.starts_with(l)) {
                        out.push((ls + lead_chars, le, strong));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Rect;

    /// A page with one text line per entry, 10 pt per char and 12 pt per line.
    fn page(lines: &[&str]) -> PageText {
        let mut t = PageText::default();
        for (li, line) in lines.iter().enumerate() {
            if li > 0 {
                t.chars.push('\n');
                t.rects.push(Rect::default());
            }
            let y = li as f32 * 12.0;
            for (ci, c) in line.chars().enumerate() {
                let x = ci as f32 * 10.0;
                t.chars.push(c);
                t.rects.push(Rect::new(x, y, x + 10.0, y + 10.0));
            }
        }
        t
    }

    fn line_of(t: &PageText, start: usize) -> String {
        t.chars[start..].iter().take_while(|&&c| c != '\n').collect()
    }

    #[test]
    fn theorem_kind_first() {
        let t = page(&["some text", "Theorem 2.36. Every compact set is closed."]);
        let defs = find_definitions(&t, RefKind::Theorem, "Theorem 2.36");
        assert_eq!(defs.len(), 1);
        assert!(defs[0].2);
    }

    #[test]
    fn theorem_number_first() {
        let t = page(&[
            "By Theorem 2.36 we have",
            "2.36 Theorem. If {K_a} is a collection of compact subsets",
            "3.9 Definition (Cauchy) A sequence",
            "2.361 Theorem. Not this one",
            "2.36 Corollary. Not this kind",
        ]);
        let defs = find_definitions(&t, RefKind::Theorem, "Theorem 2.36");
        assert_eq!(defs.len(), 1);
        assert!(defs[0].2);
        assert!(line_of(&t, defs[0].0).starts_with("2.36 Theorem."));
        let defs = find_definitions(&t, RefKind::Theorem, "Definition 3.9");
        assert_eq!(defs.len(), 1);
        assert!(defs[0].2);
        assert!(find_definitions(&t, RefKind::Theorem, "Lemma 2.36").is_empty());
    }

    #[test]
    fn captions_are_not_references() {
        let t = page(&[
            "Fig. 1. The Penrose diagram of a black hole.",
            "  Table 2: Parameters used",
            "Theorem 3.1. Let M be a manifold.",
            "as shown in Fig. 1, and Table 2 and Theorem 3.1 imply",
            "Figure 4 shows the horizon",
        ]);
        let refs = detect(&t);
        let found: Vec<(RefKind, &str)> = refs.iter().map(|r| (r.kind, r.label.as_str())).collect();
        assert_eq!(
            found,
            vec![(RefKind::Figure, "1"), (RefKind::Table, "2"), (RefKind::Theorem, "Theorem 3.1"), (RefKind::Figure, "4"),]
        );
        // All found references are in running text, not the captions on lines 0-2.
        assert!(refs.iter().all(|r| t.chars[..r.start].iter().filter(|&&c| c == '\n').count() >= 3));
    }

    #[test]
    fn section_toc_line_is_weak() {
        let t = page(&["Contents", "3.4 Black holes 27", "3.5 Generalizations 29", "3.6 Outlook . . . . 31"]);
        let defs = find_definitions(&t, RefKind::Section, "3.5");
        assert_eq!(defs.len(), 1);
        assert!(!defs[0].2);
        let t = page(&["the end of 3.4.", "3.5 Generalizations", "We now generalize"]);
        let defs = find_definitions(&t, RefKind::Section, "3.5");
        assert_eq!(defs.len(), 1);
        assert!(defs[0].2);
        assert!(is_toc_line("3.6 Outlook . . . . 31"));
        assert!(!is_toc_line("3.5 Generalizations"));
    }
}
