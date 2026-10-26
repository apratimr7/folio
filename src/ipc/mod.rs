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

/// One command line from a client, and where to send its outcome.
pub struct Request {
    pub line: String,
    pub reply: crossbeam_channel::Sender<Reply>,
}

/// The outcome of a remote command line: output to print (may be empty), or the warnings and errors it raised.
pub enum Reply {
    Ok(String),
    Err(String),
}

impl Reply {
    /// One protocol line: `ok`, `ok <output>` or `err <message>`, with newlines escaped.
    pub fn encode(&self) -> String {
        let (tag, text) = match self {
            Reply::Ok(t) => ("ok", t),
            Reply::Err(t) => ("err", t),
        };
        let text = text.replace('\\', "\\\\").replace('\n', "\\n");
        if text.is_empty() { format!("{tag}\n") } else { format!("{tag} {text}\n") }
    }

    pub fn decode(line: &str) -> Reply {
        let line = line.trim_end_matches(['\r', '\n']);
        let (tag, text) = line.split_once(' ').unwrap_or((line, ""));
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
            } else if let Some(e) = chars.next() {
                out.push(if e == 'n' { '\n' } else { e });
            }
        }
        if tag == "err" { Reply::Err(out) } else { Reply::Ok(out) }
    }
}

#[cfg(test)]
mod tests {
    use super::Reply;

    #[test]
    fn replies_round_trip() {
        for (reply, line) in [
            (Reply::Ok(String::new()), "ok\n"),
            (Reply::Ok("12".into()), "ok 12\n"),
            (Reply::Err("unknown command 'x'\nsecond \\ line".into()), "err unknown command 'x'\\nsecond \\\\ line\n"),
        ] {
            assert_eq!(reply.encode(), line);
            match (Reply::decode(line), reply) {
                (Reply::Ok(a), Reply::Ok(b)) | (Reply::Err(a), Reply::Err(b)) => assert_eq!(a, b),
                _ => panic!("kind changed for {line:?}"),
            }
        }
    }
}
