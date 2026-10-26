//! User configuration: `~/.config/folio/config.toml`.
//!
//! Every field has a default, so a config file only needs to contain overrides.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PageMode {
    /// Pages exactly as authored.
    Normal,
    /// Lightness-inverted, hue-preserving (figures keep their colors).
    Dark,
    /// Black→`page_fg`, white→`page_bg` from the theme.
    Custom,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: String,
    pub page_mode: PageMode,
    pub statusbar: bool,
    /// Scroll step for `move_down`/`move_up`, in screen points.
    pub scroll_step: f32,
    pub zoom_step: f32,
    /// Gap between pages, in screen points.
    pub page_gap: f32,
    pub auto_reload: bool,
    pub restore_position: bool,
    pub render_threads: usize,
    /// Characters used to build link-hint labels.
    pub hint_chars: String,
    pub ui_font_size: f32,
    /// Optional path to a TTF/OTF used for the interface.
    pub ui_font: Option<PathBuf>,
    /// Delay (ms) before an ambiguous key sequence (e.g. `g` vs `gg`) fires.
    pub key_timeout_ms: u64,
    /// Keep the page sharp: textures are rendered at this multiple of the screen density.
    pub render_oversample: f32,
    /// Animate scrolling.
    pub smooth_scroll: bool,
    /// How dark the reading ruler makes what's above it (0 = off, 1 = black). Unset: a theme-dependent default.
    pub ruler_dim: Option<f32>,
    /// How dark the page gets around the row whose words are being labeled (`v`). Unset: theme-dependent.
    pub hint_dim: Option<f32>,
}

impl General {
    /// The ruler's "already read" dim as an alpha over black.
    pub fn ruler_dim_alpha(&self, dark: bool) -> u8 {
        dim_alpha(self.ruler_dim, if dark { 40 } else { 18 })
    }

    /// The dim around the labeled row in visual-mode hints, as an alpha over black.
    pub fn hint_dim_alpha(&self, dark: bool) -> u8 {
        dim_alpha(self.hint_dim, if dark { 90 } else { 60 })
    }
}

fn dim_alpha(setting: Option<f32>, default: u8) -> u8 {
    setting.map(|f| (f.clamp(0.0, 1.0) * 255.0).round() as u8).unwrap_or(default)
}

impl Default for General {
    fn default() -> Self {
        Self {
            theme: "midnight".into(),
            page_mode: PageMode::Normal,
            statusbar: true,
            scroll_step: 70.0,
            zoom_step: 1.15,
            page_gap: 14.0,
            auto_reload: true,
            restore_position: true,
            render_threads: 2,
            hint_chars: "asdfghjklqwertyuiop".into(),
            ui_font_size: 14.0,
            ui_font: None,
            key_timeout_ms: 600,
            render_oversample: 1.0,
            smooth_scroll: true,
            ruler_dim: None,
            hint_dim: None,
        }
    }
}

/// Theme colors as `#rrggbb` / `#rrggbbaa` strings. Unset fields come from the preset.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ThemeOverrides {
    pub bg: Option<String>,
    pub surface: Option<String>,
    pub surface_alt: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub text_dim: Option<String>,
    pub accent: Option<String>,
    pub accent_text: Option<String>,
    pub search_match: Option<String>,
    pub search_current: Option<String>,
    pub selection: Option<String>,
    pub ruler: Option<String>,
    pub hint_bg: Option<String>,
    pub hint_text: Option<String>,
    pub page_bg: Option<String>,
    pub page_fg: Option<String>,
    pub page_shadow: Option<String>,
    pub radius: Option<f32>,
    pub highlight: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub theme: ThemeOverrides,
    /// `mode -> keys -> command`. An empty command unbinds a default.
    pub keys: BTreeMap<String, BTreeMap<String, String>>,
}

/// The program's name, from `Cargo.toml`. Folders, socket names and the window use it, so renaming the package
/// renames those too (see "Renaming" in `CLAUDE.md`).
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

/// The user's home directory (`HOME`, or `USERPROFILE` on Windows).
pub fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// A Windows known folder from the environment (`APPDATA`, `LOCALAPPDATA`), if set.
fn windows_dir(var: &str) -> Option<PathBuf> {
    if cfg!(windows) { std::env::var_os(var).map(PathBuf::from) } else { None }
}

/// `$XDG_CONFIG_HOME/folio`, else `~/.config/folio` (on macOS too, like most keyboard-driven tools), or
/// `%APPDATA%\folio` on Windows.
pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| windows_dir("APPDATA"))
        .unwrap_or_else(|| home().join(".config"))
        .join(APP_NAME)
}

/// `$XDG_DATA_HOME/folio`, else `~/.local/share/folio`, or `%LOCALAPPDATA%\folio` on Windows.
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| windows_dir("LOCALAPPDATA"))
        .unwrap_or_else(|| home().join(".local/share"))
        .join(APP_NAME)
}

impl Config {
    /// The config, plus the paths of settings it doesn't know (`general.hint_char`), to warn about.
    pub fn load() -> Result<(Self, Vec<String>)> {
        let path = config_dir().join("config.toml");
        if !path.exists() {
            return Ok((Self::default(), Vec::new()));
        }
        let text = std::fs::read_to_string(&path)?;
        let mut unknown = Vec::new();
        let de = toml::Deserializer::parse(&text).with_context(|| format!("in {}", path.display()))?;
        let cfg =
            serde_ignored::deserialize(de, |p| unknown.push(p.to_string())).with_context(|| format!("in {}", path.display()))?;
        Ok((cfg, unknown))
    }
}

impl Config {
    /// Values that parse but can't be used (they fall back to defaults), to warn about.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (key, v) in [("ruler_dim", self.general.ruler_dim), ("hint_dim", self.general.hint_dim)] {
            if let Some(f) = v.filter(|f| !(0.0..=1.0).contains(f)) {
                out.push(format!("general.{key} = {f} (use 0 to 1)"));
            }
        }
        let t = &self.theme;
        let colors = [
            ("bg", &t.bg),
            ("surface", &t.surface),
            ("surface_alt", &t.surface_alt),
            ("border", &t.border),
            ("text", &t.text),
            ("text_dim", &t.text_dim),
            ("accent", &t.accent),
            ("accent_text", &t.accent_text),
            ("search_match", &t.search_match),
            ("search_current", &t.search_current),
            ("selection", &t.selection),
            ("ruler", &t.ruler),
            ("hint_bg", &t.hint_bg),
            ("hint_text", &t.hint_text),
            ("page_bg", &t.page_bg),
            ("page_fg", &t.page_fg),
            ("page_shadow", &t.page_shadow),
        ];
        for (name, v) in colors {
            if let Some(v) = v.as_deref().filter(|v| Rgba::parse(v).is_none()) {
                out.push(format!("theme.{name} = \"{v}\" (use #rrggbb or #rrggbbaa)"));
            }
        }
        for (name, v) in t.highlight.iter().filter(|(_, v)| Rgba::parse(v).is_none()) {
            out.push(format!("theme.highlight.{name} = \"{v}\" (use #rrggbb or #rrggbbaa)"));
        }
        out
    }
}

pub const DEFAULT_CONFIG: &str = include_str!("../config/config.toml");
pub const DEFAULT_INIT_LUA: &str = include_str!("../config/init.lua");

// ---------------------------------------------------------------------------
// Themes

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().trim_start_matches('#');
        let v = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        match h.len() {
            6 => Some(Self([v(0)?, v(2)?, v(4)?, 255])),
            8 => Some(Self([v(0)?, v(2)?, v(4)?, v(6)?])),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    pub bg: Rgba,
    pub surface: Rgba,
    pub surface_alt: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub text_dim: Rgba,
    pub accent: Rgba,
    pub accent_text: Rgba,
    pub search_match: Rgba,
    pub search_current: Rgba,
    pub selection: Rgba,
    pub ruler: Rgba,
    pub hint_bg: Rgba,
    pub hint_text: Rgba,
    pub page_bg: Rgba,
    pub page_fg: Rgba,
    pub page_shadow: Rgba,
    pub radius: f32,
    /// Named highlight colors, in cycling order.
    pub highlights: Vec<(String, Rgba)>,
}

pub const THEME_NAMES: &[&str] = &["midnight", "paper", "nord", "gruvbox"];

impl Theme {
    pub fn preset(name: &str) -> Theme {
        let hl = |v: &[(&str, Rgba)]| v.iter().map(|(n, c)| (n.to_string(), *c)).collect::<Vec<_>>();
        match name {
            "paper" => Theme {
                name: "paper".into(),
                dark: false,
                bg: Rgba::rgb(0xe9, 0xe6, 0xdf),
                surface: Rgba::rgb(0xfb, 0xfa, 0xf7),
                surface_alt: Rgba::rgb(0xef, 0xec, 0xe5),
                border: Rgba::rgb(0xd6, 0xd1, 0xc6),
                text: Rgba::rgb(0x2b, 0x2a, 0x27),
                text_dim: Rgba::rgb(0x7d, 0x78, 0x6d),
                accent: Rgba::rgb(0xb4, 0x4a, 0x1f),
                accent_text: Rgba::rgb(0xff, 0xff, 0xff),
                search_match: Rgba::new(0xff, 0xc4, 0x00, 0x60),
                search_current: Rgba::new(0xff, 0x6a, 0x00, 0x80),
                selection: Rgba::new(0x2f, 0x6f, 0xeb, 0x50),
                ruler: Rgba::new(0xb4, 0x4a, 0x1f, 0x30),
                hint_bg: Rgba::rgb(0x2b, 0x2a, 0x27),
                hint_text: Rgba::rgb(0xff, 0xd8, 0x6b),
                page_bg: Rgba::rgb(0xf8, 0xf1, 0xe3),
                page_fg: Rgba::rgb(0x2b, 0x2a, 0x27),
                page_shadow: Rgba::new(0, 0, 0, 0x28),
                radius: 8.0,
                highlights: hl(&[
                    ("yellow", Rgba::new(0xff, 0xd8, 0x30, 0x70)),
                    ("green", Rgba::new(0x5c, 0xd6, 0x6a, 0x60)),
                    ("blue", Rgba::new(0x4a, 0x9c, 0xff, 0x55)),
                    ("pink", Rgba::new(0xff, 0x6b, 0xb5, 0x55)),
                    ("purple", Rgba::new(0xa8, 0x7c, 0xff, 0x55)),
                ]),
            },
            "nord" => Theme {
                name: "nord".into(),
                dark: true,
                bg: Rgba::rgb(0x24, 0x29, 0x33),
                surface: Rgba::rgb(0x2e, 0x34, 0x40),
                surface_alt: Rgba::rgb(0x3b, 0x42, 0x52),
                border: Rgba::rgb(0x43, 0x4c, 0x5e),
                text: Rgba::rgb(0xec, 0xef, 0xf4),
                text_dim: Rgba::rgb(0x8f, 0x9b, 0xb3),
                accent: Rgba::rgb(0x88, 0xc0, 0xd0),
                accent_text: Rgba::rgb(0x2e, 0x34, 0x40),
                search_match: Rgba::new(0xeb, 0xcb, 0x8b, 0x60),
                search_current: Rgba::new(0xd0, 0x87, 0x70, 0x90),
                selection: Rgba::new(0x88, 0xc0, 0xd0, 0x50),
                ruler: Rgba::new(0x88, 0xc0, 0xd0, 0x30),
                hint_bg: Rgba::rgb(0x88, 0xc0, 0xd0),
                hint_text: Rgba::rgb(0x2e, 0x34, 0x40),
                page_bg: Rgba::rgb(0x2e, 0x34, 0x40),
                page_fg: Rgba::rgb(0xd8, 0xde, 0xe9),
                page_shadow: Rgba::new(0, 0, 0, 0x60),
                radius: 8.0,
                highlights: hl(&[
                    ("yellow", Rgba::new(0xeb, 0xcb, 0x8b, 0x70)),
                    ("green", Rgba::new(0xa3, 0xbe, 0x8c, 0x70)),
                    ("blue", Rgba::new(0x81, 0xa1, 0xc1, 0x70)),
                    ("pink", Rgba::new(0xbf, 0x61, 0x6a, 0x60)),
                    ("purple", Rgba::new(0xb4, 0x8e, 0xad, 0x70)),
                ]),
            },
            "gruvbox" => Theme {
                name: "gruvbox".into(),
                dark: true,
                bg: Rgba::rgb(0x1d, 0x20, 0x21),
                surface: Rgba::rgb(0x28, 0x28, 0x28),
                surface_alt: Rgba::rgb(0x3c, 0x38, 0x36),
                border: Rgba::rgb(0x50, 0x49, 0x45),
                text: Rgba::rgb(0xeb, 0xdb, 0xb2),
                text_dim: Rgba::rgb(0xa8, 0x99, 0x84),
                accent: Rgba::rgb(0xfa, 0xbd, 0x2f),
                accent_text: Rgba::rgb(0x28, 0x28, 0x28),
                search_match: Rgba::new(0xfa, 0xbd, 0x2f, 0x60),
                search_current: Rgba::new(0xfe, 0x80, 0x19, 0x90),
                selection: Rgba::new(0x83, 0xa5, 0x98, 0x60),
                ruler: Rgba::new(0xfa, 0xbd, 0x2f, 0x28),
                hint_bg: Rgba::rgb(0xfa, 0xbd, 0x2f),
                hint_text: Rgba::rgb(0x28, 0x28, 0x28),
                page_bg: Rgba::rgb(0x28, 0x28, 0x28),
                page_fg: Rgba::rgb(0xeb, 0xdb, 0xb2),
                page_shadow: Rgba::new(0, 0, 0, 0x70),
                radius: 6.0,
                highlights: hl(&[
                    ("yellow", Rgba::new(0xfa, 0xbd, 0x2f, 0x70)),
                    ("green", Rgba::new(0xb8, 0xbb, 0x26, 0x60)),
                    ("blue", Rgba::new(0x83, 0xa5, 0x98, 0x70)),
                    ("pink", Rgba::new(0xfb, 0x49, 0x34, 0x60)),
                    ("purple", Rgba::new(0xd3, 0x86, 0x9b, 0x70)),
                ]),
            },
            // "midnight", the default.
            _ => Theme {
                name: "midnight".into(),
                dark: true,
                bg: Rgba::rgb(0x12, 0x14, 0x1a),
                surface: Rgba::rgb(0x1b, 0x1e, 0x27),
                surface_alt: Rgba::rgb(0x25, 0x29, 0x35),
                border: Rgba::rgb(0x31, 0x36, 0x45),
                text: Rgba::rgb(0xe4, 0xe7, 0xef),
                text_dim: Rgba::rgb(0x86, 0x8d, 0xa1),
                accent: Rgba::rgb(0x7a, 0xa2, 0xff),
                accent_text: Rgba::rgb(0x0f, 0x12, 0x1a),
                search_match: Rgba::new(0xff, 0xd0, 0x4d, 0x58),
                search_current: Rgba::new(0xff, 0x8a, 0x3d, 0x90),
                selection: Rgba::new(0x7a, 0xa2, 0xff, 0x50),
                ruler: Rgba::new(0x7a, 0xa2, 0xff, 0x2a),
                hint_bg: Rgba::rgb(0xff, 0xd0, 0x4d),
                hint_text: Rgba::rgb(0x12, 0x14, 0x1a),
                page_bg: Rgba::rgb(0x1b, 0x1e, 0x27),
                page_fg: Rgba::rgb(0xd5, 0xd9, 0xe3),
                page_shadow: Rgba::new(0, 0, 0, 0x70),
                radius: 10.0,
                highlights: hl(&[
                    ("yellow", Rgba::new(0xff, 0xd0, 0x4d, 0x68)),
                    ("green", Rgba::new(0x5f, 0xd3, 0x8d, 0x5c)),
                    ("blue", Rgba::new(0x5a, 0x9b, 0xff, 0x5c)),
                    ("pink", Rgba::new(0xff, 0x6e, 0xb4, 0x58)),
                    ("purple", Rgba::new(0xb0, 0x8b, 0xff, 0x5c)),
                ]),
            },
        }
    }

    pub fn with_overrides(mut self, o: &ThemeOverrides) -> Theme {
        let set = |dst: &mut Rgba, src: &Option<String>| {
            if let Some(c) = src.as_deref().and_then(Rgba::parse) {
                *dst = c;
            }
        };
        set(&mut self.bg, &o.bg);
        set(&mut self.surface, &o.surface);
        set(&mut self.surface_alt, &o.surface_alt);
        set(&mut self.border, &o.border);
        set(&mut self.text, &o.text);
        set(&mut self.text_dim, &o.text_dim);
        set(&mut self.accent, &o.accent);
        set(&mut self.accent_text, &o.accent_text);
        set(&mut self.search_match, &o.search_match);
        set(&mut self.search_current, &o.search_current);
        set(&mut self.selection, &o.selection);
        set(&mut self.ruler, &o.ruler);
        set(&mut self.hint_bg, &o.hint_bg);
        set(&mut self.hint_text, &o.hint_text);
        set(&mut self.page_bg, &o.page_bg);
        set(&mut self.page_fg, &o.page_fg);
        set(&mut self.page_shadow, &o.page_shadow);
        if let Some(r) = o.radius {
            self.radius = r;
        }
        for (name, c) in &o.highlight {
            if let Some(c) = Rgba::parse(c) {
                match self.highlights.iter_mut().find(|(n, _)| n == name) {
                    Some(slot) => slot.1 = c,
                    None => self.highlights.push((name.clone(), c)),
                }
            }
        }
        self
    }

    pub fn highlight_color(&self, name: &str) -> Rgba {
        self.highlights
            .iter()
            .find(|(n, _)| n == name)
            .or(self.highlights.first())
            .map(|(_, c)| *c)
            .unwrap_or(Rgba::new(255, 220, 0, 100))
    }
}
