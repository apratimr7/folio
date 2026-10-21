//! Remote control.
//!
//! `folio --remote "goto_page 12"` sends command lines to the most recently
//! started instance. Editors can use this for forward search, and external
//! tools (or AI agents) can drive the reader the same way keys do.

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;
