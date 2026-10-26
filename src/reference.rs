//! Reference tables generated from the code: commands, `[general]` settings, the Lua API and its events.
//!
//! Each table lives in a doc between markers (`<!-- generated: NAME -->` … `<!-- end generated -->`).
//! `folio --update-docs` rewrites them, and a test fails when a doc and the code disagree.

use std::path::Path;

use anyhow::{Context, Result};

use crate::{commands, config, keymap, script};

/// Markdown table of all built-in commands with their default bindings.
pub fn commands_md() -> String {
    let mut keys: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
    for (mode, defaults) in [("", keymap::NORMAL_DEFAULTS), ("visual: ", keymap::VISUAL_DEFAULTS)] {
        for (k, c) in defaults {
            let name = c.split_whitespace().next().unwrap_or(c);
            keys.entry(name).or_default().push(format!("{mode}`{}`", k.replace('|', "\\|")));
        }
    }
    let mut out = String::from("| command | argument | default keys | description |\n|---|---|---|---|\n");
    for c in commands::BUILTINS {
        let k = keys.get(c.name).map(|v| v.join(" ")).unwrap_or_default();
        out += &format!("| `{}` | {} | {} | {} |\n", c.name, c.arg.unwrap_or(""), k, c.description);
    }
    out
}

/// The `[general]` settings with their defaults.
pub fn config_md() -> String {
    let defaults = serde_json::to_value(config::General::default()).unwrap_or_default();
    let mut out = String::from("| key | default | |\n|---|---|---|\n");
    for (key, unset, desc) in config::GENERAL_DOCS {
        let default = match defaults.get(key) {
            Some(serde_json::Value::String(s)) => format!("`\"{s}\"`"),
            Some(serde_json::Value::Null) | None => unset.to_string(),
            // Settings are f32; printing them as f32 gives `1.15`, not `1.149999976158142`.
            Some(serde_json::Value::Number(n)) if n.is_f64() => format!("`{}`", n.as_f64().unwrap_or_default() as f32),
            Some(v) => format!("`{v}`"),
        };
        out += &format!("| `{key}` | {default} | {desc} |\n");
    }
    out
}

/// One section of the Lua API.
pub fn lua_md(section: &str) -> String {
    let mut out = String::from("| | |\n|---|---|\n");
    for (_, usage, desc) in script::LUA_API.iter().filter(|(s, _, _)| *s == section) {
        out += &format!("| `{}` | {desc} |\n", usage.replace('|', "\\|"));
    }
    out
}

pub fn hooks_md() -> String {
    let mut out = String::from("| event | argument |\n|---|---|\n");
    for (name, arg) in script::HOOKS {
        out += &format!("| `{name}` | {arg} |\n");
    }
    out
}

/// Every generated block: (file in docs/, block name, contents).
pub fn blocks() -> Vec<(&'static str, String, String)> {
    let mut out = vec![
        ("COMMANDS.md", "commands".to_string(), commands_md()),
        ("CONFIGURATION.md", "general".to_string(), config_md()),
        ("SCRIPTING.md", "events".to_string(), hooks_md()),
    ];
    let mut sections: Vec<&str> = script::LUA_API.iter().map(|(s, _, _)| *s).collect();
    sections.dedup();
    for s in sections {
        out.push(("SCRIPTING.md", format!("lua: {s}"), lua_md(s)));
    }
    out
}

fn markers(name: &str) -> (String, &'static str) {
    (format!("<!-- generated: {name} -->\n"), "<!-- end generated -->")
}

/// The text of a generated block in a doc, if the doc has its markers.
pub fn block_in<'a>(doc: &'a str, name: &str) -> Option<&'a str> {
    let (start, end) = markers(name);
    let from = doc.find(&start)? + start.len();
    let to = from + doc[from..].find(end)?;
    Some(&doc[from..to])
}

/// Rewrite the generated blocks in `dir` (the docs folder). Returns the files that changed.
pub fn update_docs(dir: &Path) -> Result<Vec<String>> {
    let mut changed = Vec::new();
    for (file, name, contents) in blocks() {
        let path = dir.join(file);
        let doc = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let Some(old) = block_in(&doc, &name) else {
            anyhow::bail!("{} has no markers for '{name}'", path.display());
        };
        if old != contents {
            let new = doc.replacen(&format!("{}{old}", markers(&name).0), &format!("{}{contents}", markers(&name).0), 1);
            std::fs::write(&path, new)?;
            changed.push(format!("{file} ({name})"));
        }
    }
    Ok(changed)
}
