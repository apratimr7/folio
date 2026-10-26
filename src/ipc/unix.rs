//! Remote control over a Unix socket (Linux, macOS).

use std::io::ErrorKind;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use super::{Reply, Request};
use crate::config::APP_NAME;

pub fn socket_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

/// The "current instance" socket, a symlink to the per-process one.
pub fn default_socket() -> PathBuf {
    socket_dir().join(format!("{APP_NAME}.sock"))
}

pub struct Server {
    pub rx: Receiver<Request>,
    path: PathBuf,
}

impl Server {
    pub fn start(wake: impl Fn() + Send + 'static) -> Result<Self> {
        cleanup_stale();
        let path = socket_dir().join(format!("{APP_NAME}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).with_context(|| format!("bind {}", path.display()))?;
        // Anyone who can connect can run commands (and Lua), so keep it private.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
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

/// Per-process sockets (`folio-<pid>.sock`) in the socket directory.
fn instance_sockets() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(socket_dir()) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_prefix(&format!("{APP_NAME}-"))
                .and_then(|n| n.strip_suffix(".sock"))
                .is_some_and(|pid| pid.parse::<u32>().is_ok())
        })
        .map(|e| e.path())
        .collect()
}

/// Remove sockets left behind by instances that were killed: nobody listens on them any more.
fn cleanup_stale() {
    for path in instance_sockets() {
        if UnixStream::connect(&path).is_err_and(|e| e.kind() == ErrorKind::ConnectionRefused) {
            let _ = std::fs::remove_file(&path);
        }
    }
    let link = default_socket();
    if std::fs::symlink_metadata(&link).is_ok() && std::fs::metadata(&link).is_err() {
        let _ = std::fs::remove_file(link);
    }
}

/// The most recently started instance that still accepts connections.
fn newest_live_socket() -> Option<UnixStream> {
    let mut socks: Vec<(SystemTime, PathBuf)> = instance_sockets()
        .into_iter()
        .filter_map(|p| Some((std::fs::metadata(&p).and_then(|m| m.modified()).ok()?, p)))
        .collect();
    socks.sort_by_key(|s| std::cmp::Reverse(s.0));
    socks.into_iter().find_map(|(_, p)| UnixStream::connect(p).ok())
}

fn serve(listener: UnixListener, tx: Sender<Request>, wake: impl Fn()) {
    for stream in listener.incoming().flatten() {
        let mut writer = stream.try_clone().ok();
        for line in BufReader::new(stream).lines().map_while(|l| l.ok()) {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            let (reply_tx, reply_rx) = crossbeam_channel::bounded(1);
            if tx.send(Request { line, reply: reply_tx }).is_err() {
                return;
            }
            wake();
            // The UI thread runs the command on its next frame; a modal dialog can hold it up.
            let reply = reply_rx.recv_timeout(Duration::from_secs(60)).unwrap_or(Reply::Err("no reply (folio is busy)".into()));
            if let Some(w) = writer.as_mut() {
                let _ = w.write_all(reply.encode().as_bytes());
            }
        }
    }
}

/// Client side of `folio --remote`: print output, report errors, and exit with status 1 if a command failed.
pub fn send(commands: &[String]) -> Result<()> {
    let mut failed = 0;
    for reply in deliver(commands)? {
        match reply {
            Reply::Ok(out) if !out.is_empty() => println!("{out}"),
            Reply::Ok(_) => {}
            Reply::Err(e) => {
                eprintln!("{APP_NAME}: {e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Send command lines to the running instance and collect one reply per line. Fails if none is running.
pub fn deliver(commands: &[String]) -> Result<Vec<Reply>> {
    let explicit = std::env::var_os(socket_env()).map(PathBuf::from);
    let path = explicit.clone().unwrap_or_else(default_socket);
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        // `folio.sock` can point at an instance that was killed; try the others.
        Err(e) => match explicit.is_none().then(newest_live_socket).flatten() {
            Some(s) => s,
            None => return Err(e).with_context(|| format!("no running folio at {}", path.display())),
        },
    };
    for c in commands {
        stream.write_all(c.as_bytes())?;
        stream.write_all(b"\n")?;
    }
    stream.shutdown(std::net::Shutdown::Write)?;
    BufReader::new(stream).lines().map(|l| Ok(Reply::decode(&l?))).collect()
}

/// `FOLIO_SOCKET` (upper-case program name): targets a specific instance.
fn socket_env() -> String {
    format!("{}_SOCKET", APP_NAME.to_uppercase())
}
