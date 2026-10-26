//! Native file dialogs: the system's own open and save panels, through `rfd`.
//!
//! A dialog is created on the UI thread (macOS then shows it as a sheet on the window) and awaited on a helper
//! thread, so the window keeps drawing while it's open. The chosen path comes back over a channel. On Linux this
//! needs the XDG desktop portal (GNOME, KDE and most desktops have it); without one the dialog closes at once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender};

/// What a chosen path is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Purpose {
    Open { new_tab: bool },
    ExportHighlights,
    Screenshot,
}

pub struct Dialogs {
    tx: Sender<(Purpose, PathBuf)>,
    pub rx: Receiver<(Purpose, PathBuf)>,
    open: Arc<AtomicBool>,
}

impl Dialogs {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx, open: Arc::new(AtomicBool::new(false)) }
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    /// Ask for a PDF to open, starting in `dir`.
    pub fn pick_pdf(&self, dir: Option<&Path>, new_tab: bool, wake: impl Fn() + Send + 'static) {
        let mut d = rfd::AsyncFileDialog::new().set_title("Open").add_filter("PDF documents", &["pdf"]);
        if let Some(dir) = dir {
            d = d.set_directory(dir);
        }
        self.run(d.pick_file(), Purpose::Open { new_tab }, wake);
    }

    /// Ask where to save a file, suggesting `dir` and `name`; `kind` is (description, extension).
    pub fn save(&self, dir: Option<&Path>, name: &str, kind: (&str, &str), purpose: Purpose, wake: impl Fn() + Send + 'static) {
        let mut d = rfd::AsyncFileDialog::new().set_title("Save").set_file_name(name).add_filter(kind.0, &[kind.1]);
        if let Some(dir) = dir {
            d = d.set_directory(dir);
        }
        self.run(d.save_file(), purpose, wake);
    }

    fn run(
        &self,
        dialog: impl Future<Output = Option<rfd::FileHandle>> + Send + 'static,
        purpose: Purpose,
        wake: impl Fn() + Send + 'static,
    ) {
        let (tx, open) = (self.tx.clone(), self.open.clone());
        open.store(true, Ordering::SeqCst);
        let spawned = std::thread::Builder::new().name("dialog".into()).spawn(move || {
            if let Some(file) = pollster::block_on(dialog) {
                let _ = tx.send((purpose, file.path().to_path_buf()));
            }
            open.store(false, Ordering::SeqCst);
            wake();
        });
        if spawned.is_err() {
            self.open.store(false, Ordering::SeqCst);
        }
    }
}
