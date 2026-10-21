//! The Lua API (`script.rs`): a `Script` with a test snapshot and a real document, checked through Lua.

use std::rc::Rc;
use std::time::Duration;

use super::{IsolatedEnv, TempDir, open_fixture};
use crate::config::DEFAULT_INIT_LUA;
use crate::doc::Backend;
use crate::script::{Effect, Level, Script, Snapshot};

fn script() -> Script {
    Script::new(|| {}).expect("create Lua state")
}

/// A script on `refs.pdf` (7 pages), currently on page index 3.
fn script_with_doc() -> Script {
    let s = script();
    let doc = open_fixture("refs.pdf");
    s.set_snapshot(Snapshot {
        path: "/papers/refs.pdf".into(),
        title: "Folio Test Paper".into(),
        page: 3,
        page_count: doc.page_count(),
        zoom: 1.5,
        mode: "normal".into(),
        selection: Some("selected words".into()),
        search: None,
        page_mode: "dark".into(),
        theme: "nord".into(),
        outline: Rc::new(doc.outline()),
        highlights: vec![(4, "E = mc2".into(), "yellow".into(), "famous".into())],
    });
    s.set_doc(Some(Rc::new(doc)));
    s
}

fn eval(s: &Script, code: &str) -> String {
    s.run_code(code).unwrap_or_else(|e| panic!("lua error in {code:?}: {e}"))
}

/// Effects as short strings, for readable assertions.
fn effects(s: &Script) -> Vec<String> {
    s.take_effects()
        .into_iter()
        .map(|e| match e {
            Effect::Exec(c) => format!("exec {c}"),
            Effect::Notify(m, l) => format!("notify {l:?} {m}"),
            Effect::Map { mode, keys, command } => format!("map {mode} {keys} {command}"),
            Effect::Show { title, text } => format!("show {title}: {text}"),
            Effect::Append(t) => format!("append {t}"),
            Effect::Prompt { label, initial, .. } => format!("prompt {label} [{initial}]"),
            Effect::Pick { title, items, .. } => format!("pick {title} {}", items.join(",")),
            Effect::Clipboard(t) => format!("clipboard {t}"),
            Effect::Open(p) => format!("open {}", p.display()),
            Effect::SetOption(k, v) => format!("set {k}={v}"),
        })
        .collect()
}

#[test]
fn state_reports_the_snapshot_with_one_based_page() {
    let s = script_with_doc();
    let got = eval(
        &s,
        "local t = folio.state() return table.concat({t.path, t.title, t.page, t.page_count, t.zoom, t.mode, \
         t.selection, tostring(t.search), t.page_mode, t.theme}, '|')",
    );
    assert_eq!(got, "/papers/refs.pdf|Folio Test Paper|4|7|1.5|normal|selected words|nil|dark|nord");
}

#[test]
fn selection_and_highlights_come_from_the_snapshot() {
    let s = script_with_doc();
    assert_eq!(eval(&s, "folio.selection()"), "selected words");
    assert_eq!(eval(&s, "local h = folio.highlights()[1] return h.page, h.text, h.color, h.note"), "5\tE = mc2\tyellow\tfamous");
}

#[test]
fn page_text_defaults_to_the_current_page() {
    let s = script_with_doc();
    let text = eval(&s, "folio.page_text()");
    assert!(text.starts_with("1 Introduction"), "{text}");
    assert_eq!(text, eval(&s, "folio.page_text(4)"));
}

#[test]
fn page_text_is_nil_outside_the_document() {
    let s = script_with_doc();
    assert_eq!(eval(&s, "folio.page_text(0)"), "nil");
    assert_eq!(eval(&s, "folio.page_text(8)"), "nil");
    assert!(eval(&s, "folio.page_text(7)").contains("[1] A. Alpha"));
}

#[test]
fn page_text_is_nil_without_a_document() {
    let s = script();
    assert_eq!(eval(&s, "folio.page_text(1)"), "nil");
}

#[test]
fn page_text_is_nil_for_negative_or_fractional_pages() {
    let s = script_with_doc();
    assert_eq!(s.run_code("folio.page_text(-1)"), Ok("nil".into()));
    assert_eq!(s.run_code("folio.page_text(2.5)"), Ok("nil".into()));
}

#[test]
fn outline_lists_titles_depths_and_one_based_pages() {
    let s = script_with_doc();
    let got = eval(
        &s,
        "local out = {} for _, o in ipairs(folio.outline()) do out[#out+1] = o.depth .. ':' .. o.title .. ':' .. o.page end \
         return table.concat(out, ' ')",
    );
    assert_eq!(got, "0:Introduction:4 1:Motivation:4 0:Results:5 0:Proofs:6");
}

#[test]
fn registered_command_receives_args_and_count() {
    let s = script();
    eval(&s, "folio.command('greet', function(args, count) GOT = args .. '/' .. tostring(count) end, 'Say hi')");
    assert!(s.has_command("greet"));
    assert_eq!(s.commands(), [("greet".to_string(), "Say hi".to_string())]);
    s.call_command("greet", "world", Some(3));
    assert_eq!(eval(&s, "GOT"), "world/3");
    s.call_command("greet", "", None);
    assert_eq!(eval(&s, "GOT"), "/nil");
}

#[test]
fn command_error_becomes_an_error_notification() {
    let s = script();
    eval(&s, "folio.command('boom', function() error('kaput') end)");
    s.call_command("boom", "", None);
    let fx = effects(&s);
    assert_eq!(fx.len(), 1);
    assert!(fx[0].starts_with("notify Error lua:") && fx[0].contains("kaput"), "{fx:?}");
}

#[test]
fn api_calls_queue_effects_in_order() {
    let s = script();
    eval(
        &s,
        "folio.exec('goto_page 3') folio.notify('hi') folio.notify('careful', 'warn') folio.notify('bad', 'error') \
         folio.map('gx', 'toc', { mode = 'visual' }) folio.set('theme', 'paper') folio.set('zoom', 150) \
         folio.show('Title', 'body') folio.append('more') folio.clipboard('copied') folio.open('/tmp/x.pdf')",
    );
    assert_eq!(
        effects(&s),
        [
            "exec goto_page 3",
            "notify Info hi",
            "notify Warn careful",
            "notify Error bad",
            "map visual gx toc",
            "set theme=paper",
            "set zoom=150",
            "show Title: body",
            "append more",
            "clipboard copied",
            "open /tmp/x.pdf",
        ]
    );
    assert!(effects(&s).is_empty(), "take_effects drains the queue");
}

#[test]
fn exec_is_deferred_not_run() {
    let s = script();
    eval(&s, "folio.command('t', function() RAN = true end) folio.exec('t')");
    assert_eq!(eval(&s, "RAN"), "nil", "folio.exec only queues the command for the app");
    assert_eq!(effects(&s), ["exec t"]);
}

#[test]
fn map_to_a_function_registers_a_lua_command() {
    let s = script();
    eval(&s, "folio.map('<Space>w', function() HIT = (HIT or 0) + 1 end, { desc = 'count' })");
    let fx = effects(&s);
    let command = fx[0].strip_prefix("map normal <Space>w ").expect("a normal-mode map effect").to_string();
    assert!(command.starts_with("lua:"), "{command}");
    assert_eq!(s.commands(), [(command.clone(), "count".to_string())]);
    s.call_command(&command, "", None);
    assert_eq!(eval(&s, "HIT"), "1");
}

#[test]
fn map_to_nil_unbinds() {
    let s = script();
    eval(&s, "folio.map('s', nil)");
    assert_eq!(effects(&s), ["map normal s "]);
}

#[test]
fn prompt_and_pick_queue_effects_with_callbacks() {
    let s = script();
    eval(
        &s,
        "folio.prompt('Ask:', function(x) ANSWER = x end, 'init') folio.pick('Choose', {'a', 'b'}, function(i, item) PICKED = i .. item end)",
    );
    let fx = s.take_effects();
    let (Effect::Prompt { label, callback: prompt_cb, initial }, Effect::Pick { title, items, callback: pick_cb }) =
        (&fx[0], &fx[1])
    else {
        panic!("expected a prompt and a pick effect")
    };
    assert_eq!((label.as_str(), initial.as_str(), title.as_str()), ("Ask:", "init", "Choose"));
    assert_eq!(items, &["a", "b"]);
    assert_eq!(eval(&s, "ANSWER"), "nil", "nothing runs until the app answers");
    s.call_fn(*prompt_cb, "typed".to_string());
    s.call_fn(*pick_cb, (2, "b".to_string()));
    assert_eq!(eval(&s, "ANSWER .. ' ' .. PICKED"), "typed 2b");
}

#[test]
fn print_goes_to_notifications() {
    let s = script();
    eval(&s, "print('a', 1, nil)");
    assert_eq!(effects(&s), ["notify Info a\t1\tnil"]);
}

#[test]
fn hooks_run_in_order_and_an_error_does_not_stop_the_next() {
    let s = script();
    eval(
        &s,
        "LOG = {} folio.on('page', function(p) LOG[#LOG+1] = 'first ' .. p end) \
         folio.on('page', function() error('hook failed') end) \
         folio.on('page', function(p) LOG[#LOG+1] = 'third ' .. p end)",
    );
    assert!(s.has_hook("page") && !s.has_hook("open"));
    s.emit("page", 5);
    assert_eq!(eval(&s, "table.concat(LOG, ', ')"), "first 5, third 5");
    let fx = effects(&s);
    assert!(fx.len() == 1 && fx[0].contains("hook failed"), "{fx:?}");
}

#[test]
fn run_code_returns_values_and_short_errors() {
    let s = script();
    assert_eq!(s.run_code("1 + 1"), Ok("2".into()));
    assert_eq!(s.run_code("x = 3"), Ok("".into()));
    assert_eq!(eval(&s, "x"), "3");
    let err = s.run_code("error('nope')").unwrap_err();
    assert!(err.contains("nope") && !err.contains("stack traceback"), "{err}");
}

/// Run `code`, which must call `folio.spawn`, and wait for its callback.
fn spawn_and_wait(code: &str) -> String {
    let (tx, rx) = crossbeam_channel::unbounded();
    let s = Script::new(move || {
        let _ = tx.send(());
    })
    .unwrap();
    eval(&s, code);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !s.poll_jobs() {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(rx.recv_timeout(left).is_ok(), "spawned job didn't finish in time");
    }
    eval(&s, "RESULT")
}

#[test]
#[cfg(unix)] // uses sh
fn spawn_delivers_stdout_stderr_and_exit_code() {
    let got = spawn_and_wait(
        "folio.spawn({'sh', '-c', 'echo out; echo err >&2; exit 3'}, function(o, e, c) RESULT = o .. '|' .. e .. '|' .. c end)",
    );
    assert_eq!(got, "out\n|err\n|3");
}

#[test]
#[cfg(unix)] // uses cat
fn spawn_passes_stdin() {
    let got = spawn_and_wait("folio.spawn({'cat'}, function(o) RESULT = o end, { stdin = 'piped input' })");
    assert_eq!(got, "piped input");
}

#[test]
fn spawn_of_a_missing_program_reports_code_minus_one() {
    let got = spawn_and_wait("folio.spawn({'folio-no-such-program'}, function(o, e, c) RESULT = c end)");
    assert_eq!(got, "-1");
}

#[test]
fn spawn_with_empty_argv_is_an_error() {
    let s = script();
    assert!(s.run_code("folio.spawn({}, function() end)").unwrap_err().contains("empty command"));
}

#[test]
fn require_finds_modules_next_to_init_lua() {
    let env = IsolatedEnv::new();
    let dir = env.config_dir();
    std::fs::write(dir.join("helper.lua"), "return { answer = 42 }").unwrap();
    std::fs::create_dir_all(dir.join("lua/pkg")).unwrap();
    std::fs::write(dir.join("lua/util.lua"), "return { name = 'util' }").unwrap();
    std::fs::write(dir.join("lua/pkg/init.lua"), "return { name = 'pkg' }").unwrap();
    let s = script();
    assert_eq!(eval(&s, "require('helper').answer"), "42");
    assert_eq!(eval(&s, "require('util').name"), "util");
    assert_eq!(eval(&s, "require('pkg').name"), "pkg");
    assert_eq!(eval(&s, "folio.config_dir"), dir.display().to_string());
}

#[test]
fn shipped_init_lua_runs_and_registers_its_commands() {
    let dir = TempDir::new();
    let path = dir.path().join("init.lua");
    std::fs::write(&path, DEFAULT_INIT_LUA).unwrap();
    let s = script_with_doc();
    s.run_file(&path).expect("config/init.lua runs without errors");
    let names: Vec<String> = s.commands().into_iter().map(|c| c.0).filter(|n| !n.starts_with("lua:")).collect();
    assert_eq!(names, ["cite_here", "define", "scholar", "wordcount"]);
    let fx = effects(&s);
    assert!(fx.iter().any(|e| e == "map visual gd define"), "{fx:?}");
    assert!(fx.iter().any(|e| e.starts_with("map normal <Space>w lua:")), "{fx:?}");
    assert!(s.has_hook("highlight"));
}

#[test]
fn shipped_wordcount_counts_words_on_the_current_page() {
    let dir = TempDir::new();
    let path = dir.path().join("init.lua");
    std::fs::write(&path, DEFAULT_INIT_LUA).unwrap();
    let s = script_with_doc();
    s.run_file(&path).unwrap();
    s.take_effects();
    s.call_command("wordcount", "", None);
    let fx = s.take_effects();
    let [Effect::Notify(msg, Level::Info)] = fx.as_slice() else { panic!("expected one info notification") };
    assert!(msg.starts_with("page 4: ") && msg.ends_with(" words"), "{msg}");
}

#[test]
fn version_matches_the_crate() {
    assert_eq!(eval(&script(), "folio.version"), env!("CARGO_PKG_VERSION"));
}
