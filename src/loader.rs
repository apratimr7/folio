//! Opening documents in the background, so a long book doesn't stall the window.
//!
//! Requests queue up and are read one at a time on a helper thread ([`Prepared::load`]: every page's size, the
//! outline, the title). The UI thread then builds the session quickly and puts it where the request said: the
//! current tab, a new tab, or the restored tab it was waiting for. One at a time keeps tabs in the order asked.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossbeam_channel::Receiver;

use crate::app::{App, Mode, PromptKind};
use crate::doc::{DocPos, OpenError};
use crate::script::Level;
use crate::session::{Prepared, Session};
use crate::tabs::Tab;

/// Where an opened document goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Replaces the current tab's document.
    Current,
    NewTab,
    /// Fills the restored tab with this path (shown if it's the active one).
    Lazy,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub path: PathBuf,
    pub password: Option<String>,
    pub target: Target,
    /// Where to go once it's open (a global mark), instead of the saved position.
    pub then: Option<DocPos>,
}

type Pending = (Request, Receiver<anyhow::Result<Prepared>>, Instant);

#[derive(Default)]
pub struct Loader {
    queue: VecDeque<Request>,
    busy: Option<Pending>,
}

impl Loader {
    /// The file being read now, if any.
    pub fn loading(&self) -> Option<&Path> {
        self.busy.as_ref().map(|(r, _, _)| r.path.as_path())
    }

    pub fn is_idle(&self) -> bool {
        self.busy.is_none() && self.queue.is_empty()
    }
}

impl App {
    pub fn request_open(&mut self, path: &Path, password: Option<String>, target: Target, then: Option<DocPos>) {
        self.loader.queue.push_back(Request { path: path.to_path_buf(), password, target, then });
        self.pump_loader();
    }

    fn pump_loader(&mut self) {
        if self.loader.busy.is_some() {
            return;
        }
        let Some(req) = self.loader.queue.pop_front() else { return };
        let (tx, rx) = crossbeam_channel::bounded(1);
        let (path, password, ctx) = (req.path.clone(), req.password.clone(), self.ctx.clone());
        let spawned = std::thread::Builder::new().name("open".into()).spawn(move || {
            let _ = tx.send(Prepared::load(&path, password));
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.loader.busy = Some((req, rx, Instant::now())),
            Err(e) => self.open_failed(req, anyhow::anyhow!("couldn't start reading the file: {e}")),
        }
    }

    /// Wait for every queued document (for `--remote`, whose callers expect a command to have finished).
    pub fn finish_loading_now(&mut self) {
        while let Some((_, rx, _)) = &self.loader.busy {
            let result = rx.recv().unwrap_or_else(|_| Err(anyhow::anyhow!("reading the file stopped")));
            let (req, _, _) = self.loader.busy.take().unwrap();
            match result.and_then(|p| Session::from_prepared(p, &self.cfg, &self.theme, &self.ctx)) {
                Ok(s) => self.place_opened(req, s),
                Err(e) => self.open_failed(req, e),
            }
            self.pump_loader();
        }
    }

    /// Finish a document that has been read, then start the next request.
    pub fn poll_loader(&mut self) {
        let done = match &self.loader.busy {
            Some((_, rx, _)) => rx.try_recv().ok(),
            None => None,
        };
        if let Some(result) = done {
            let (req, _, _) = self.loader.busy.take().unwrap();
            match result.and_then(|p| Session::from_prepared(p, &self.cfg, &self.theme, &self.ctx)) {
                Ok(s) => self.place_opened(req, s),
                Err(e) => self.open_failed(req, e),
            }
            self.pump_loader();
        }
        if self.loader.busy.is_some() {
            // The status bar shows "opening…"; keep it current.
            self.ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn place_opened(&mut self, req: Request, mut s: Session) {
        if let Some(pos) = req.then {
            s.state.pos = Some(pos);
        }
        match req.target {
            Target::Lazy => {
                // Restored tabs hold the path as it was saved, which can differ from the canonical one.
                if self.doc.is_none() && self.active_pending.as_deref() == Some(req.path.as_path()) {
                    self.place_in_current_tab(s);
                } else {
                    // Not shown any more: keep it loaded in its tab, if that's still open.
                    if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Lazy(p) if *p == req.path)) {
                        self.tabs[i] = Tab::Loaded(Box::new(s));
                    }
                    return;
                }
            }
            Target::Current | Target::NewTab => {
                // Opened in the meantime (asked for twice): show that tab.
                if let Some(i) = self.tab_of(&s.path) {
                    self.switch_tab(i);
                    if let (Some(pos), Some(d)) = (req.then, &mut self.doc) {
                        d.jump(pos, 0.0, false);
                    }
                    return;
                }
                // A new tab also when the active one is a restored tab still waiting for its document.
                if req.target == Target::NewTab && (self.doc.is_some() || self.active_pending.is_some()) {
                    self.insert_tab_after_active();
                } else if let Some(d) = &mut self.doc {
                    d.save_state();
                }
                self.place_in_current_tab(s);
            }
        }
        let Some(d) = &self.doc else { return };
        let (path, title, page) = (d.path.clone(), d.title.clone(), d.state.pos.map(|p| p.page).unwrap_or(0));
        self.global.touch_recent(&path, &title, page);
        self.global.save();
        self.mode = Mode::Normal;
        self.picker = None;
        self.after_switch();
        self.emit("open", path.to_string_lossy().to_string());
    }

    fn open_failed(&mut self, req: Request, e: anyhow::Error) {
        if let Some(err) = e.downcast_ref::<OpenError>() {
            if *err == OpenError::WrongPassword {
                self.warn("Wrong password");
            }
            let name = req.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            self.ask(&format!("password for {name}"), PromptKind::Password(Request { password: None, ..req }), String::new());
            return;
        }
        crate::app::notify_into(&mut self.toasts, format!("{}: {e:#}", req.path.display()), Level::Error);
        if req.target == Target::Lazy {
            self.drop_lazy_tab(&req.path);
        }
    }

    /// A restored tab whose file can't be opened (or whose password was refused): close it.
    pub(crate) fn drop_lazy_tab(&mut self, path: &Path) {
        if self.doc.is_none() && self.active_pending.as_deref() == Some(path) {
            self.active_pending = None;
            let i = self.active_tab;
            if i < self.tabs.len() {
                self.tabs.remove(i);
            }
            self.activate(i);
        } else if let Some(i) = self.tabs.iter().position(|t| matches!(t, Tab::Lazy(p) if p == path)) {
            self.tabs.remove(i);
            if i < self.active_tab {
                self.active_tab -= 1;
            }
        }
    }
}
