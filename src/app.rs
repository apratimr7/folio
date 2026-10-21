//! Application state and command execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::commands::{self, BUILTINS};
use crate::config::{self, Config, PageMode, THEME_NAMES, Theme};
use crate::doc::{DocPos, LinkTarget, Rect};
use crate::keymap::{self, Chord, Dispatcher, ModeMap, Outcome};
use crate::refs::{self, RefKind};
use crate::script::{Effect, FnId, Level, Script, Snapshot};
use crate::session::{HintAction, HintItem, HintMode, Hints, Peek, Ruler, Selection, Session, render_colors};
use crate::store::{self, Bookmark, Global, Highlight};
use crate::ui::picker::{PickData, PickItem, Picker, PickerEvent, PickerKind};
use crate::ui::style;
use crate::view::Fit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Visual => "visual",
        }
    }
}

#[derive(Clone, Debug)]
pub enum PromptKind {
    Search { backward: bool },
    GotoPage,
    CommandArg(String),
    Lua(FnId),
    Bookmark,
    Annotate,
}

pub struct Prompt {
    pub label: String,
    pub text: String,
    pub kind: PromptKind,
    pub origin: Option<DocPos>,
    pub history_idx: Option<usize>,
}

pub struct Toast {
    pub text: String,
    pub level: Level,
    pub born: Instant,
}

pub struct TextPanel {
    pub title: String,
    pub text: String,
    pub visible: bool,
}

pub struct App {
    pub ctx: egui::Context,
    pub cfg: Config,
    pub theme: Theme,
    pub keymaps: HashMap<String, ModeMap>,
    pub disp: Dispatcher,
    pub script: Option<Script>,
    pub global: Global,
    pub doc: Option<Session>,
    pub mode: Mode,
    pub picker: Option<Picker>,
    pub prompt: Option<Prompt>,
    pub toasts: Vec<Toast>,
    pub show_statusbar: bool,
    pub show_sidebar: bool,
    pub text_panel: TextPanel,
    pub last_frame: Instant,
    pub fullscreen: bool,
    pub search_history: Vec<String>,
    /// Pending mouse drag selection start.
    pub drag_anchor: Option<(usize, usize)>,
    pub lua_picker_cb: Option<FnId>,
    pub ipc: Option<crate::ipc::Server>,
    /// Nesting depth of `process_effects` (scripts calling commands calling scripts).
    pub effect_depth: u32,
    pub quit_emitted: bool,
}

pub fn notify_into(toasts: &mut Vec<Toast>, text: impl Into<String>, level: Level) {
    let text = text.into();
    if level == Level::Error {
        eprintln!("folio: {text}");
    }
    // Repeating the newest message just refreshes it.
    if let Some(last) = toasts.last_mut()
        && last.text == text
    {
        last.born = Instant::now();
        return;
    }
    toasts.push(Toast { text, level, born: Instant::now() });
    if toasts.len() > 8 {
        toasts.remove(0);
    }
}

fn build_keymaps(cfg: &Config, toasts: &mut Vec<Toast>) -> HashMap<String, ModeMap> {
    let mut maps: HashMap<String, ModeMap> = HashMap::new();
    for (mode, defaults) in [("normal", keymap::NORMAL_DEFAULTS), ("visual", keymap::VISUAL_DEFAULTS)] {
        let m = maps.entry(mode.to_string()).or_default();
        for (k, c) in defaults {
            m.bind(keymap::parse_seq(k).expect("valid default binding"), c.to_string());
        }
    }
    for (mode, binds) in &cfg.keys {
        let m = maps.entry(mode.clone()).or_default();
        for (k, c) in binds {
            match keymap::parse_seq(k) {
                Ok(seq) => m.bind(seq, c.clone()),
                Err(e) => notify_into(toasts, format!("bad key '{k}': {e}"), Level::Warn),
            }
        }
    }
    maps
}

/// Labels like `a s d … aa as …`, all the same length so none is a prefix of another.
fn hint_labels(n: usize, chars: &str) -> Vec<String> {
    let alphabet: Vec<char> = chars.chars().collect();
    let k = alphabet.len().max(2);
    let mut len = 1;
    while k.pow(len as u32) < n {
        len += 1;
    }
    (0..n)
        .map(|mut i| {
            let mut s = Vec::with_capacity(len);
            for _ in 0..len {
                s.push(alphabet[i % k]);
                i /= k;
            }
            s.into_iter().rev().collect()
        })
        .collect()
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        let ctx = cc.egui_ctx.clone();
        let mut toasts = Vec::new();
        let cfg = Config::load().unwrap_or_else(|e| {
            notify_into(&mut toasts, format!("config: {e:#}"), Level::Error);
            Config::default()
        });
        let theme = Theme::preset(&cfg.general.theme).with_overrides(&cfg.theme);
        style::install_fonts(&ctx, &cfg.general);
        style::apply(&ctx, &theme, &cfg.general);
        let keymaps = build_keymaps(&cfg, &mut toasts);
        let mut app = Self {
            show_statusbar: cfg.general.statusbar,
            ctx,
            cfg,
            theme,
            keymaps,
            disp: Dispatcher::default(),
            script: None,
            global: Global::load(),
            doc: None,
            mode: Mode::Normal,
            picker: None,
            prompt: None,
            toasts,
            show_sidebar: false,
            text_panel: TextPanel { title: String::new(), text: String::new(), visible: false },
            last_frame: Instant::now(),
            fullscreen: false,
            search_history: Vec::new(),
            drag_anchor: None,
            lua_picker_cb: None,
            ipc: None,
            effect_depth: 0,
            quit_emitted: false,
        };
        let wake = app.ctx.clone();
        match crate::ipc::Server::start(move || wake.request_repaint()) {
            Ok(server) => app.ipc = Some(server),
            Err(e) => {
                eprintln!("folio: remote control disabled: {e:#}");
                notify_into(&mut app.toasts, format!("Remote control disabled: {e:#}"), Level::Warn);
            }
        }
        app.load_script();
        match path {
            Some(p) => app.open(&p),
            None => app.open_recent_picker(),
        }
        app
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        notify_into(&mut self.toasts, text, Level::Info);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        notify_into(&mut self.toasts, text, Level::Warn);
    }

    fn load_script(&mut self) {
        let ctx = self.ctx.clone();
        match Script::new(move || ctx.request_repaint()) {
            Ok(s) => {
                // Scripts may call folio.state() at load time.
                s.set_snapshot(self.snapshot());
                let init = config::config_dir().join("init.lua");
                if init.exists()
                    && let Err(e) = s.run_file(&init)
                {
                    notify_into(&mut self.toasts, format!("init.lua: {e}"), Level::Error);
                }
                self.script = Some(s);
                self.process_effects();
            }
            Err(e) => notify_into(&mut self.toasts, format!("lua: {e}"), Level::Error),
        }
    }

    pub fn reload_config(&mut self) {
        match Config::load() {
            Ok(cfg) => self.cfg = cfg,
            Err(e) => {
                notify_into(&mut self.toasts, format!("config: {e:#}"), Level::Error);
                return;
            }
        }
        self.theme = Theme::preset(&self.cfg.general.theme).with_overrides(&self.cfg.theme);
        style::install_fonts(&self.ctx, &self.cfg.general);
        style::apply(&self.ctx, &self.theme, &self.cfg.general);
        self.keymaps = build_keymaps(&self.cfg, &mut self.toasts);
        self.show_statusbar = self.cfg.general.statusbar;
        if let Some(d) = &mut self.doc {
            d.pool.set_colors(render_colors(&self.theme));
            d.view.set_gap(self.cfg.general.page_gap);
            d.textures.clear();
            d.tex_bytes = 0;
        }
        self.load_script();
        self.sync_script_doc();
        self.notify("Configuration reloaded");
    }

    // ------------------------------------------------------------------
    // Documents

    pub fn open(&mut self, path: &Path) {
        if let Some(d) = &mut self.doc {
            d.save_state();
        }
        match Session::open(path, &self.cfg, &self.theme, &self.ctx) {
            Ok(s) => {
                self.global.touch_recent(&s.path, &s.title, s.state.pos.map(|p| p.page).unwrap_or(0));
                self.global.save();
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{} — folio", s.title)));
                self.doc = Some(s);
                self.mode = Mode::Normal;
                self.picker = None;
                self.sync_script_doc();
                let p = self.doc.as_ref().map(|d| d.path.to_string_lossy().to_string()).unwrap_or_default();
                self.emit("open", p);
            }
            Err(e) => notify_into(&mut self.toasts, format!("{}: {e:#}", path.display()), Level::Error),
        }
    }

    fn sync_script_doc(&self) {
        if let Some(s) = &self.script {
            s.set_doc(self.doc.as_ref().map(|d| d.backend.clone()));
        }
    }

    pub fn save_all(&mut self) {
        if let Some(d) = &mut self.doc {
            d.save_state();
            let page = d.view.current_page();
            self.global.touch_recent(&d.path.clone(), &d.title.clone(), page);
        }
        self.global.save();
    }

    // ------------------------------------------------------------------
    // Scripting glue

    pub fn snapshot(&mut self) -> Snapshot {
        let mode = self.mode.name().to_string();
        let theme = self.theme.name.clone();
        let Some(d) = &mut self.doc else { return Snapshot { mode, theme, ..Default::default() } };
        Snapshot {
            path: d.path.to_string_lossy().to_string(),
            title: d.title.clone(),
            page: d.view.current_page(),
            page_count: d.view.page_count(),
            zoom: d.view.zoom,
            mode,
            selection: d.selection_text(),
            search: d.search.as_ref().map(|s| s.query.clone()),
            page_mode: format!("{:?}", d.page_mode).to_lowercase(),
            theme,
            outline: d.outline.clone(),
            highlights: d.state.highlights.iter().map(|h| (h.page, h.text.clone(), h.color.clone(), h.note.clone())).collect(),
        }
    }

    fn with_script(&mut self, f: impl FnOnce(&Script)) {
        if self.script.is_none() {
            return;
        }
        let snap = self.snapshot();
        let s = self.script.as_ref().unwrap();
        s.set_snapshot(snap);
        f(s);
        self.process_effects();
    }

    /// The `quit` hook, at most once, whether we quit by command or the window is closed.
    pub fn emit_quit(&mut self) {
        if !self.quit_emitted {
            self.quit_emitted = true;
            self.emit("quit", String::new());
        }
    }

    pub fn emit(&mut self, event: &str, arg: String) {
        if self.script.as_ref().is_some_and(|s| s.has_hook(event)) {
            self.with_script(|s| s.emit(event, arg));
        }
    }

    pub fn poll_jobs(&mut self) {
        if self.script.is_none() {
            return;
        }
        let snap = self.snapshot();
        let s = self.script.as_ref().unwrap();
        s.set_snapshot(snap);
        if s.poll_jobs() {
            self.process_effects();
        }
    }

    /// Apply queued script effects. Effects can run commands that call Lua again, which queues
    /// more effects; a runaway chain (a command that execs itself) is cut off instead of
    /// overflowing the stack.
    pub fn process_effects(&mut self) {
        const MAX_NESTING: u32 = 24;
        if self.effect_depth >= MAX_NESTING {
            if let Some(s) = &self.script {
                let dropped = s.take_effects();
                if let Some(Effect::Exec(line)) = dropped.iter().find(|e| matches!(e, Effect::Exec(_))) {
                    let line = line.clone();
                    notify_into(&mut self.toasts, format!("lua: command loop detected, stopped at '{line}'"), Level::Error);
                }
            }
            return;
        }
        self.effect_depth += 1;
        for _ in 0..64 {
            let Some(s) = &self.script else { break };
            let effects = s.take_effects();
            if effects.is_empty() {
                break;
            }
            for e in effects {
                self.apply_effect(e);
            }
        }
        self.effect_depth -= 1;
    }

    fn apply_effect(&mut self, e: Effect) {
        match e {
            Effect::Exec(line) => self.run_line(&line, None),
            Effect::Notify(msg, level) => notify_into(&mut self.toasts, msg, level),
            Effect::Map { mode, keys, command } => match keymap::parse_seq(&keys) {
                Ok(seq) => self.keymaps.entry(mode).or_default().bind(seq, command),
                Err(e) => self.warn(format!("folio.map '{keys}': {e}")),
            },
            Effect::Show { title, text } => {
                self.text_panel = TextPanel { title, text, visible: true };
            }
            Effect::Append(text) => {
                self.text_panel.text.push_str(&text);
                self.text_panel.visible = true;
            }
            Effect::Prompt { label, callback, initial } => {
                self.prompt =
                    Some(Prompt { label, text: initial, kind: PromptKind::Lua(callback), origin: None, history_idx: None });
            }
            Effect::Pick { title, items, callback } => {
                let items = items.into_iter().enumerate().map(|(i, s)| PickItem::new(s, PickData::Lua(callback, i))).collect();
                self.picker = Some(Picker::new(PickerKind::Lua, title, items));
                self.lua_picker_cb = Some(callback);
            }
            Effect::Clipboard(text) => self.ctx.copy_text(text),
            Effect::Open(p) => self.open(&p),
            Effect::SetOption(k, v) => self.set_option(&k, &v),
        }
    }

    fn set_option(&mut self, key: &str, value: &str) {
        let b = matches!(value, "true" | "1" | "on" | "yes");
        let f = value.parse::<f32>().ok();
        match key {
            "statusbar" => self.show_statusbar = b,
            "sidebar" => self.show_sidebar = b,
            "smooth_scroll" => self.cfg.general.smooth_scroll = b,
            "scroll_step" => self.cfg.general.scroll_step = f.unwrap_or(self.cfg.general.scroll_step),
            "zoom_step" => self.cfg.general.zoom_step = f.unwrap_or(self.cfg.general.zoom_step).max(1.01),
            "hint_chars" => self.cfg.general.hint_chars = value.to_string(),
            "theme" => self.set_theme(value),
            "page_mode" => self.run_line(&format!("page_mode {value}"), None),
            "page_gap" => {
                if let (Some(g), Some(d)) = (f, &mut self.doc) {
                    self.cfg.general.page_gap = g;
                    d.view.set_gap(g);
                }
            }
            _ => {
                // Theme colors: folio.set("theme.accent", "#ff8800")
                if let Some(name) = key.strip_prefix("theme.") {
                    let mut o = config::ThemeOverrides::default();
                    let v = Some(value.to_string());
                    match name {
                        "bg" => o.bg = v,
                        "surface" => o.surface = v,
                        "surface_alt" => o.surface_alt = v,
                        "border" => o.border = v,
                        "text" => o.text = v,
                        "text_dim" => o.text_dim = v,
                        "accent" => o.accent = v,
                        "accent_text" => o.accent_text = v,
                        "search_match" => o.search_match = v,
                        "search_current" => o.search_current = v,
                        "selection" => o.selection = v,
                        "ruler" => o.ruler = v,
                        "hint_bg" => o.hint_bg = v,
                        "hint_text" => o.hint_text = v,
                        "page_bg" => o.page_bg = v,
                        "page_fg" => o.page_fg = v,
                        "page_shadow" => o.page_shadow = v,
                        "radius" => o.radius = f,
                        _ => {
                            self.warn(format!("unknown theme key '{name}'"));
                            return;
                        }
                    }
                    self.theme = self.theme.clone().with_overrides(&o);
                    self.apply_theme();
                } else {
                    self.warn(format!("unknown option '{key}'"));
                }
            }
        }
    }

    fn set_theme(&mut self, name: &str) {
        if !THEME_NAMES.contains(&name) {
            self.warn(format!("unknown theme '{name}' (have: {})", THEME_NAMES.join(", ")));
            return;
        }
        self.cfg.general.theme = name.to_string();
        self.theme = Theme::preset(name).with_overrides(&self.cfg.theme);
        self.apply_theme();
    }

    fn apply_theme(&mut self) {
        style::apply(&self.ctx, &self.theme, &self.cfg.general);
        if let Some(d) = &mut self.doc {
            d.pool.set_colors(render_colors(&self.theme));
            if d.page_mode != PageMode::Normal {
                d.textures.clear();
                d.tex_bytes = 0;
            }
        }
    }

    // ------------------------------------------------------------------
    // Keyboard

    pub fn feed_key(&mut self, chord: Chord) {
        let map = self.keymaps.get(self.mode.name()).cloned().unwrap_or_default();
        match self.disp.feed(&map, chord) {
            Outcome::Pending => {}
            Outcome::Run { command, count } => self.run_line(&command, count),
            Outcome::Unbound(seq) => {
                // Visual mode falls back to normal-mode bindings for navigation.
                if self.mode == Mode::Visual
                    && let Ok(chords) = keymap::parse_seq(&seq)
                {
                    let normal = self.keymaps.get("normal").cloned().unwrap_or_default();
                    let mut d = Dispatcher::default();
                    for c in chords {
                        if let Outcome::Run { command, count } = d.feed(&normal, c) {
                            self.run_line(&command, count);
                        }
                    }
                }
            }
        }
    }

    pub fn tick_keys(&mut self) {
        let timeout = Duration::from_millis(self.cfg.general.key_timeout_ms);
        let map = self.keymaps.get(self.mode.name()).cloned().unwrap_or_default();
        if let Some(Outcome::Run { command, count }) = self.disp.tick(&map, timeout) {
            self.run_line(&command, count);
        }
        if let Some(d) = self.disp.deadline(timeout) {
            self.ctx.request_repaint_after(d + Duration::from_millis(5));
        }
    }

    /// Key bindings for a command, for display.
    pub fn keys_for(&self, command: &str) -> String {
        let mut keys: Vec<String> = self
            .keymaps
            .get(self.mode.name())
            .into_iter()
            .flat_map(|m| m.iter())
            .filter(|(_, c)| c.as_str() == command)
            .map(|(s, _)| keymap::format_seq(s))
            .collect();
        keys.sort_by_key(|k| (k.len(), k.clone()));
        keys.truncate(2);
        keys.join("  ")
    }

    // ------------------------------------------------------------------
    // Commands

    pub fn run_line(&mut self, line: &str, count: Option<u32>) {
        let (name, args) = commands::split(line);
        if name.is_empty() {
            return;
        }
        // `:42` goes to page 42, like vim.
        if name.chars().all(|c| c.is_ascii_digit()) {
            self.goto_page_str(name);
            return;
        }
        if self.script.as_ref().is_some_and(|s| s.has_command(name)) {
            let (name, args) = (name.to_string(), args.to_string());
            self.with_script(|s| s.call_command(&name, &args, count));
            return;
        }
        if commands::builtin(name).is_none() {
            self.warn(format!("unknown command '{name}'"));
            return;
        }
        self.run_builtin(name, args, count);
    }

    fn need_doc(&mut self) -> bool {
        if self.doc.is_none() {
            self.notify("No document open — press o to open one");
            return false;
        }
        true
    }

    fn ask(&mut self, label: &str, kind: PromptKind, initial: String) {
        let origin = self.doc.as_ref().map(|d| d.view.pos_at(0.0));
        self.prompt = Some(Prompt { label: label.to_string(), text: initial, kind, origin, history_idx: None });
    }

    fn run_builtin(&mut self, name: &str, args: &str, count: Option<u32>) {
        let n = count.unwrap_or(1).max(1) as f32;
        let animate = self.cfg.general.smooth_scroll;
        let step = self.cfg.general.scroll_step;
        let zoom_step = self.cfg.general.zoom_step;
        // Commands that don't need a document.
        match name {
            "command_palette" => return self.open_command_palette(args),
            "open_file" => {
                if args.is_empty() {
                    return self.open_file_picker();
                }
                let p = shellexpand(args);
                return self.open(&p);
            }
            "recent_files" => return self.open_recent_picker(),
            "quit" | "q" => {
                self.save_all();
                self.emit_quit();
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            "help" => return self.open_help(),
            "reload_config" => return self.reload_config(),
            "cycle_theme" => {
                let i = THEME_NAMES.iter().position(|t| *t == self.theme.name).unwrap_or(0);
                let next = THEME_NAMES[(i + 1) % THEME_NAMES.len()];
                self.set_theme(next);
                self.notify(format!("Theme: {next}"));
                return;
            }
            "theme" => {
                if args.is_empty() {
                    let items = THEME_NAMES.iter().map(|t| PickItem::new(*t, PickData::Theme(t.to_string()))).collect::<Vec<_>>();
                    self.picker = Some(Picker::new(PickerKind::Themes, "Theme", items));
                } else {
                    self.set_theme(args);
                }
                return;
            }
            "toggle_statusbar" => {
                self.show_statusbar = !self.show_statusbar;
                return;
            }
            "toggle_text_panel" => {
                self.text_panel.visible = !self.text_panel.visible;
                if self.text_panel.visible && self.text_panel.title.is_empty() {
                    self.text_panel.title = "Notes".into();
                }
                return;
            }
            "fullscreen" => {
                self.fullscreen = !self.fullscreen;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                return;
            }
            "lua" => {
                if self.script.is_none() {
                    return;
                }
                let snap = self.snapshot();
                let s = self.script.as_ref().unwrap();
                s.set_snapshot(snap);
                let res = s.run_code(args);
                self.process_effects();
                match res {
                    Ok(out) if !out.is_empty() && out != "nil" => self.notify(out),
                    Ok(_) => {}
                    Err(e) => notify_into(&mut self.toasts, e, Level::Error),
                }
                return;
            }
            "clear" => {
                // Like Esc: Lua pickers and prompts get their `nil` callback.
                if self.picker.is_some() {
                    self.picker_event(PickerEvent::Close);
                }
                if self.prompt.is_some() {
                    self.prompt_cancel();
                }
                if let Some(d) = &mut self.doc {
                    d.search = None;
                    d.text.cancel_search();
                    d.selection = None;
                    d.hints = None;
                    d.peek = None;
                    d.pending_ref = None;
                }
                self.mode = Mode::Normal;
                return;
            }
            "echo" => return self.notify(args.to_string()),
            "open_url" => return self.open_link(args),
            "screenshot" => {
                let path = if args.is_empty() { PathBuf::from("folio.png") } else { shellexpand(args) };
                let cb = egui::ScreenshotCallback::new(move |img| {
                    if let Err(e) = save_png(&path, &img) {
                        eprintln!("folio: screenshot failed: {e}");
                    }
                });
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(cb));
                self.ctx.request_repaint();
                return;
            }
            _ => {}
        }
        if !self.need_doc() {
            return;
        }
        // Commands whose argument we prompt for.
        if args.is_empty() {
            let prompt = match name {
                "goto_page" => Some(("page", PromptKind::GotoPage)),
                "zoom" => Some(("zoom %", PromptKind::CommandArg("zoom".into()))),
                "set_mark" | "goto_mark" | "page_mode" => Some((name, PromptKind::CommandArg(name.into()))),
                _ => None,
            };
            if let Some((label, kind)) = prompt {
                return self.ask(label, kind, String::new());
            }
        }
        let mut emit_selection = false;
        let d = self.doc.as_mut().unwrap();
        let vh = d.view.viewport.1;
        match name {
            "move_down" | "move_up" if d.ruler.is_some() => {
                let delta = if name == "move_down" { n as i64 } else { -(n as i64) };
                move_ruler(d, delta, animate);
            }
            "move_down" => d.view.scroll_by(step * n, animate),
            "move_up" => d.view.scroll_by(-step * n, animate),
            "move_left" => {
                d.view.scroll_x += step * n;
                d.view.clamp();
            }
            "move_right" => {
                d.view.scroll_x -= step * n;
                d.view.clamp();
            }
            "half_page_down" => d.view.scroll_by(vh * 0.5 * n, animate),
            "half_page_up" => d.view.scroll_by(-vh * 0.5 * n, animate),
            "screen_down" => d.view.scroll_by((vh - 40.0) * n, animate),
            "screen_up" => d.view.scroll_by(-(vh - 40.0) * n, animate),
            "next_page" | "prev_page" => {
                // Page jumps park the view half a gap above the page, so probe one gap below the
                // scroll target (not the animated position) to find the page we're really on.
                let probe = d.view.target_y + d.view.gap;
                let cur = d.view.page_at(probe);
                let into_page = probe - d.view.page_top(cur);
                let delta = n as i64 * if name == "next_page" { 1 } else { -1 };
                let mut page = (cur as i64 + delta).clamp(0, d.view.page_count() as i64 - 1) as usize;
                // Scrolled into a page: "prev" goes to its own top first.
                if name == "prev_page" && into_page > d.view.gap * 1.5 && n as i64 == 1 {
                    page = cur;
                }
                d.view.goto(DocPos { page, y: 0.0 }, 0.0, animate);
                d.view.target_y -= d.view.gap * 0.5;
                d.view.clamp();
            }
            "goto_start" | "goto_end" => {
                let page = match count {
                    Some(c) => (c as usize).saturating_sub(1),
                    None if name == "goto_start" => 0,
                    None => d.view.page_count().saturating_sub(1),
                };
                let y = if count.is_none() && name == "goto_end" { 1e6 } else { 0.0 };
                d.jump(DocPos { page, y }, 0.0, false);
                if y > 0.0 {
                    d.view.scroll_by(1e9, false);
                }
            }
            "goto_page" => {
                let a = args.to_string();
                self.goto_page_str(&a);
            }
            "goto_page_prompt" => self.ask("page", PromptKind::GotoPage, String::new()),
            "zoom_in" | "zoom_out" => {
                let f = if name == "zoom_in" { zoom_step.powf(n) } else { 1.0 / zoom_step.powf(n) };
                d.view.fit = Fit::None;
                let z = d.view.zoom * f;
                d.view.set_zoom(z, 0.5);
                d.dirty = true;
            }
            "zoom" => {
                if let Ok(p) = args.trim_end_matches('%').parse::<f32>() {
                    d.view.fit = Fit::None;
                    d.view.set_zoom(p / 100.0, 0.5);
                }
            }
            "fit_width" => {
                d.view.fit = Fit::Width;
                d.view.apply_fit();
            }
            "fit_page" => {
                d.view.fit = Fit::Page;
                d.view.apply_fit();
                let page = d.view.current_page();
                d.view.goto(DocPos { page, y: 0.0 }, 0.0, false);
                d.view.target_y -= d.view.gap;
                d.view.clamp();
                d.view.scroll_y = d.view.target_y;
            }
            "actual_size" => {
                d.view.fit = Fit::None;
                d.view.set_zoom(1.0, 0.5);
            }
            "center_line" => {
                if let Some(r) = d.ruler {
                    let t = d.page_text(r.page);
                    if let Some(&(s, e)) = t.lines().get(r.line)
                        && let Some(rect) = t.span_rects(s, e).first()
                    {
                        d.view.goto(DocPos { page: r.page, y: rect.center_y() }, 0.5, animate);
                    }
                } else if let Some(h) = d.search.as_ref().and_then(|s| s.current.map(|c| s.hits[c].clone()))
                    && let Some(r) = h.rects.first()
                {
                    d.view.goto(DocPos { page: h.page, y: r.center_y() }, 0.5, animate);
                }
            }
            "search" | "search_backward" => {
                let backward = name == "search_backward";
                if args.is_empty() {
                    self.ask(if backward { "?" } else { "/" }, PromptKind::Search { backward }, String::new());
                } else {
                    let origin = d.view.pos_at(0.0);
                    d.start_search(args.to_string(), backward, origin, true);
                    self.emit("search", args.to_string());
                    return;
                }
            }
            "next_match" | "prev_match" => {
                let forward = (name == "next_match") != d.search.as_ref().is_some_and(|s| s.backward);
                for _ in 0..count.unwrap_or(1) {
                    step_match(d, forward, animate);
                }
                if d.search.is_none() {
                    notify_into(&mut self.toasts, "No active search", Level::Info);
                }
            }
            "search_selection" => {
                if let Some(t) = d.selection_text() {
                    let origin = d.view.pos_at(0.0);
                    d.selection = None;
                    self.mode = Mode::Normal;
                    let q = t.trim().to_string();
                    let d = self.doc.as_mut().unwrap();
                    d.start_search(q.clone(), false, origin, true);
                    self.search_history.push(q);
                } else {
                    self.notify("Nothing selected");
                }
            }
            "toc" => self.open_toc(),
            "toggle_sidebar" => self.show_sidebar = !self.show_sidebar,
            "set_mark" => {
                let Some(c) = args.chars().next() else { return };
                let pos = d.view.pos_at(0.0);
                if c.is_ascii_uppercase() {
                    self.global.marks.insert(c, (d.path.clone(), pos));
                    self.global.save();
                } else {
                    d.state.marks.insert(c, pos);
                    d.save_state();
                }
                self.notify(format!("Mark '{c}' set at page {}", pos.page + 1));
            }
            "goto_mark" => {
                let Some(c) = args.chars().next() else { return };
                if c == '\'' || c == '`' {
                    return self.run_builtin("jump_back", "", None);
                }
                if c.is_ascii_uppercase() {
                    if let Some((path, pos)) = self.global.marks.get(&c).cloned() {
                        if path != d.path {
                            self.open(&path);
                        }
                        if let Some(d) = &mut self.doc {
                            d.jump(pos, 0.0, false);
                        }
                    } else {
                        self.notify(format!("Mark '{c}' not set"));
                    }
                } else if let Some(pos) = d.state.marks.get(&c).copied() {
                    d.jump(pos, 0.0, false);
                } else {
                    self.notify(format!("Mark '{c}' not set"));
                }
            }
            "marks" => self.open_marks(),
            "jump_back" => {
                if d.hist_idx == d.history.len() {
                    d.push_history();
                    d.hist_idx = d.history.len().saturating_sub(1);
                }
                if d.hist_idx > 0 {
                    d.hist_idx -= 1;
                    let p = d.history[d.hist_idx];
                    d.view.goto(p, 0.0, false);
                } else {
                    self.notify("Already at the oldest position");
                }
            }
            "jump_forward" => {
                if d.hist_idx + 1 < d.history.len() {
                    d.hist_idx += 1;
                    let p = d.history[d.hist_idx];
                    d.view.goto(p, 0.0, false);
                } else {
                    self.notify("Already at the newest position");
                }
            }
            "follow_link" | "peek_link" => {
                let mode = if name == "peek_link" { HintMode::Peek } else { HintMode::Jump };
                self.start_link_hints(mode);
            }
            "smart_jump" | "smart_peek" => {
                let mode = if name == "smart_peek" { HintMode::Peek } else { HintMode::Jump };
                self.start_ref_hints(mode);
            }
            "toggle_dark" | "toggle_custom_colors" => {
                let target = if name == "toggle_dark" { PageMode::Dark } else { PageMode::Custom };
                d.page_mode = if d.page_mode == target { PageMode::Normal } else { target };
            }
            "page_mode" => {
                d.page_mode = match args {
                    "dark" => PageMode::Dark,
                    "custom" => PageMode::Custom,
                    _ => PageMode::Normal,
                };
            }
            "add_bookmark" => {
                if args.is_empty() {
                    let sec = d.current_section().map(|i| d.outline[i].title.clone()).unwrap_or_default();
                    return self.ask("bookmark", PromptKind::Bookmark, sec);
                }
                let pos = d.view.pos_at(0.0);
                d.state.bookmarks.push(Bookmark { pos, label: args.to_string(), created: store::now() });
                d.save_state();
                self.notify(format!("Bookmarked page {}", pos.page + 1));
            }
            "bookmarks" => self.open_bookmarks(),
            "visual" => {
                if self.start_visual() {
                    self.mode = Mode::Visual;
                    emit_selection = true;
                }
            }
            "exit_visual" => {
                d.selection = None;
                self.mode = Mode::Normal;
            }
            n if n.starts_with("sel_") => {
                if extend_selection(d, n) {
                    emit_selection = true;
                }
            }
            "yank" => match d.selection_text() {
                Some(t) => {
                    let len = t.chars().count();
                    self.ctx.copy_text(t);
                    let d = self.doc.as_mut().unwrap();
                    d.selection = None;
                    self.mode = Mode::Normal;
                    self.notify(format!("Copied {len} characters"));
                }
                None => self.notify("Nothing selected (v to select)"),
            },
            "yank_page" => {
                let page = d.view.current_page();
                let t = d.page_text(page);
                self.ctx.copy_text(t.span_text(0, t.chars.len()));
                self.notify(format!("Copied text of page {}", page + 1));
            }
            "highlight" | "annotate" => {
                if d.selection.is_none() {
                    return self.notify("Nothing selected (v to select)");
                }
                if name == "annotate" && args.is_empty() {
                    return self.ask("note", PromptKind::Annotate, String::new());
                }
                let color =
                    if name == "highlight" && !args.is_empty() { args.to_string() } else { self.theme.highlights[0].0.clone() };
                let note = if name == "annotate" { args.to_string() } else { String::new() };
                self.add_highlight(color, note);
            }
            "highlights" => self.open_highlights(),
            "export_highlights" => {
                let md = export_markdown(d);
                self.ctx.copy_text(md.clone());
                self.text_panel = TextPanel { title: "Highlights".into(), text: md, visible: true };
                self.notify("Highlights copied as Markdown");
            }
            "toggle_ruler" => {
                if d.ruler.is_some() {
                    d.ruler = None;
                } else {
                    let pos = d.view.pos_at(0.25);
                    let t = d.page_text(pos.page);
                    let line = t
                        .lines()
                        .iter()
                        .position(|&(s, e)| t.span_rects(s, e).first().is_some_and(|r| r.y1 >= pos.y))
                        .unwrap_or(0);
                    d.ruler = Some(Ruler { page: pos.page, line });
                }
            }
            "reload" => {
                if let Err(e) = d.reload() {
                    notify_into(&mut self.toasts, format!("reload failed: {e:#}"), Level::Error);
                } else {
                    self.notify("Reloaded");
                }
            }
            _ => self.warn(format!("'{name}' is not available here")),
        }
        if emit_selection && let Some(t) = self.doc.as_mut().and_then(|d| d.selection_text()) {
            self.emit("selection", t);
        }
        if let Some(d) = &mut self.doc {
            d.dirty = true;
        }
    }

    /// Open a web or mail link. Anything else (`file:`, custom schemes, paths) could launch files
    /// or apps from a PDF, so it's only copied to the clipboard.
    pub fn open_link(&mut self, target: &str) {
        let target = target.trim();
        if target.is_empty() {
            return self.warn("No URL given");
        }
        if is_web_url(target) {
            open_external(target);
            self.notify(format!("Opening {target}"));
        } else {
            self.ctx.copy_text(target.to_string());
            self.warn(format!("Not opening '{target}' (only http, https and mailto links open); copied it"));
        }
    }

    fn goto_page_str(&mut self, s: &str) {
        let Some(d) = &mut self.doc else { return };
        let s = s.trim();
        let page = match s.parse::<usize>() {
            Ok(n) => Some(n.saturating_sub(1)),
            // Page labels, e.g. roman numerals in front matter.
            Err(_) => (0..d.view.page_count()).find(|&i| d.backend.page_label(i).as_deref() == Some(s)),
        };
        match page {
            Some(p) if p < d.view.page_count() => d.jump(DocPos { page: p, y: 0.0 }, 0.0, false),
            _ => self.warn(format!("No page '{s}'")),
        }
    }

    fn add_highlight(&mut self, color: String, note: String) {
        let Some(d) = &mut self.doc else { return };
        let Some(sel) = d.selection else { return };
        let t = d.page_text(sel.page);
        let (s, e) = sel.range();
        let text = t.span_text(s, e);
        let rects = t.span_rects(s, e).into_iter().map(Into::into).collect();
        d.state.highlights.push(Highlight {
            page: sel.page,
            start: s,
            end: e,
            rects,
            text: text.clone(),
            color,
            note,
            created: store::now(),
        });
        d.state.highlights.sort_by_key(|h| (h.page, h.start));
        d.save_state();
        d.selection = None;
        self.mode = Mode::Normal;
        self.emit("highlight", text);
    }

    fn start_visual(&mut self) -> bool {
        let Some(d) = &mut self.doc else { return false };
        if d.selection.is_some() {
            return true;
        }
        // Start from the current search match, the ruler line, or the first word on screen.
        if let Some(h) = d.search.as_ref().and_then(|s| s.current.map(|c| s.hits[c].clone()))
            && h.rects.first().is_some_and(|r| d.view.is_visible(h.page, r.y0, r.y1))
        {
            d.selection = Some(Selection { page: h.page, anchor: h.start, cursor: h.end.saturating_sub(1) });
            return true;
        }
        if let Some(r) = d.ruler {
            let t = d.page_text(r.page);
            if let Some(&(s, e)) = t.lines().get(r.line) {
                d.selection = Some(Selection { page: r.page, anchor: s, cursor: e.saturating_sub(1).max(s) });
                return true;
            }
        }
        for page in d.view.visible_pages() {
            let t = d.page_text(page);
            let top = d.view.pos_at(0.0);
            let min_y = if page == top.page { top.y } else { 0.0 };
            if let Some(&(s, e)) = t.words().iter().find(|&&(s, _)| t.rects[s].y0 >= min_y) {
                d.selection = Some(Selection { page, anchor: s, cursor: e - 1 });
                return true;
            }
        }
        self.notify("No text on screen");
        false
    }

    // ------------------------------------------------------------------
    // Hints

    fn start_link_hints(&mut self, mode: HintMode) {
        let Some(d) = &mut self.doc else { return };
        let mut items = Vec::new();
        for page in d.view.visible_pages() {
            for link in d.page_links(page).iter() {
                if d.view.is_visible(page, link.rect.y0, link.rect.y1) || d.view.visible_pages().len() == 1 {
                    items.push(HintItem {
                        label: String::new(),
                        page,
                        rect: link.rect,
                        action: HintAction::Link(link.target.clone()),
                    });
                }
            }
        }
        self.show_hints(mode, items, "No links on screen");
    }

    fn start_ref_hints(&mut self, mode: HintMode) {
        let Some(d) = &mut self.doc else { return };
        let mut items = Vec::new();
        for page in d.view.visible_pages() {
            let text = d.page_text(page);
            for r in refs::detect(&text) {
                let Some(rect) = text.span_rects(r.start, r.end).into_iter().reduce(|a, b| a.union(&b)) else { continue };
                if !d.view.is_visible(page, rect.y0, rect.y1) {
                    continue;
                }
                items.push(HintItem {
                    label: String::new(),
                    page,
                    rect,
                    action: HintAction::Ref { kind: r.kind, label: r.label, page, char_idx: r.start },
                });
            }
        }
        self.show_hints(mode, items, "No references on screen");
    }

    fn show_hints(&mut self, mode: HintMode, mut items: Vec<HintItem>, empty: &str) {
        if items.is_empty() {
            return self.notify(empty);
        }
        items.sort_by_key(|a| (a.page, a.rect.y0 as i32, a.rect.x0 as i32));
        let labels = hint_labels(items.len(), &self.cfg.general.hint_chars);
        for (it, l) in items.iter_mut().zip(labels) {
            it.label = l;
        }
        if let Some(d) = &mut self.doc {
            d.hints = Some(Hints { mode, items, typed: String::new() });
        }
    }

    /// Feed a typed char to the active hints. Uppercase labels peek instead of jumping.
    pub fn hint_input(&mut self, c: char) {
        let Some(d) = &mut self.doc else { return };
        let Some(h) = &mut d.hints else { return };
        let peek_override = c.is_uppercase();
        h.typed.push(c.to_ascii_lowercase());
        let matches: Vec<&HintItem> = h.items.iter().filter(|i| i.label.starts_with(&h.typed)).collect();
        if matches.is_empty() {
            d.hints = None;
            return;
        }
        if matches.len() == 1 && matches[0].label == h.typed {
            let item = matches[0].clone();
            let mode = if peek_override { HintMode::Peek } else { h.mode };
            d.hints = None;
            self.activate_hint(item, mode);
        }
    }

    fn activate_hint(&mut self, item: HintItem, mode: HintMode) {
        let Some(d) = &mut self.doc else { return };
        match item.action {
            HintAction::Link(LinkTarget::Page(pos)) => {
                if mode == HintMode::Peek {
                    let title = format!("Link → page {}", pos.page + 1);
                    open_peek(d, pos, None, title);
                } else {
                    d.jump(pos, 0.1, false);
                }
            }
            HintAction::Link(LinkTarget::Uri(uri)) => {
                self.ctx.copy_text(uri.clone());
                self.open_link(&uri);
            }
            HintAction::Link(LinkTarget::Other(s)) => self.notify(format!("Unsupported link: {s}")),
            HintAction::Ref { kind, label, page, char_idx } => {
                let display = format!("{} {}", kind.name(), label);
                if kind == RefKind::Section {
                    let prefix = label.trim_end_matches('.').to_string();
                    let hit = d.outline.iter().find(|o| {
                        o.title.split_whitespace().next().is_some_and(|w| w.trim_end_matches('.') == prefix)
                            || o.title.starts_with(&format!("Appendix {prefix}"))
                    });
                    if let Some(pos) = hit.and_then(|o| o.target) {
                        if mode == HintMode::Peek {
                            open_peek(d, pos, None, display);
                        } else {
                            d.jump(pos, 0.05, false);
                        }
                        return;
                    }
                }
                let id = d.next_ref_id;
                d.next_ref_id += 1;
                d.pending_ref = Some((id, mode == HintMode::Peek, display.clone()));
                d.text.send(crate::text::Request::FindRef { id, kind, label, from_page: page, from_char: char_idx });
                self.notify(format!("Looking up {display}…"));
            }
        }
    }

    pub fn on_ref_result(&mut self, id: u64, target: Option<(usize, Rect)>) {
        let Some(d) = &mut self.doc else { return };
        let Some((pending, peek, label)) = d.pending_ref.clone() else { return };
        if pending != id {
            return;
        }
        d.pending_ref = None;
        match target {
            Some((page, rect)) => {
                let pos = DocPos { page, y: rect.y0 };
                if peek {
                    open_peek(d, pos, Some(rect), label);
                } else {
                    d.jump(pos, 0.3, false);
                }
                self.toasts.retain(|t| !t.text.starts_with("Looking up"));
            }
            None => self.warn(format!("Couldn't find {label}")),
        }
    }

    // ------------------------------------------------------------------
    // Pickers

    fn open_command_palette(&mut self, initial: &str) {
        let mut items: Vec<PickItem> = BUILTINS
            .iter()
            .map(|c| {
                let label = match c.arg {
                    Some(a) => format!("{} ‹{a}›", c.name),
                    None => c.name.to_string(),
                };
                PickItem::new(label, PickData::Command(c.name.to_string())).detail(c.description).hint(self.keys_for(c.name))
            })
            .collect();
        if let Some(s) = &self.script {
            for (name, desc) in s.commands() {
                if name.starts_with("lua:") {
                    continue;
                }
                items.push(PickItem::new(name.clone(), PickData::Command(name.clone())).detail(desc).hint(self.keys_for(&name)));
            }
        }
        let mut p = Picker::new(PickerKind::Commands, "Command", items);
        p.placeholder = "command  (args)   ·   a number jumps to that page".into();
        p.query = initial.to_string();
        p.refilter();
        self.picker = Some(p);
    }

    fn open_help(&mut self) {
        let mut items = Vec::new();
        for mode in ["normal", "visual"] {
            if let Some(m) = self.keymaps.get(mode) {
                for (cmd, keys) in m.sorted() {
                    let desc = commands::builtin(commands::split(&cmd).0)
                        .map(|b| b.description.to_string())
                        .or_else(|| {
                            self.script.as_ref().and_then(|s| s.commands().into_iter().find(|(n, _)| *n == cmd).map(|(_, d)| d))
                        })
                        .unwrap_or_default();
                    let label = if mode == "normal" { cmd.clone() } else { format!("{cmd}  [visual]") };
                    items.push(PickItem::new(label, PickData::Command(cmd.clone())).detail(desc).hint(keys.join("  ")));
                }
            }
        }
        let mut p = Picker::new(PickerKind::Help, "Key bindings", items);
        p.placeholder = "Search bindings… (Enter runs the command)".into();
        self.picker = Some(p);
    }

    fn open_toc(&mut self) {
        let Some(d) = &self.doc else { return };
        if d.outline.is_empty() {
            return self.notify("This document has no outline");
        }
        let items = d
            .outline
            .iter()
            .map(|o| {
                let hint = o
                    .target
                    .map(|t| d.backend.page_label(t.page).unwrap_or_else(|| (t.page + 1).to_string()))
                    .unwrap_or_default();
                PickItem::new(o.title.clone(), o.target.map(PickData::Pos).unwrap_or(PickData::None))
                    .hint(hint)
                    .depth(o.depth.min(6))
            })
            .collect();
        let cur = d.current_section();
        let mut p = Picker::new(PickerKind::Toc, "Contents", items);
        p.keep_order = true;
        if let Some(c) = cur {
            p.select_item(c);
        }
        self.picker = Some(p);
    }

    fn open_bookmarks(&mut self) {
        let Some(d) = &self.doc else { return };
        let items: Vec<PickItem> = d
            .state
            .bookmarks
            .iter()
            .enumerate()
            .map(|(i, b)| PickItem::new(b.label.clone(), PickData::Bookmark(i)).hint(format!("p. {}", b.pos.page + 1)))
            .collect();
        if items.is_empty() {
            return self.notify("No bookmarks yet (b to add)");
        }
        self.picker = Some(Picker::new(PickerKind::Bookmarks, "Bookmarks  ·  Ctrl-D deletes", items));
    }

    fn open_highlights(&mut self) {
        let Some(d) = &self.doc else { return };
        let items: Vec<PickItem> = d
            .state
            .highlights
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let label: String = h.text.chars().take(90).collect();
                PickItem::new(label, PickData::Highlight(i)).detail(h.note.clone()).hint(format!("p. {}", h.page + 1))
            })
            .collect();
        if items.is_empty() {
            return self.notify("No highlights yet (v to select, H to highlight)");
        }
        self.picker = Some(Picker::new(PickerKind::Highlights, "Highlights  ·  Ctrl-D deletes", items));
    }

    fn open_marks(&mut self) {
        let mut items = Vec::new();
        if let Some(d) = &self.doc {
            for (c, p) in &d.state.marks {
                items.push(PickItem::new(format!("'{c}"), PickData::Mark(*c)).hint(format!("p. {}", p.page + 1)));
            }
        }
        for (c, (path, p)) in &self.global.marks {
            let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            items.push(PickItem::new(format!("'{c}"), PickData::Mark(*c)).detail(name).hint(format!("p. {}", p.page + 1)));
        }
        if items.is_empty() {
            return self.notify("No marks (m{letter} sets one)");
        }
        self.picker = Some(Picker::new(PickerKind::Marks, "Marks", items));
    }

    pub fn open_recent_picker(&mut self) {
        let items: Vec<PickItem> = self
            .global
            .recent
            .iter()
            .filter(|r| r.path.exists())
            .map(|r| {
                let dir = r.path.parent().map(tilde).unwrap_or_default();
                PickItem::new(r.title.clone(), PickData::Path(r.path.clone())).detail(dir).hint(ago(r.opened))
            })
            .collect();
        if items.is_empty() {
            if self.doc.is_some() {
                self.notify("No recent documents");
            }
            return;
        }
        self.picker = Some(Picker::new(PickerKind::Recent, "Recent", items));
    }

    fn open_file_picker(&mut self) {
        let dir = self
            .doc
            .as_ref()
            .and_then(|d| d.path.parent().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut p = Picker::new(PickerKind::Files, "Open", Vec::new());
        p.query = format!("{}/", tilde(&dir));
        p.placeholder = "path/to/file.pdf  ·  Tab completes".into();
        refresh_files(&mut p);
        self.picker = Some(p);
    }

    pub fn picker_event(&mut self, ev: PickerEvent) {
        let Some(p) = &mut self.picker else { return };
        match ev {
            PickerEvent::None => {}
            PickerEvent::Close => {
                let kind = p.kind;
                self.picker = None;
                if kind == PickerKind::Lua
                    && let Some(cb) = self.lua_picker_cb.take()
                {
                    self.with_script(|s| s.call_fn(cb, ()));
                }
            }
            PickerEvent::QueryChanged => {
                if p.kind == PickerKind::Files {
                    refresh_files(p);
                }
            }
            PickerEvent::Complete => {
                if p.kind == PickerKind::Files {
                    if let Some(PickData::Path(path)) = p.current().map(|i| i.data.clone()) {
                        p.query = tilde(&path) + if path.is_dir() { "/" } else { "" };
                        refresh_files(p);
                        p.query_cursor_end();
                    }
                } else if let Some(cur) = p.current().map(|i| i.data.clone())
                    && let PickData::Command(c) = cur
                {
                    p.query = format!("{c} ");
                    p.refilter();
                    p.query_cursor_end();
                }
            }
            PickerEvent::Delete => {
                let kind = p.kind;
                let cur = p.current().map(|i| i.data.clone());
                let Some(d) = &mut self.doc else { return };
                match (kind, cur) {
                    (PickerKind::Highlights, Some(PickData::Highlight(i))) => {
                        d.state.highlights.remove(i);
                        d.save_state();
                        self.picker = None;
                        self.open_highlights();
                    }
                    (PickerKind::Bookmarks, Some(PickData::Bookmark(i))) => {
                        d.state.bookmarks.remove(i);
                        d.save_state();
                        self.picker = None;
                        self.open_bookmarks();
                    }
                    _ => {}
                }
            }
            PickerEvent::Accept => {
                let p = self.picker.take().unwrap();
                let query = p.query.trim().to_string();
                let cur = p.current().cloned();
                match p.kind {
                    PickerKind::Commands => {
                        let (cmd, args) = commands::split(&query);
                        if !args.is_empty() || cmd.chars().all(|c| c.is_ascii_digit()) && !cmd.is_empty() {
                            self.run_line(&query, None);
                        } else if let Some(PickData::Command(c)) = cur.map(|c| c.data) {
                            self.run_line(&c, None);
                        } else if !query.is_empty() {
                            self.run_line(&query, None);
                        }
                    }
                    PickerKind::Files => {
                        let typed = shellexpand(&query);
                        let chosen = match cur.map(|c| c.data) {
                            Some(PickData::Path(path)) => path,
                            _ => typed.clone(),
                        };
                        let target = if typed.is_file() { typed } else { chosen };
                        if target.is_dir() {
                            let mut p = p;
                            p.query = tilde(&target) + "/";
                            refresh_files(&mut p);
                            p.query_cursor_end();
                            self.picker = Some(p);
                        } else if target.is_file() {
                            self.open(&target);
                        }
                    }
                    _ => {
                        if let Some(item) = cur {
                            if let PickData::Lua(cb, idx) = item.data {
                                self.lua_picker_cb = None;
                                self.with_script(|s| s.call_fn(cb, ((idx + 1) as i64, item.label)));
                            } else {
                                self.accept_item(item.data);
                            }
                        }
                    }
                }
            }
        }
    }

    fn accept_item(&mut self, data: PickData) {
        match data {
            PickData::Command(c) => self.run_line(&c, None),
            PickData::Pos(pos) => {
                if let Some(d) = &mut self.doc {
                    d.jump(pos, 0.03, false);
                }
            }
            PickData::Path(p) => self.open(&p),
            PickData::Highlight(i) => {
                if let Some(d) = &mut self.doc
                    && let Some(h) = d.state.highlights.get(i)
                {
                    let y = h.rects.first().map(|r| Rect::from(r).y0).unwrap_or(0.0);
                    let pos = DocPos { page: h.page, y };
                    d.jump(pos, 0.3, false);
                }
            }
            PickData::Bookmark(i) => {
                if let Some(d) = &mut self.doc
                    && let Some(b) = d.state.bookmarks.get(i)
                {
                    let pos = b.pos;
                    d.jump(pos, 0.0, false);
                }
            }
            PickData::Mark(c) => self.run_line(&format!("goto_mark {c}"), None),
            PickData::Theme(t) => self.set_theme(&t),
            PickData::Lua(..) => {} // handled in picker_event, which still has the label
            PickData::None => {}
        }
    }

    // ------------------------------------------------------------------
    // Prompt

    pub fn prompt_changed(&mut self) {
        let Some(p) = &self.prompt else { return };
        if let PromptKind::Search { backward } = p.kind {
            let (q, origin) = (p.text.clone(), p.origin);
            if let Some(d) = &mut self.doc {
                if q.chars().count() >= 2 {
                    d.start_search(q, backward, origin.unwrap_or_default(), true);
                } else {
                    d.search = None;
                }
            }
        }
    }

    pub fn prompt_accept(&mut self) {
        let Some(p) = self.prompt.take() else { return };
        let text = p.text.trim().to_string();
        match p.kind {
            PromptKind::Search { backward } => {
                if text.is_empty() {
                    return;
                }
                if !self.search_history.ends_with(std::slice::from_ref(&text)) {
                    self.search_history.push(text.clone());
                }
                let same = self.doc.as_ref().and_then(|d| d.search.as_ref()).is_some_and(|s| s.query == text);
                if let Some(d) = &mut self.doc {
                    if !same {
                        d.start_search(text.clone(), backward, p.origin.unwrap_or_default(), true);
                    }
                    if let Some(origin) = p.origin {
                        // Record where we came from so <C-o> returns there.
                        d.history.truncate(d.hist_idx);
                        d.history.push(origin);
                        d.hist_idx = d.history.len();
                    }
                }
                self.emit("search", text);
            }
            PromptKind::GotoPage => self.goto_page_str(&text),
            PromptKind::CommandArg(cmd) => {
                if !text.is_empty() {
                    self.run_line(&format!("{cmd} {text}"), None);
                }
            }
            PromptKind::Lua(cb) => self.with_script(|s| s.call_fn(cb, text)),
            PromptKind::Bookmark => {
                self.run_line(&format!("add_bookmark {}", if text.is_empty() { "Bookmark" } else { &text }), None)
            }
            PromptKind::Annotate => {
                let color = self.theme.highlights.get(1).map(|h| h.0.clone()).unwrap_or_else(|| "yellow".into());
                self.add_highlight(color, text);
            }
        }
    }

    pub fn prompt_cancel(&mut self) {
        let Some(p) = self.prompt.take() else { return };
        if let PromptKind::Lua(cb) = p.kind {
            // Tell the script the user canceled.
            self.with_script(|s| s.call_fn(cb, ()));
            return;
        }
        if let PromptKind::Search { .. } = p.kind
            && let Some(d) = &mut self.doc
        {
            d.search = None;
            d.text.cancel_search();
            if let Some(o) = p.origin {
                d.view.goto(o, 0.0, false);
            }
        }
    }

    pub fn prompt_history(&mut self, up: bool) {
        let Some(p) = &mut self.prompt else { return };
        if !matches!(p.kind, PromptKind::Search { .. }) || self.search_history.is_empty() {
            return;
        }
        let n = self.search_history.len();
        let idx = match (p.history_idx, up) {
            (None, true) => n - 1,
            (Some(i), true) => i.saturating_sub(1),
            (Some(i), false) if i + 1 < n => i + 1,
            _ => return,
        };
        p.history_idx = Some(idx);
        p.text = self.search_history[idx].clone();
        self.prompt_changed();
    }
}

// ----------------------------------------------------------------------
// Helpers operating on a session

pub fn open_peek(d: &mut Session, dest: crate::doc::DocPos, target: Option<Rect>, title: String) {
    let (w, _) = d.view.page_size(dest.page);
    let zoom = d.view.zoom.min(d.view.viewport.0 * 0.7 / w.max(1.0));
    d.peek = Some(Peek { page: dest.page, y: (dest.y - 40.0).max(0.0), zoom, target, title, dest });
}

fn step_match(d: &mut Session, forward: bool, animate: bool) {
    let Some(s) = &mut d.search else { return };
    if s.hits.is_empty() {
        return;
    }
    let n = s.hits.len();
    let next = match s.current {
        Some(c) if forward => (c + 1) % n,
        Some(c) => (c + n - 1) % n,
        None => {
            let pos = d.view.pos_at(0.0);
            let after = |h: &crate::text::SearchHit| (h.page, h.rects.first().map(|r| r.y0).unwrap_or(0.0)) > (pos.page, pos.y);
            if forward {
                s.hits.iter().position(after).unwrap_or(0)
            } else {
                s.hits.iter().rposition(|h| !after(h)).unwrap_or(n - 1)
            }
        }
    };
    s.current = Some(next);
    let h = s.hits[next].clone();
    reveal(d, h.page, h.rects.first().copied(), animate);
}

/// Scroll so a rect is comfortably visible, jumping only if needed.
pub fn reveal(d: &mut Session, page: usize, rect: Option<Rect>, animate: bool) {
    let r = rect.unwrap_or_default();
    if !d.view.is_visible(page, r.y0 - 10.0, r.y1 + 10.0) {
        let far = (d.view.page_top(page) - d.view.target_y).abs() > d.view.viewport.1 * 2.0;
        if far {
            d.push_history();
        }
        d.view.goto(DocPos { page, y: r.y0 }, 0.35, animate && !far);
    }
    // Horizontal: when zoomed in, keep the match on screen.
    let (sx, _) = d.view.doc_to_screen(page, r.x0, r.y0);
    let vw = d.view.viewport.0;
    if sx < 0.0 || sx > vw - 40.0 {
        d.view.scroll_x += vw * 0.4 - sx;
        d.view.clamp();
    }
}

fn move_ruler(d: &mut Session, delta: i64, animate: bool) {
    let Some(mut r) = d.ruler else { return };
    let mut remaining = delta;
    while remaining != 0 {
        let lines = d.page_text(r.page).lines().len();
        if remaining > 0 {
            if r.line + 1 < lines {
                r.line += 1;
            } else if r.page + 1 < d.view.page_count() {
                r.page += 1;
                r.line = 0;
                // Skip pages without text.
                while d.page_text(r.page).lines().is_empty() && r.page + 1 < d.view.page_count() {
                    r.page += 1;
                }
            }
            remaining -= 1;
        } else {
            if r.line > 0 {
                r.line -= 1;
            } else if r.page > 0 {
                r.page -= 1;
                while d.page_text(r.page).lines().is_empty() && r.page > 0 {
                    r.page -= 1;
                }
                r.line = d.page_text(r.page).lines().len().saturating_sub(1);
            }
            remaining += 1;
        }
    }
    d.ruler = Some(r);
    let t = d.page_text(r.page);
    if let Some(&(s, e)) = t.lines().get(r.line)
        && let Some(rect) = t.span_rects(s, e).into_iter().reduce(|a, b| a.union(&b))
    {
        let top = d.view.page_top(r.page) + rect.y0 * d.view.zoom - d.view.target_y;
        let bottom = d.view.page_top(r.page) + rect.y1 * d.view.zoom - d.view.target_y;
        let vh = d.view.viewport.1;
        if bottom > vh * 0.75 || top < vh * 0.1 {
            d.view.goto(DocPos { page: r.page, y: rect.y0 }, if delta > 0 { 0.25 } else { 0.6 }, animate);
        }
    }
}

fn extend_selection(d: &mut Session, cmd: &str) -> bool {
    let Some(mut sel) = d.selection else { return false };
    let t = d.page_text(sel.page);
    let n = t.chars.len();
    if n == 0 {
        return false;
    }
    let cur = sel.cursor.min(n - 1);
    let words = t.words();
    let lines = t.lines();
    let line_of = |i: usize| lines.iter().position(|&(s, e)| i >= s && i < e.max(s + 1)).unwrap_or(0);
    match cmd {
        "sel_word_next" => {
            if let Some(&(_, e)) = words.iter().find(|&&(_, e)| e - 1 > cur) {
                sel.cursor = e - 1;
            }
        }
        "sel_word_prev" => {
            if let Some(&(s, _)) = words.iter().rev().find(|&&(s, _)| s < cur) {
                sel.cursor = s;
            }
        }
        "sel_char_next" => {
            let mut i = cur + 1;
            while i < n && t.chars[i] == '\n' {
                i += 1;
            }
            if i < n {
                sel.cursor = i;
            }
        }
        "sel_char_prev" => {
            let mut i = cur;
            while i > 0 {
                i -= 1;
                if t.chars[i] != '\n' {
                    sel.cursor = i;
                    break;
                }
            }
        }
        "sel_line_next" | "sel_line_prev" => {
            let li = line_of(cur);
            let target = if cmd == "sel_line_next" { li + 1 } else { li.wrapping_sub(1) };
            if let Some(&(s, e)) = lines.get(target) {
                let x = t.rects[cur].x1;
                let best = (s..e).min_by(|&a, &b| {
                    (t.rects[a].x1 - x).abs().partial_cmp(&(t.rects[b].x1 - x).abs()).unwrap_or(std::cmp::Ordering::Equal)
                });
                sel.cursor = best.unwrap_or(s);
            }
        }
        "sel_line_end" => {
            if let Some(&(_, e)) = lines.get(line_of(cur)) {
                sel.cursor = e.saturating_sub(1);
            }
        }
        "sel_line_start" => {
            if let Some(&(s, _)) = lines.get(line_of(cur)) {
                sel.cursor = s;
            }
        }
        "sel_sentence_next" => {
            let mut i = cur + 1;
            while i < n {
                if matches!(t.chars[i], '.' | '?' | '!') && t.chars.get(i + 1).is_none_or(|c| c.is_whitespace()) {
                    break;
                }
                i += 1;
            }
            sel.cursor = i.min(n - 1);
        }
        "sel_swap" => std::mem::swap(&mut sel.anchor, &mut sel.cursor),
        _ => return false,
    }
    d.selection = Some(sel);
    let r = t.rects[sel.cursor];
    reveal(d, sel.page, Some(r), true);
    true
}

fn export_markdown(d: &Session) -> String {
    let mut md = format!("# {}\n\n", d.title);
    for h in &d.state.highlights {
        md += &format!("> {}\n\n— p. {}", h.text.trim(), h.page + 1);
        if !h.note.is_empty() {
            md += &format!("\n\n{}", h.note);
        }
        md += "\n\n";
    }
    if !d.state.bookmarks.is_empty() {
        md += "## Bookmarks\n\n";
        for b in &d.state.bookmarks {
            md += &format!("- {} (p. {})\n", b.label, b.pos.page + 1);
        }
    }
    md
}

fn refresh_files(p: &mut Picker) {
    let q = shellexpand(&p.query);
    let q_str = p.query.clone();
    let dir =
        if q_str.ends_with('/') { q.clone() } else { q.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/")) };
    let mut items = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name().to_string_lossy().to_lowercase()));
        for e in entries {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && !q_str.rsplit('/').next().unwrap_or("").starts_with('.') {
                continue;
            }
            let path = e.path();
            let is_dir = path.is_dir();
            let is_doc = name.to_lowercase().ends_with(".pdf");
            if !is_dir && !is_doc {
                continue;
            }
            let label = if is_dir { format!("{name}/") } else { name };
            let hint = if is_dir { String::new() } else { human_size(e.metadata().map(|m| m.len()).unwrap_or(0)) };
            items.push(PickItem::new(label, PickData::Path(path)).hint(hint));
        }
    }
    p.set_items(items);
}

fn human_size(b: u64) -> String {
    match b {
        b if b > 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KB", b / 1024),
    }
}

fn save_png(path: &Path, img: &egui::ColorImage) -> anyhow::Result<()> {
    let [w, h] = img.size;
    let mut surface = cairo::ImageSurface::create(cairo::Format::Rgb24, w as i32, h as i32)?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data()?;
        for (i, px) in img.pixels.iter().enumerate() {
            let (x, y) = (i % w, i / w);
            let v = u32::from_be_bytes([0, px.r(), px.g(), px.b()]).to_ne_bytes();
            data[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&v);
        }
    }
    let mut f = std::fs::File::create(path)?;
    surface.write_to_png(&mut f)?;
    Ok(())
}

fn is_web_url(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    ["http://", "https://", "mailto:"].iter().any(|p| l.starts_with(p))
}

/// Open a URL with the desktop's default handler. Callers check the scheme ([`App::open_link`]).
fn open_external(target: &str) {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    if let Err(e) = std::process::Command::new(opener).arg(target).spawn() {
        eprintln!("folio: {opener} failed: {e}");
    }
}

pub fn shellexpand(s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("~")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(format!("{}{rest}", home.to_string_lossy()));
    }
    PathBuf::from(s)
}

pub fn tilde(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    match std::env::var("HOME") {
        Ok(home) if s.starts_with(&home) => format!("~{}", &s[home.len()..]),
        _ => s,
    }
}

fn ago(t: u64) -> String {
    let d = store::now().saturating_sub(t);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", d / 60),
        3600..=86399 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86400),
    }
}

impl Picker {
    pub fn query_cursor_end(&mut self) {
        self.move_cursor_end = true;
    }
}
