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
    let mut push = |kind, label: String, start: usize, end: usize| {
        let (start, end) = (b2c[start], b2c[end]);
        if !out.iter().any(|r| r.start < end && start < r.end) {
            out.push(Reference { kind, label, start, end });
        }
    };
    for c in p.theorem.captures_iter(&s) {
        let m = c.get(0).unwrap();
        push(RefKind::Theorem, format!("{} {}", &c[1], &c[2]), m.start(), m.end());
    }
    for c in p.figure.captures_iter(&s) {
        let m = c.get(0).unwrap();
        push(RefKind::Figure, c[1].to_string(), m.start(), m.end());
    }
    for c in p.table.captures_iter(&s) {
        let m = c.get(0).unwrap();
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
                }
            }
            RefKind::Section => {
                let l = label.trim_end_matches('.');
                if let Some(after) = trimmed.strip_prefix(l) {
                    let after_t = after.trim_start_matches('.');
                    // "3.2 Results" — the label followed by a capitalized title.
                    if after_t.starts_with(' ') && after_t.trim_start().chars().next().is_some_and(char::is_uppercase) {
                        out.push((ls + lead_chars, le, true));
                    }
                }
                for pre in ["Appendix ", "APPENDIX ", "Chapter ", "CHAPTER "] {
                    if trimmed.strip_prefix(pre).is_some_and(|r| r.starts_with(l)) {
                        out.push((ls + lead_chars, le, true));
                    }
                }
            }
        }
    }
    out
}
