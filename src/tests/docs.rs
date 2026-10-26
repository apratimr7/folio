//! The generated reference tables in `docs/` against the code they describe, so the docs can't drift.
//! After changing commands, settings or the Lua API, run `folio --update-docs`.

use std::collections::BTreeSet;
use std::path::Path;

use crate::{config, reference, script};

#[test]
fn generated_tables_are_up_to_date() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    for (file, name, want) in reference::blocks() {
        let doc = std::fs::read_to_string(docs.join(file)).unwrap();
        let got = reference::block_in(&doc, &name).unwrap_or_else(|| panic!("docs/{file} lacks the markers for '{name}'"));
        assert!(got == want, "docs/{file} ({name}) is out of date: run `folio --update-docs`");
    }
}

#[test]
fn general_docs_list_every_setting() {
    let fields: BTreeSet<String> = match serde_json::to_value(config::General::default()).unwrap() {
        serde_json::Value::Object(m) => m.keys().cloned().collect(),
        _ => unreachable!(),
    };
    let documented: BTreeSet<String> = config::GENERAL_DOCS.iter().map(|d| d.0.to_string()).collect();
    assert_eq!(documented, fields, "config::GENERAL_DOCS must list exactly the fields of General");
}

#[test]
fn lua_api_lists_every_function() {
    let s = script::Script::new(|| {}).unwrap();
    let keys =
        s.run_code("local t = {} for k in pairs(folio) do t[#t + 1] = k end table.sort(t) return table.concat(t, ',')").unwrap();
    let defined: BTreeSet<&str> = keys.split(',').collect();
    let documented: BTreeSet<&str> = script::LUA_API
        .iter()
        .map(|(_, usage, _)| usage.trim_start_matches("folio.").split(['(', ' ']).next().unwrap_or(""))
        .collect();
    assert_eq!(documented, defined, "script::LUA_API must list exactly the functions in the folio table");
}

#[test]
fn hooks_are_the_events_the_app_emits() {
    let mut emitted = BTreeSet::new();
    let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let code = std::fs::read_to_string(&path).unwrap();
                for part in code.split("emit(\"").skip(1) {
                    if let Some((name, _)) = part.split_once('"') {
                        emitted.insert(name.to_string());
                    }
                }
            }
        }
    }
    let documented: BTreeSet<String> = script::HOOKS.iter().map(|h| h.0.to_string()).collect();
    assert_eq!(documented, emitted, "script::HOOKS must list exactly the events passed to App::emit");
}
