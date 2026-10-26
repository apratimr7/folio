//! Lua scripting (`~/.config/folio/init.lua`).
//!
//! Lua code never touches the app directly. Calls into the `folio` table either
//! read a snapshot of the app state or queue an [`Effect`] that the app applies
//! once the Lua call returns. That keeps the borrow structure simple and makes
//! scripts unable to observe half-updated state.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

use crossbeam_channel::{Receiver, Sender};
use mlua::{Function, Lua, MultiValue, Table, Value, Variadic};

use crate::doc::{Backend, OutlineItem};

/// A Lua function registered by the script, addressed by index.
pub type FnId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

pub enum Effect {
    Exec(String),
    Notify(String, Level),
    Map {
        mode: String,
        keys: String,
        command: String,
    },
    Show {
        title: String,
        text: String,
    },
    Append(String),
    Prompt {
        label: String,
        callback: FnId,
        initial: String,
    },
    Pick {
        title: String,
        items: Vec<String>,
        callback: FnId,
    },
    Clipboard(String),
    /// A path and, for a protected document, its password.
    Open(PathBuf, Option<String>),
    SetOption(String, String),
}

/// Read-only view of the app handed to scripts.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub path: String,
    pub title: String,
    pub page: usize,
    pub page_count: usize,
    pub zoom: f32,
    pub mode: String,
    pub selection: Option<String>,
    pub search: Option<String>,
    pub page_mode: String,
    pub theme: String,
    pub outline: Rc<Vec<OutlineItem>>,
    pub highlights: Vec<(usize, String, String, String)>,
    /// 1-based index of the active tab (0 without a document) and the number of tabs.
    pub tab: usize,
    pub tab_count: usize,
}

pub struct ScriptCommand {
    pub callback: FnId,
    pub description: String,
}

struct Job {
    callback: FnId,
    stdout: String,
    stderr: String,
    code: i32,
}

struct Shared {
    effects: Vec<Effect>,
    functions: Vec<Function>,
    hooks: BTreeMap<String, Vec<FnId>>,
    commands: BTreeMap<String, ScriptCommand>,
    snapshot: Snapshot,
    doc: Option<Rc<dyn Backend>>,
}

pub struct Script {
    lua: Lua,
    shared: Rc<RefCell<Shared>>,
    jobs_rx: Receiver<Job>,
}

fn register(shared: &Rc<RefCell<Shared>>, f: Function) -> FnId {
    let mut s = shared.borrow_mut();
    s.functions.push(f);
    s.functions.len() - 1
}

impl Script {
    pub fn new(wake: impl Fn() + Send + Sync + Clone + 'static) -> mlua::Result<Self> {
        let lua = Lua::new();
        let shared = Rc::new(RefCell::new(Shared {
            effects: Vec::new(),
            functions: Vec::new(),
            hooks: BTreeMap::new(),
            commands: BTreeMap::new(),
            snapshot: Snapshot::default(),
            doc: None,
        }));
        let (jobs_tx, jobs_rx) = crossbeam_channel::unbounded();
        install_api(&lua, &shared, jobs_tx, wake)?;
        Ok(Self { lua, shared, jobs_rx })
    }

    pub fn run_file(&self, path: &std::path::Path) -> Result<(), String> {
        let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.lua.load(&code).set_name(format!("@{}", path.display())).exec().map_err(|e| short_error(&e))
    }

    pub fn run_code(&self, code: &str) -> Result<String, String> {
        // Evaluate as an expression first so `:lua 1+1` prints something.
        let res = self
            .lua
            .load(format!("return {code}"))
            .set_name("=lua")
            .eval::<MultiValue>()
            .or_else(|_| self.lua.load(code).set_name("=lua").eval::<MultiValue>());
        match res {
            Ok(values) => Ok(values.iter().map(display_value).collect::<Vec<_>>().join("\t")),
            Err(e) => Err(short_error(&e)),
        }
    }

    pub fn set_snapshot(&self, snap: Snapshot) {
        self.shared.borrow_mut().snapshot = snap;
    }

    pub fn set_doc(&self, doc: Option<Rc<dyn Backend>>) {
        self.shared.borrow_mut().doc = doc;
    }

    pub fn take_effects(&self) -> Vec<Effect> {
        std::mem::take(&mut self.shared.borrow_mut().effects)
    }

    pub fn commands(&self) -> Vec<(String, String)> {
        self.shared.borrow().commands.iter().map(|(k, v)| (k.clone(), v.description.clone())).collect()
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.shared.borrow().commands.contains_key(name)
    }

    fn function(&self, id: FnId) -> Option<Function> {
        self.shared.borrow().functions.get(id).cloned()
    }

    fn report(&self, r: mlua::Result<()>) {
        if let Err(e) = r {
            self.shared.borrow_mut().effects.push(Effect::Notify(format!("lua: {}", short_error(&e)), Level::Error));
        }
    }

    pub fn call_command(&self, name: &str, args: &str, count: Option<u32>) {
        let id = self.shared.borrow().commands.get(name).map(|c| c.callback);
        if let Some(f) = id.and_then(|id| self.function(id)) {
            self.report(f.call::<()>((args.to_string(), count)));
        }
    }

    pub fn call_fn(&self, id: FnId, arg: impl mlua::IntoLuaMulti) {
        if let Some(f) = self.function(id) {
            self.report(f.call::<()>(arg));
        }
    }

    /// Fire an event hook (`open`, `page`, `selection`, `search`, `highlight`, `quit`).
    pub fn emit(&self, event: &str, arg: impl mlua::IntoLua + Clone) {
        let ids = self.shared.borrow().hooks.get(event).cloned().unwrap_or_default();
        for id in ids {
            if let Some(f) = self.function(id) {
                self.report(f.call::<()>(arg.clone()));
            }
        }
    }

    pub fn has_hook(&self, event: &str) -> bool {
        self.shared.borrow().hooks.get(event).is_some_and(|v| !v.is_empty())
    }

    /// Deliver finished `folio.spawn` jobs. Returns true if anything ran.
    pub fn poll_jobs(&self) -> bool {
        let mut any = false;
        while let Ok(job) = self.jobs_rx.try_recv() {
            any = true;
            self.call_fn(job.callback, (job.stdout, job.stderr, job.code));
        }
        any
    }
}

/// The error message without the stack traceback, which goes to stderr instead.
fn short_error(e: &mlua::Error) -> String {
    let full = e.to_string();
    match full.split_once("\nstack traceback:") {
        Some((msg, _)) => {
            eprintln!("folio: lua: {full}");
            msg.trim().to_string()
        }
        None => full,
    }
}

fn display_value(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::String(s) => s.to_string_lossy().to_string(),
        other => other.to_string().unwrap_or_else(|_| format!("{other:?}")),
    }
}

fn install_api(
    lua: &Lua,
    shared: &Rc<RefCell<Shared>>,
    jobs_tx: Sender<Job>,
    wake: impl Fn() + Send + Sync + Clone + 'static,
) -> mlua::Result<()> {
    let folio = lua.create_table()?;
    let push = {
        let shared = shared.clone();
        move |e: Effect| shared.borrow_mut().effects.push(e)
    };

    // folio.exec("goto_page 12")
    let p = push.clone();
    folio.set(
        "exec",
        lua.create_function(move |_, cmd: String| {
            let _: () = p(Effect::Exec(cmd));
            Ok(())
        })?,
    )?;

    // folio.notify(msg, "info"|"warn"|"error")
    let p = push.clone();
    folio.set(
        "notify",
        lua.create_function(move |_, (msg, level): (String, Option<String>)| {
            let level = match level.as_deref() {
                Some("warn") => Level::Warn,
                Some("error") => Level::Error,
                _ => Level::Info,
            };
            let _: () = p(Effect::Notify(msg, level));
            Ok(())
        })?,
    )?;

    // folio.map("gx", "command" | function, { mode = "normal" })
    {
        let p = push.clone();
        let shared = shared.clone();
        folio.set(
            "map",
            lua.create_function(move |_, (keys, action, opts): (String, Value, Option<Table>)| {
                let mode =
                    opts.as_ref().and_then(|o| o.get::<Option<String>>("mode").ok().flatten()).unwrap_or_else(|| "normal".into());
                let command = match action {
                    Value::String(s) => s.to_str()?.to_string(),
                    Value::Function(f) => {
                        let desc = opts
                            .as_ref()
                            .and_then(|o| o.get::<Option<String>>("desc").ok().flatten())
                            .unwrap_or_else(|| format!("lua mapping {keys}"));
                        let id = register(&shared, f);
                        let name = format!("lua:{id}");
                        shared.borrow_mut().commands.insert(name.clone(), ScriptCommand { callback: id, description: desc });
                        name
                    }
                    Value::Nil => String::new(),
                    _ => return Err(mlua::Error::runtime("folio.map: action must be a string or function")),
                };
                let _: () = p(Effect::Map { mode, keys, command });
                Ok(())
            })?,
        )?;
    }

    // folio.command("name", function(args, count) end, "description")
    {
        let shared = shared.clone();
        folio.set(
            "command",
            lua.create_function(move |_, (name, f, desc): (String, Function, Option<String>)| {
                let id = register(&shared, f);
                shared
                    .borrow_mut()
                    .commands
                    .insert(name, ScriptCommand { callback: id, description: desc.unwrap_or_else(|| "user command".into()) });
                Ok(())
            })?,
        )?;
    }

    // folio.on("open", function(state) end)
    {
        let shared = shared.clone();
        folio.set(
            "on",
            lua.create_function(move |_, (event, f): (String, Function)| {
                let id = register(&shared, f);
                shared.borrow_mut().hooks.entry(event).or_default().push(id);
                Ok(())
            })?,
        )?;
    }

    // folio.state() -> { path, title, page (1-based), page_count, zoom, mode, ... }
    {
        let shared = shared.clone();
        folio.set(
            "state",
            lua.create_function(move |lua, ()| {
                let s = shared.borrow();
                let snap = &s.snapshot;
                let t = lua.create_table()?;
                t.set("path", snap.path.clone())?;
                t.set("title", snap.title.clone())?;
                t.set("page", snap.page + 1)?;
                t.set("page_count", snap.page_count)?;
                t.set("zoom", snap.zoom)?;
                t.set("mode", snap.mode.clone())?;
                t.set("selection", snap.selection.clone())?;
                t.set("search", snap.search.clone())?;
                t.set("page_mode", snap.page_mode.clone())?;
                t.set("tab", snap.tab)?;
                t.set("tab_count", snap.tab_count)?;
                t.set("theme", snap.theme.clone())?;
                Ok(t)
            })?,
        )?;
    }

    // folio.selection() -> string | nil
    {
        let shared = shared.clone();
        folio.set("selection", lua.create_function(move |_, ()| Ok(shared.borrow().snapshot.selection.clone()))?)?;
    }

    // folio.page_text(page?) -> string   (1-based, default current page)
    {
        let shared = shared.clone();
        folio.set(
            "page_text",
            lua.create_function(move |_, page: mlua::Value| {
                let s = shared.borrow();
                // Anything but a whole number in 1..page_count gives nil.
                let page = match page {
                    mlua::Value::Nil => s.snapshot.page,
                    mlua::Value::Integer(p) if p >= 1 => (p - 1) as usize,
                    mlua::Value::Number(p) if p >= 1.0 && p.fract() == 0.0 && p < 1e15 => p as usize - 1,
                    _ => return Ok(None),
                };
                Ok(s.doc.as_ref().filter(|d| page < d.page_count()).map(|d| {
                    let t = d.page_text(page);
                    t.span_text(0, t.chars.len())
                }))
            })?,
        )?;
    }

    // folio.outline() -> { {title=, depth=, page=}, ... }
    {
        let shared = shared.clone();
        folio.set(
            "outline",
            lua.create_function(move |lua, ()| {
                let outline = shared.borrow().snapshot.outline.clone();
                let t = lua.create_table()?;
                for (i, item) in outline.iter().enumerate() {
                    let e = lua.create_table()?;
                    e.set("title", item.title.clone())?;
                    e.set("depth", item.depth)?;
                    e.set("page", item.target.map(|p| p.page + 1))?;
                    t.set(i + 1, e)?;
                }
                Ok(t)
            })?,
        )?;
    }

    // folio.highlights() -> { {page=, text=, color=, note=}, ... }
    {
        let shared = shared.clone();
        folio.set(
            "highlights",
            lua.create_function(move |lua, ()| {
                let s = shared.borrow();
                let t = lua.create_table()?;
                for (i, (page, text, color, note)) in s.snapshot.highlights.iter().enumerate() {
                    let e = lua.create_table()?;
                    e.set("page", page + 1)?;
                    e.set("text", text.clone())?;
                    e.set("color", color.clone())?;
                    e.set("note", note.clone())?;
                    t.set(i + 1, e)?;
                }
                Ok(t)
            })?,
        )?;
    }

    // folio.show(title, text) / folio.append(text): the side text panel.
    let p = push.clone();
    folio.set(
        "show",
        lua.create_function(move |_, (title, text): (String, Option<String>)| {
            let _: () = p(Effect::Show { title, text: text.unwrap_or_default() });
            Ok(())
        })?,
    )?;
    let p = push.clone();
    folio.set(
        "append",
        lua.create_function(move |_, text: String| {
            let _: () = p(Effect::Append(text));
            Ok(())
        })?,
    )?;

    // folio.prompt("Ask:", function(input) end, "initial")
    {
        let p = push.clone();
        let shared = shared.clone();
        folio.set(
            "prompt",
            lua.create_function(move |_, (label, f, initial): (String, Function, Option<String>)| {
                let callback = register(&shared, f);
                let _: () = p(Effect::Prompt { label, callback, initial: initial.unwrap_or_default() });
                Ok(())
            })?,
        )?;
    }

    // folio.pick("Title", {"a", "b"}, function(choice, index) end)
    {
        let p = push.clone();
        let shared = shared.clone();
        folio.set(
            "pick",
            lua.create_function(move |_, (title, items, f): (String, Vec<String>, Function)| {
                let callback = register(&shared, f);
                let _: () = p(Effect::Pick { title, items, callback });
                Ok(())
            })?,
        )?;
    }

    let p = push.clone();
    folio.set(
        "clipboard",
        lua.create_function(move |_, text: String| {
            let _: () = p(Effect::Clipboard(text));
            Ok(())
        })?,
    )?;
    let p = push.clone();
    folio.set(
        "open",
        lua.create_function(move |_, (path, password): (String, Option<String>)| {
            let _: () = p(Effect::Open(PathBuf::from(path), password));
            Ok(())
        })?,
    )?;
    let p = push.clone();
    folio.set(
        "set",
        lua.create_function(move |_, (key, value): (String, Value)| {
            let _: () = p(Effect::SetOption(key, display_value(&value)));
            Ok(())
        })?,
    )?;

    // folio.spawn({"curl", "-s", url}, function(stdout, stderr, code) end, { stdin = "..." })
    // Runs without blocking the UI; the callback fires on the UI thread.
    {
        let shared = shared.clone();
        folio.set(
            "spawn",
            lua.create_function(move |_, (argv, f, opts): (Vec<String>, Function, Option<Table>)| {
                if argv.is_empty() {
                    return Err(mlua::Error::runtime("folio.spawn: empty command"));
                }
                let stdin = opts.and_then(|o| o.get::<Option<String>>("stdin").ok().flatten());
                let callback = register(&shared, f);
                let tx = jobs_tx.clone();
                let wake = wake.clone();
                std::thread::spawn(move || {
                    let job = run_process(&argv, stdin.as_deref(), callback);
                    let _ = tx.send(job);
                    wake();
                });
                Ok(())
            })?,
        )?;
    }

    folio.set("config_dir", crate::config::config_dir().to_string_lossy().to_string())?;
    folio.set("version", env!("CARGO_PKG_VERSION"))?;

    // print() goes to the notification area as well as stderr.
    let p = push;
    lua.globals().set(
        "print",
        lua.create_function(move |_, args: Variadic<Value>| {
            let msg = args.iter().map(display_value).collect::<Vec<_>>().join("\t");
            eprintln!("[lua] {msg}");
            p(Effect::Notify(msg, Level::Info));
            Ok(())
        })?,
    )?;

    // Let `require` find modules next to init.lua.
    let package: Table = lua.globals().get("package")?;
    let dir = crate::config::config_dir();
    let path: String = package.get("path")?;
    package.set("path", format!("{0}/?.lua;{0}/lua/?.lua;{0}/lua/?/init.lua;{path}", dir.display()))?;

    lua.globals().set("folio", folio)?;
    Ok(())
}

fn run_process(argv: &[String], stdin: Option<&str>, callback: FnId) -> Job {
    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Job { callback, stdout: String::new(), stderr: e.to_string(), code: -1 },
    };
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let input = input.to_string();
        // Write from a separate thread so large inputs can't deadlock against stdout.
        std::thread::spawn(move || {
            let _ = pipe.write_all(input.as_bytes());
        });
    }
    match child.wait_with_output() {
        Ok(out) => Job {
            callback,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        },
        Err(e) => Job { callback, stdout: String::new(), stderr: e.to_string(), code: -1 },
    }
}
