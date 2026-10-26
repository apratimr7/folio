//! `config.toml` parsing, themes and the default key bindings.

use super::IsolatedEnv;
use crate::commands;
use crate::config::{Config, DEFAULT_CONFIG, General, PageMode, Rgba, THEME_NAMES, Theme};
use crate::keymap::{NORMAL_DEFAULTS, VISUAL_DEFAULTS, parse_seq};

fn parse(toml: &str) -> Config {
    toml::from_str(toml).unwrap_or_else(|e| panic!("config should parse: {e}\n{toml}"))
}

#[test]
fn shipped_config_parses_to_the_defaults() {
    let c = parse(DEFAULT_CONFIG);
    assert_eq!(
        format!("{:?}", c.general),
        format!("{:?}", General::default()),
        "config/config.toml and General::default() differ"
    );
}

#[test]
fn shipped_config_has_no_active_overrides() {
    let c = parse(DEFAULT_CONFIG);
    assert_eq!(format!("{:?}", c.theme), format!("{:?}", crate::config::ThemeOverrides::default()));
    assert!(c.keys.values().all(|m| m.is_empty()), "the shipped bindings are all commented out: {:?}", c.keys);
}

#[test]
fn empty_config_is_all_defaults() {
    let c = parse("");
    assert_eq!(format!("{:?}", c.general), format!("{:?}", General::default()));
    assert!(c.keys.is_empty());
}

#[test]
fn every_theme_preset_loads() {
    for name in THEME_NAMES {
        let t = Theme::preset(name);
        assert_eq!(t.name, *name);
        assert!(!t.highlights.is_empty(), "{name} has no highlight colors");
        assert_eq!(t.highlight_color("yellow"), t.highlights[0].1, "{name}: yellow comes first");
    }
    assert!(!Theme::preset("paper").dark);
}

#[test]
fn unknown_theme_falls_back_to_midnight() {
    assert_eq!(Theme::preset("no-such-theme").name, "midnight");
}

#[test]
fn general_overrides_apply() {
    let c = parse("[general]\ntheme = \"paper\"\npage_mode = \"dark\"\nscroll_step = 10\nhint_chars = \"jkl\"\n");
    assert_eq!(c.general.theme, "paper");
    assert_eq!(c.general.page_mode, PageMode::Dark);
    assert_eq!(c.general.scroll_step, 10.0);
    assert_eq!(c.general.hint_chars, "jkl");
    assert_eq!(c.general.zoom_step, General::default().zoom_step, "unset fields keep their defaults");
}

#[test]
fn theme_overrides_apply_on_top_of_the_preset() {
    let c = parse("[theme]\naccent = \"#112233\"\nradius = 3\n[theme.highlight]\nyellow = \"#01020304\"\nred = \"#ff000080\"\n");
    let t = Theme::preset("nord").with_overrides(&c.theme);
    assert_eq!(t.accent, Rgba::rgb(0x11, 0x22, 0x33));
    assert_eq!(t.radius, 3.0);
    assert_eq!(t.highlight_color("yellow"), Rgba::new(1, 2, 3, 4));
    assert_eq!(t.highlight_color("red"), Rgba::new(0xff, 0, 0, 0x80));
    assert_eq!(t.bg, Theme::preset("nord").bg, "colors that aren't overridden come from the preset");
}

#[test]
fn key_bindings_parse_per_mode() {
    let c = parse("[keys.normal]\n\"<Space>t\" = \"toc\"\ns = \"\"\n[keys.visual]\n\"<C-h>\" = \"highlight pink\"\n");
    assert_eq!(c.keys["normal"]["<Space>t"], "toc");
    assert_eq!(c.keys["normal"]["s"], "");
    assert_eq!(c.keys["visual"]["<C-h>"], "highlight pink");
}

#[test]
fn type_error_is_an_error_not_a_panic() {
    let err = toml::from_str::<Config>("[general]\nscroll_step = \"fast\"\n").expect_err("a string scroll_step must fail");
    assert!(err.to_string().contains("scroll_step"), "the error should name the key: {err}");
    assert!(toml::from_str::<Config>("[general]\npage_mode = \"drak\"\n").is_err());
    assert!(toml::from_str::<Config>("not toml at all [").is_err());
}

#[test]
fn load_reports_the_file_on_error() {
    let env = IsolatedEnv::new();
    std::fs::write(env.config_dir().join("config.toml"), "[general]\nstatusbar = 3\n").unwrap();
    let err = Config::load().expect_err("a bad config.toml must fail to load");
    assert!(format!("{err:#}").contains("config.toml"), "error should name the file: {err:#}");
}

#[test]
fn load_without_a_file_gives_defaults() {
    let _env = IsolatedEnv::new();
    let (c, unknown) = Config::load().expect("no config file is fine");
    assert_eq!(format!("{:?}", c.general), format!("{:?}", General::default()));
    assert!(unknown.is_empty());
}

#[test]
fn load_reads_the_file_under_xdg_config_home() {
    let env = IsolatedEnv::new();
    std::fs::write(env.config_dir().join("config.toml"), "[general]\ntheme = \"gruvbox\"\n").unwrap();
    assert_eq!(Config::load().unwrap().0.general.theme, "gruvbox");
}

#[test]
fn load_reports_unknown_settings() {
    let env = IsolatedEnv::new();
    std::fs::write(env.config_dir().join("config.toml"), "[general]\nhint_char = \"jkl\"\nbogus = 1\n").unwrap();
    let (_, unknown) = Config::load().unwrap();
    assert!(unknown.iter().any(|k| k.contains("hint_char")), "typo should be reported: {unknown:?}");
    assert!(unknown.iter().any(|k| k.contains("bogus")), "unknown key should be reported: {unknown:?}");
}

#[test]
fn rgba_parse() {
    assert_eq!(Rgba::parse("#7aa2ff"), Some(Rgba::rgb(0x7a, 0xa2, 0xff)));
    assert_eq!(Rgba::parse(" 7aa2ff80 "), Some(Rgba::new(0x7a, 0xa2, 0xff, 0x80)));
    for bad in ["", "#fff", "#gggggg", "notacolor", "#1234567"] {
        assert_eq!(Rgba::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn default_bindings_parse_and_name_builtin_commands() {
    for (mode, defaults) in [("normal", NORMAL_DEFAULTS), ("visual", VISUAL_DEFAULTS)] {
        for (keys, command) in defaults {
            let seq = parse_seq(keys).unwrap_or_else(|e| panic!("{mode} binding {keys:?} doesn't parse: {e}"));
            assert!(!seq.is_empty(), "{mode} binding {keys:?} is empty");
            let name = commands::split(command).0;
            assert!(commands::builtin(name).is_some(), "{mode} binding {keys:?} names unknown command {name:?}");
        }
    }
}

#[test]
fn default_bindings_have_no_duplicate_keys() {
    for (mode, defaults) in [("normal", NORMAL_DEFAULTS), ("visual", VISUAL_DEFAULTS)] {
        let mut seen = std::collections::HashSet::new();
        for (keys, _) in defaults {
            assert!(seen.insert(parse_seq(keys).unwrap()), "{mode}: {keys:?} is bound twice");
        }
    }
}

#[test]
fn unusable_values_are_reported() {
    let c = parse(
        "[general]\nruler_dim = 1.5\nhint_dim = 0.3\n[theme]\naccent = \"notacolor\"\nbg = \"#101014\"\n[theme.highlight]\nred = \"#ff\"\n",
    );
    let p = c.problems();
    assert_eq!(p.len(), 3, "{p:?}");
    assert!(p[0].contains("ruler_dim") && p[1].contains("theme.accent") && p[2].contains("theme.highlight.red"), "{p:?}");
    assert!(parse(DEFAULT_CONFIG).problems().is_empty());
}

#[test]
fn dim_settings_override_the_theme_defaults() {
    let mut g = General::default();
    assert_eq!((g.ruler_dim_alpha(true), g.ruler_dim_alpha(false)), (40, 18));
    g.ruler_dim = Some(0.0);
    g.hint_dim = Some(1.0);
    assert_eq!((g.ruler_dim_alpha(true), g.hint_dim_alpha(false)), (0, 255));
}
