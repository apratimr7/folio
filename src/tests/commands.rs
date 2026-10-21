//! The command table against `docs/COMMANDS.md`, so the docs can't drift.

use std::collections::BTreeSet;

use crate::commands::{self, BUILTINS};

const COMMANDS_MD: &str = include_str!("../../docs/COMMANDS.md");

/// Command names from the first column of the table in `docs/COMMANDS.md`.
fn documented() -> BTreeSet<&'static str> {
    COMMANDS_MD.lines().filter_map(|l| l.strip_prefix("| `")).filter_map(|l| l.split_once('`').map(|(name, _)| name)).collect()
}

#[test]
fn every_builtin_is_documented() {
    let docs = documented();
    let missing: Vec<&str> = BUILTINS.iter().map(|c| c.name).filter(|n| !docs.contains(n)).collect();
    assert!(missing.is_empty(), "missing from docs/COMMANDS.md (regenerate with --list-commands): {missing:?}");
}

#[test]
fn every_documented_command_exists() {
    let stale: Vec<&str> = documented().into_iter().filter(|n| commands::builtin(n).is_none()).collect();
    assert!(stale.is_empty(), "docs/COMMANDS.md lists commands that don't exist: {stale:?}");
}

#[test]
fn builtin_names_are_unique() {
    let mut seen = BTreeSet::new();
    for c in BUILTINS {
        assert!(seen.insert(c.name), "{} is defined twice", c.name);
    }
}

#[test]
fn split_separates_name_and_argument() {
    assert_eq!(commands::split("goto_page 12"), ("goto_page", "12"));
    assert_eq!(commands::split("  lua  print(1) ; x  "), ("lua", "print(1) ; x"));
    assert_eq!(commands::split("quit"), ("quit", ""));
    assert_eq!(commands::split(""), ("", ""));
}
