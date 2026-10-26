//! Remote control isn't available on Windows yet: it needs a named-pipe transport (see `TODO.md`).
//! The reader works without it; `folio --remote` says so instead of failing obscurely.

use anyhow::{Result, bail};
use crossbeam_channel::Receiver;

#[allow(dead_code)] // never constructed until the named-pipe transport exists
pub struct Server {
    pub rx: Receiver<super::Request>,
}

impl Server {
    pub fn start(_wake: impl Fn() + Send + 'static) -> Result<Self> {
        bail!("not supported on Windows yet")
    }
}

pub fn send(_commands: &[String]) -> Result<()> {
    bail!("remote control (--remote) isn't supported on Windows yet")
}
