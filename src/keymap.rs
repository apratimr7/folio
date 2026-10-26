//! Modal, vim-style key handling: multi-key sequences (`gg`), count prefixes
//! (`12j`), and commands that take the next key as an argument (`m{char}`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// A single char (case carries shift) or a named key like `Esc`, `Tab`, `F1`.
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    /// Only meaningful for named keys (`<S-Tab>`).
    pub shift: bool,
}

impl Chord {
    pub fn char(c: char) -> Self {
        let key = if c == ' ' { "Space".to_string() } else { c.to_string() };
        Self { key, ctrl: false, alt: false, shift: false }
    }

    /// The plain character this chord types, if any.
    pub fn as_char(&self) -> Option<char> {
        if self.ctrl || self.alt {
            return None;
        }
        if self.key == "Space" {
            return Some(' ');
        }
        let mut it = self.key.chars();
        match (it.next(), it.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let simple = !self.ctrl && !self.alt && !self.shift && self.key.chars().count() == 1 && self.key != "<";
        if simple {
            return write!(f, "{}", self.key);
        }
        write!(f, "<")?;
        if self.ctrl {
            write!(f, "C-")?;
        }
        if self.alt {
            write!(f, "A-")?;
        }
        if self.shift {
            write!(f, "S-")?;
        }
        let key = if self.key == "<" { "lt" } else { &self.key };
        write!(f, "{key}>")
    }
}

pub fn format_seq(seq: &[Chord]) -> String {
    seq.iter().map(|c| c.to_string()).collect()
}

fn canonical_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let n = match lower.as_str() {
        "esc" | "escape" => "Esc",
        "cr" | "enter" | "return" => "Enter",
        "tab" => "Tab",
        "bs" | "backspace" => "Backspace",
        "del" | "delete" => "Delete",
        "space" => "Space",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "pageup" | "pgup" => "PageUp",
        "pagedown" | "pgdn" => "PageDown",
        "home" => "Home",
        "end" => "End",
        "insert" => "Insert",
        "lt" => "<",
        "gt" => ">",
        "bar" => "|",
        _ => {
            if lower.starts_with('f') && lower[1..].parse::<u8>().is_ok_and(|n| (1..=35).contains(&n)) {
                return lower.to_ascii_uppercase();
            }
            return name.to_string();
        }
    };
    n.to_string()
}

/// Canonical names of non-character keys (plus `F1`…`F35`).
const NAMED_KEYS: &[&str] = &[
    "Esc",
    "Enter",
    "Tab",
    "Backspace",
    "Delete",
    "Space",
    "Up",
    "Down",
    "Left",
    "Right",
    "PageUp",
    "PageDown",
    "Home",
    "End",
    "Insert",
];

fn is_fkey(key: &str) -> bool {
    key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=35).contains(&n))
}

/// Parse `"gg"`, `"<C-d>"`, `"<Space>t"`, `"<S-Tab>"` …
pub fn parse_seq(s: &str) -> Result<Vec<Chord>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' && i + 1 < chars.len() {
            let Some(len) = chars[i + 1..].iter().position(|&c| c == '>') else {
                return Err(format!("unclosed '<' in \"{s}\" (use <lt> for the < key)"));
            };
            let inner: String = chars[i + 1..i + 1 + len].iter().collect();
            if !inner.is_empty() {
                out.push(parse_bracketed(&inner)?);
                i += len + 2;
                continue;
            }
        }
        out.push(Chord::char(c));
        i += 1;
    }
    if out.is_empty() {
        return Err("empty key sequence".into());
    }
    Ok(out)
}

fn parse_bracketed(inner: &str) -> Result<Chord, String> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut rest = inner;
    loop {
        let lower = rest.to_ascii_lowercase();
        if rest.len() > 2 && (lower.starts_with("c-") || lower.starts_with("ctrl-")) {
            ctrl = true;
            rest = &rest[rest.find('-').unwrap() + 1..];
        } else if rest.len() > 2 && (lower.starts_with("a-") || lower.starts_with("m-") || lower.starts_with("alt-")) {
            alt = true;
            rest = &rest[rest.find('-').unwrap() + 1..];
        } else if rest.len() > 2 && (lower.starts_with("s-") || lower.starts_with("shift-")) {
            shift = true;
            rest = &rest[rest.find('-').unwrap() + 1..];
        } else {
            break;
        }
    }
    let mut key = canonical_name(rest);
    if key.chars().count() > 1 && !NAMED_KEYS.contains(&key.as_str()) && !is_fkey(&key) {
        return Err(format!("unknown key <{inner}>"));
    }
    if key.chars().count() == 1 {
        if ctrl || alt {
            // Ctrl/Alt chords are case-insensitive; shift is folded into the case.
            if shift {
                key = key.to_uppercase();
            } else {
                key = key.to_lowercase();
            }
            shift = false;
        } else if shift {
            key = key.to_uppercase();
            shift = false;
        }
    }
    Ok(Chord { key, ctrl, alt, shift })
}

/// Placeholder in a command that is replaced by the next typed character.
pub const CHAR_ARG: &str = "{char}";

#[derive(Clone, Debug, Default)]
pub struct ModeMap {
    bindings: HashMap<Vec<Chord>, String>,
    prefixes: HashSet<Vec<Chord>>,
}

impl ModeMap {
    pub fn bind(&mut self, seq: Vec<Chord>, command: String) {
        if command.is_empty() {
            self.bindings.remove(&seq);
        } else {
            self.bindings.insert(seq, command);
        }
        self.rebuild_prefixes();
    }

    fn rebuild_prefixes(&mut self) {
        self.prefixes.clear();
        for seq in self.bindings.keys() {
            for n in 1..seq.len() {
                self.prefixes.insert(seq[..n].to_vec());
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Vec<Chord>, &String)> {
        self.bindings.iter()
    }

    /// Bindings sorted by command, for the help screen.
    pub fn sorted(&self) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (seq, cmd) in &self.bindings {
            out.entry(cmd.clone()).or_default().push(format_seq(seq));
        }
        for v in out.values_mut() {
            v.sort_by_key(|s| (s.len(), s.clone()));
        }
        out
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Waiting for more keys.
    Pending,
    Run {
        command: String,
        count: Option<u32>,
    },
    Unbound(String),
}

#[derive(Default)]
pub struct Dispatcher {
    pending: Vec<Chord>,
    count: Option<u32>,
    /// A command waiting for its `{char}` argument.
    awaiting_char: Option<(String, Option<u32>)>,
    since: Option<Instant>,
}

impl Dispatcher {
    pub fn reset(&mut self) {
        self.pending.clear();
        self.count = None;
        self.awaiting_char = None;
        self.since = None;
    }

    /// What's been typed so far, for the status bar.
    pub fn pending_display(&self) -> String {
        let mut s = String::new();
        if let Some(c) = self.count {
            s += &c.to_string();
        }
        s += &format_seq(&self.pending);
        if let Some((cmd, _)) = &self.awaiting_char {
            s += &format!(" {}…", cmd.split_whitespace().next().unwrap_or(""));
        }
        s
    }

    pub fn feed(&mut self, map: &ModeMap, chord: Chord) -> Outcome {
        if let Some((cmd, count)) = self.awaiting_char.take() {
            self.reset();
            if chord.key == "Esc" {
                return Outcome::Pending;
            }
            return match chord.as_char() {
                Some(c) => Outcome::Run { command: cmd.replace(CHAR_ARG, &c.to_string()), count },
                None => Outcome::Unbound(chord.to_string()),
            };
        }
        if self.pending.is_empty() {
            if let Some(d) = chord.as_char().and_then(|c| c.to_digit(10)) {
                let single = vec![chord.clone()];
                if self.count.is_some() || (d != 0 && !map.bindings.contains_key(&single)) {
                    self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d));
                    self.since = Some(Instant::now());
                    return Outcome::Pending;
                }
            }
            if chord.key == "Esc" && self.count.is_some() {
                self.reset();
                return Outcome::Pending;
            }
        }
        self.pending.push(chord);
        self.since = Some(Instant::now());
        let exact = map.bindings.get(&self.pending).cloned();
        let longer = map.prefixes.contains(&self.pending);
        match (exact, longer) {
            (Some(cmd), false) => self.fire(cmd),
            (_, true) => Outcome::Pending,
            (None, false) => {
                let seq = format_seq(&self.pending);
                self.reset();
                Outcome::Unbound(seq)
            }
        }
    }

    /// Resolve an ambiguous sequence (`g` when both `g` and `gg` exist) after a timeout.
    pub fn tick(&mut self, map: &ModeMap, timeout: Duration) -> Option<Outcome> {
        let since = self.since?;
        if self.awaiting_char.is_some() || self.pending.is_empty() || since.elapsed() < timeout {
            return None;
        }
        // A prefix that isn't bound itself (`o` before `oo`/`of`) waits for the next key, with the key menu up.
        let cmd = map.bindings.get(&self.pending).cloned()?;
        Some(self.fire(cmd))
    }

    /// Time until `tick` might fire, so the UI can schedule a repaint.
    pub fn deadline(&self, map: &ModeMap, timeout: Duration) -> Option<Duration> {
        if self.pending.is_empty() || self.awaiting_char.is_some() || !map.bindings.contains_key(&self.pending) {
            return None;
        }
        Some(timeout.saturating_sub(self.since?.elapsed()))
    }

    /// How long the current key prefix has been waiting, if one is.
    pub fn pending_for(&self) -> Option<Duration> {
        (!self.pending.is_empty() && self.awaiting_char.is_none()).then(|| self.since.map(|t| t.elapsed())).flatten()
    }

    /// The bindings that continue the typed prefix: (the rest of the keys, command), sorted by keys.
    pub fn continuations(&self, map: &ModeMap) -> Vec<(String, String)> {
        let n = self.pending.len();
        let mut out: Vec<(String, String)> = map
            .bindings
            .iter()
            .filter(|(seq, _)| seq.len() > n && seq[..n] == self.pending[..])
            .map(|(seq, cmd)| (format_seq(&seq[n..]), cmd.clone()))
            .collect();
        out.sort();
        out
    }

    fn fire(&mut self, cmd: String) -> Outcome {
        let count = self.count;
        self.reset();
        if cmd.contains(CHAR_ARG) {
            self.awaiting_char = Some((cmd, count));
            return Outcome::Pending;
        }
        Outcome::Run { command: cmd, count }
    }
}

pub const NORMAL_DEFAULTS: &[(&str, &str)] = &[
    ("j", "move_down"),
    ("<Down>", "move_down"),
    ("k", "move_up"),
    ("<Up>", "move_up"),
    ("h", "move_left"),
    ("<Left>", "move_left"),
    ("l", "move_right"),
    ("<Right>", "move_right"),
    ("<C-d>", "half_page_down"),
    ("<C-u>", "half_page_up"),
    ("<Space>", "screen_down"),
    ("<C-f>", "screen_down"),
    ("<PageDown>", "screen_down"),
    ("<S-Space>", "screen_up"),
    ("<C-b>", "screen_up"),
    ("<PageUp>", "screen_up"),
    ("J", "next_page"),
    ("K", "prev_page"),
    ("gg", "goto_start"),
    ("<Home>", "goto_start"),
    ("G", "goto_end"),
    ("<End>", "goto_end"),
    ("gp", "goto_page_prompt"),
    ("+", "zoom_in"),
    ("=", "zoom_in"),
    ("<C-=>", "zoom_in"),
    ("-", "zoom_out"),
    ("<C-->", "zoom_out"),
    ("w", "fit_width"),
    ("W", "fit_page"),
    ("z0", "actual_size"),
    ("zz", "center_line"),
    ("zs", "toggle_statusbar"),
    ("/", "search"),
    ("?", "search_backward"),
    ("n", "next_match"),
    ("N", "prev_match"),
    ("*", "search_selection"),
    ("<Esc>", "clear"),
    (":", "command_palette"),
    ("<C-p>", "command_palette"),
    ("t", "toc"),
    ("x", "toggle_sidebar"),
    ("oo", "open_dialog"),
    ("of", "open_file"),
    ("or", "recent_files"),
    ("O", "recent_files"),
    ("<C-6>", "alternate_file"),
    ("m", "set_mark {char}"),
    ("'", "goto_mark {char}"),
    ("`", "goto_mark {char}"),
    ("<C-o>", "jump_back"),
    ("<Backspace>", "jump_back"),
    ("<C-i>", "jump_forward"),
    ("<Tab>", "jump_forward"),
    ("f", "follow_link"),
    ("F", "peek_link"),
    ("s", "smart_jump"),
    ("S", "smart_peek"),
    ("i", "toggle_dark"),
    ("I", "toggle_custom_colors"),
    ("T", "cycle_theme"),
    ("b", "add_bookmark"),
    ("B", "bookmarks"),
    ("v", "visual"),
    ("y", "yank"),
    ("Y", "yank_page"),
    ("H", "highlight"),
    ("gh", "highlights"),
    ("r", "toggle_ruler"),
    ("R", "reload"),
    (">", "rotate"),
    ("<", "rotate -90"),
    ("ZZ", "quit"),
    ("<C-q>", "quit"),
    ("<F11>", "fullscreen"),
    ("<F1>", "help"),
    ("g?", "help"),
    ("ge", "export_highlights"),
    ("gE", "save_highlights"),
    ("gm", "marks"),
    ("<C-n>", "toggle_text_panel"),
];

pub const VISUAL_DEFAULTS: &[(&str, &str)] = &[
    ("w", "sel_word_next"),
    ("e", "sel_word_next"),
    ("l", "sel_char_next"),
    ("<Right>", "sel_char_next"),
    ("b", "sel_word_prev"),
    ("h", "sel_char_prev"),
    ("<Left>", "sel_char_prev"),
    ("j", "sel_line_next"),
    ("<Down>", "sel_line_next"),
    ("k", "sel_line_prev"),
    ("<Up>", "sel_line_prev"),
    ("$", "sel_line_end"),
    ("0", "sel_line_start"),
    (")", "sel_sentence_next"),
    ("o", "sel_swap"),
    ("y", "yank"),
    ("<C-c>", "yank"),
    ("H", "highlight"),
    ("a", "annotate"),
    ("*", "search_selection"),
    ("<Esc>", "exit_visual"),
    ("v", "exit_visual"),
    (":", "command_palette"),
    ("<C-d>", "half_page_down"),
    ("<C-u>", "half_page_up"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> ModeMap {
        let mut m = ModeMap::default();
        for (k, v) in pairs {
            m.bind(parse_seq(k).unwrap(), v.to_string());
        }
        m
    }

    #[test]
    fn parses_sequences() {
        assert_eq!(parse_seq("gg").unwrap().len(), 2);
        let c = &parse_seq("<C-D>").unwrap()[0];
        assert!(c.ctrl && c.key == "d");
        assert_eq!(parse_seq("<S-Tab>").unwrap()[0].to_string(), "<S-Tab>");
        assert_eq!(parse_seq("<Space>t").unwrap()[1].key, "t");
        assert_eq!(parse_seq("<lt>").unwrap()[0].key, "<");
        assert_eq!(parse_seq("<S-j>").unwrap()[0].key, "J");
        assert_eq!(parse_seq("<F12>").unwrap()[0].key, "F12");
        assert_eq!(parse_seq("<C-->").unwrap()[0].key, "-");
        assert_eq!(parse_seq("<").unwrap()[0].key, "<");
        assert_eq!(parse_seq("<pagedown>").unwrap()[0].key, "PageDown");
    }

    #[test]
    fn rejects_malformed_keys() {
        assert!(parse_seq("<C-").is_err());
        assert!(parse_seq("<Bad").is_err());
        assert!(parse_seq("g<Esc").is_err());
        assert!(parse_seq("<Foo>").is_err());
        assert!(parse_seq("<C-Foo>").is_err());
        assert!(parse_seq("<F99>").is_err());
        assert!(parse_seq("").is_err());
    }

    #[test]
    fn default_bindings_parse() {
        for (k, _) in NORMAL_DEFAULTS.iter().chain(VISUAL_DEFAULTS) {
            assert!(parse_seq(k).is_ok(), "{k}");
        }
    }

    #[test]
    fn counts_and_sequences() {
        let m = map(&[("j", "down"), ("gg", "top"), ("g?", "help"), ("m", "set_mark {char}")]);
        let mut d = Dispatcher::default();
        assert_eq!(d.feed(&m, Chord::char('1')), Outcome::Pending);
        assert_eq!(d.feed(&m, Chord::char('2')), Outcome::Pending);
        assert_eq!(d.feed(&m, Chord::char('j')), Outcome::Run { command: "down".into(), count: Some(12) });
        assert_eq!(d.feed(&m, Chord::char('g')), Outcome::Pending);
        assert_eq!(d.feed(&m, Chord::char('g')), Outcome::Run { command: "top".into(), count: None });
        assert_eq!(d.feed(&m, Chord::char('m')), Outcome::Pending);
        assert_eq!(d.feed(&m, Chord::char('a')), Outcome::Run { command: "set_mark a".into(), count: None });
        assert!(matches!(d.feed(&m, Chord::char('q')), Outcome::Unbound(_)));
    }

    #[test]
    fn zero_extends_count_even_when_bound() {
        let m = map(&[("0", "line_start"), ("j", "down")]);
        let mut d = Dispatcher::default();
        assert_eq!(d.feed(&m, Chord::char('0')), Outcome::Run { command: "line_start".into(), count: None });
        d.feed(&m, Chord::char('1'));
        d.feed(&m, Chord::char('0'));
        assert_eq!(d.feed(&m, Chord::char('j')), Outcome::Run { command: "down".into(), count: Some(10) });
    }

    #[test]
    fn ambiguous_prefix_times_out() {
        let m = map(&[("g", "one"), ("gg", "two")]);
        let mut d = Dispatcher::default();
        assert_eq!(d.feed(&m, Chord::char('g')), Outcome::Pending);
        assert_eq!(d.tick(&m, Duration::ZERO), Some(Outcome::Run { command: "one".into(), count: None }));
    }

    #[test]
    fn unbound_prefix_waits_and_lists_continuations() {
        let m = map(&[("oo", "open_dialog"), ("of", "open_file"), ("j", "move_down")]);
        let mut d = Dispatcher::default();
        assert_eq!(d.feed(&m, Chord::char('o')), Outcome::Pending);
        assert_eq!(d.tick(&m, Duration::ZERO), None, "an unbound prefix doesn't time out");
        assert_eq!(d.deadline(&m, Duration::ZERO), None, "and needs no repaint timer");
        assert!(d.pending_for().is_some());
        let got = d.continuations(&m);
        assert_eq!(got, [("f".to_string(), "open_file".to_string()), ("o".to_string(), "open_dialog".to_string())]);
        assert_eq!(d.feed(&m, Chord::char('f')), Outcome::Run { command: "open_file".into(), count: None });
        assert!(d.pending_for().is_none());
    }
}
