//! Remote control over a Unix socket.
//!
//! `folio --remote "goto_page 12"` sends command lines to the most recently
//! started instance. Editors can use this for forward search, and external
//! tools (or AI agents) can drive the reader the same way keys do.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

pub fn socket_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// The "current instance" socket, a symlink to the per-process one.
pub fn default_socket() -> PathBuf {
    socket_dir().join("folio.sock")
}

pub struct Server {
    pub rx: Receiver<String>,
    path: PathBuf,
}

impl Server {
    pub fn start(wake: impl Fn() + Send + 'static) -> Result<Self> {
        cleanup_stale();
        let path = socket_dir().join(format!("folio-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
        let link = default_socket();
        let _ = std::fs::remove_file(&link);
        let _ = std::os::unix::fs::symlink(&path, &link);
        let (tx, rx) = crossbeam_channel::unbounded();
        std::thread::Builder::new().name("ipc".into()).spawn(move || serve(listener, tx, wake))?;
        Ok(Self { rx, path })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let link = default_socket();
        if std::fs::read_link(&link).ok().as_ref() == Some(&self.path) {
            let _ = std::fs::remove_file(link);
        }
    }
}

/// Remove sockets left behind by instances that were killed.
fn cleanup_stale() {
    // Liveness is checked through /proc, which only Linux has.
    if !cfg!(target_os = "linux") {
        return;
    }
    let Ok(entries) = std::fs::read_dir(socket_dir()) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(pid) = name.strip_prefix("folio-").and_then(|n| n.strip_suffix(".sock")) else { continue };
        if pid.parse::<u32>().is_ok() && !std::path::Path::new(&format!("/proc/{pid}")).exists() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

fn serve(listener: UnixListener, tx: Sender<String>, wake: impl Fn()) {
    for stream in listener.incoming().flatten() {
        let mut writer = stream.try_clone().ok();
        for line in BufReader::new(stream).lines().map_while(|l| l.ok()) {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            if tx.send(line).is_err() {
                return;
            }
            wake();
            if let Some(w) = writer.as_mut() {
                let _ = w.write_all(b"ok\n");
            }
        }
    }
}

/// Client side of `folio --remote`.
pub fn send(commands: &[String]) -> Result<()> {
    let path = std::env::var_os("FOLIO_SOCKET").map(PathBuf::from).unwrap_or_else(default_socket);
    let mut stream = UnixStream::connect(&path).with_context(|| format!("no running folio at {}", path.display()))?;
    for c in commands {
        stream.write_all(c.as_bytes())?;
        stream.write_all(b"\n")?;
    }
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut acks = String::new();
    BufReader::new(stream).read_line(&mut acks)?;
    Ok(())
}
