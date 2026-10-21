//! The `folio` binary's command line. Never opens a window: every test uses a flag that exits.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A temp directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("folio-cli-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An isolated home for one folio invocation: config and data go to temp dirs, never the real ones.
struct Env {
    config: TempDir,
    data: TempDir,
}

impl Env {
    fn new() -> Self {
        Self { config: TempDir::new(), data: TempDir::new() }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_folio"));
        c.args(args).env("XDG_CONFIG_HOME", &self.config.0).env("XDG_DATA_HOME", &self.data.0).env_remove("FOLIO_SOCKET");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().expect("run folio")
    }
}

fn folio(args: &[&str]) -> Output {
    Env::new().run(args)
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Run and require success; returns stdout.
fn ok(args: &[&str]) -> String {
    let o = folio(args);
    assert!(o.status.success(), "folio {args:?} failed: {}", stderr(&o));
    stdout(&o)
}

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name).to_string_lossy().into_owned()
}

#[test]
fn version_prints_the_crate_version() {
    assert_eq!(ok(&["--version"]), format!("folio {}\n", env!("CARGO_PKG_VERSION")));
    assert_eq!(ok(&["-V"]), ok(&["--version"]));
}

#[test]
fn help_prints_usage() {
    let out = ok(&["--help"]);
    assert!(out.contains("usage:") && out.contains("--remote"), "{out}");
    assert_eq!(ok(&["-h"]), out);
}

#[test]
fn unknown_flag_fails_with_a_message() {
    let o = folio(&["--bogus"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("unknown option --bogus"), "{}", stderr(&o));
}

#[test]
fn list_commands_matches_docs_commands_md() {
    let docs = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/COMMANDS.md")).unwrap();
    let table: Vec<&str> = docs.lines().filter(|l| l.starts_with('|')).collect();
    let out = ok(&["--list-commands"]);
    let generated: Vec<&str> = out.lines().collect();
    assert_eq!(generated, table, "docs/COMMANDS.md is stale: regenerate its table with `folio --list-commands`");
}

#[test]
fn print_config_is_valid_toml() {
    let out = ok(&["--print-config"]);
    let value: toml::Table = toml::from_str(&out).expect("--print-config prints valid TOML");
    assert!(value.contains_key("general") && value.contains_key("keys"));
}

#[test]
fn print_init_prints_lua() {
    let out = ok(&["--print-init"]);
    assert!(out.contains("folio.command("), "{out}");
}

#[test]
fn init_config_writes_both_files_once() {
    let env = Env::new();
    let o = env.run(&["--init-config"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let dir = env.config.0.join("folio");
    let config = std::fs::read_to_string(dir.join("config.toml")).expect("config.toml written");
    let init = std::fs::read_to_string(dir.join("init.lua")).expect("init.lua written");
    assert_eq!(config, ok(&["--print-config"]));
    assert_eq!(init, ok(&["--print-init"]));

    std::fs::write(dir.join("config.toml"), "# mine\n").unwrap();
    let o = env.run(&["--init-config"]);
    assert!(o.status.success());
    assert_eq!(stdout(&o).matches("exists, skipped").count(), 2, "{}", stdout(&o));
    assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), "# mine\n", "an existing file must not be overwritten");
}

#[test]
fn remote_without_a_running_instance_fails_clearly() {
    let env = Env::new();
    let socket = env.data.0.join("nobody.sock");
    let o = env.command(&["--remote", "goto_page 2"]).env("FOLIO_SOCKET", &socket).output().unwrap();
    assert!(!o.status.success());
    let err = stderr(&o);
    assert!(err.contains("no running folio") && err.contains("nobody.sock"), "{err}");
}

#[test]
fn debug_requires_a_command_and_file() {
    let o = folio(&["--debug", "outline"]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("usage: folio --debug"), "{}", stderr(&o));
}

#[test]
fn debug_unknown_command_fails() {
    assert!(!folio(&["--debug", "nope", &fixture("refs.pdf")]).status.success());
}

#[test]
fn debug_on_a_missing_file_fails() {
    let o = folio(&["--debug", "outline", &fixture("missing.pdf")]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("missing.pdf"), "{}", stderr(&o));
}

#[test]
fn debug_outline() {
    let out = ok(&["--debug", "outline", &fixture("refs.pdf")]);
    let lines: Vec<&str> = out.lines().map(|l| l.split(" -> ").next().unwrap()).collect();
    assert_eq!(lines, ["Introduction", "  Motivation", "Results", "Proofs"]);
    assert!(out.contains("Introduction -> Some(DocPos { page: 3,"), "{out}");
}

#[test]
fn debug_links() {
    let out = ok(&["--debug", "links", &fixture("refs.pdf"), "3"]);
    assert!(out.contains("-> Uri(\"https://example.org/folio\")"), "{out}");
    assert!(out.contains("-> Other(\"file:notes.txt\")"), "{out}");
    assert_eq!(out.matches("-> Page(").count(), 10, "{out}");
}

#[test]
fn debug_text() {
    let out = ok(&["--debug", "text", &fixture("refs.pdf"), "3"]);
    let mut lines = out.lines();
    let counts = lines.next().unwrap();
    let (chars, rects) = counts.split_once(" chars, ").expect("a count line");
    assert_eq!(format!("{chars} rects"), rects, "char and rect counts match");
    assert!(lines.next().unwrap().ends_with(" 1 Introduction"), "{out}");
    assert!(out.contains(" renormaliza\n") || out.contains(" renormalization"), "{out}");
}

#[test]
fn debug_refs() {
    let out = ok(&["--debug", "refs", &fixture("refs.pdf"), "3"]);
    for line in ["Citation \"1\" '[1]'", "Citation \"2\" '[2, 3]'", "Citation \"4\" '[4–6]'", "Table \"2\" 'Table 2'"] {
        assert!(out.lines().any(|l| l == line), "missing {line:?} in\n{out}");
    }
}

#[test]
fn debug_findref() {
    let out = ok(&["--debug", "findref", &fixture("refs.pdf"), "3"]);
    for want in [
        "-> p7 «[1] A. Alpha, A survey of the field, J. Tests 1 (2020).»",
        "-> p5 «Table 2: Measured data.»",
        "-> p4 «Figure 1: The experimental setup.»",
        "-> p6 «Lemma 3.1. The key estimate is sharp.»",
    ] {
        assert!(out.contains(want), "missing {want:?} in\n{out}");
    }
}

#[test]
fn debug_search() {
    let out = ok(&["--debug", "search", &fixture("refs.pdf"), "renormalization"]);
    assert!(out.starts_with("p4 Some(Rect"), "{out}");
    assert!(out.ends_with("1 hits\n"), "{out}");
    assert!(ok(&["--debug", "search", &fixture("refs.pdf"), "Theorem"]).ends_with("4 hits\n"));
}

#[test]
fn debug_render_writes_a_ppm() {
    let dir = TempDir::new();
    let out = dir.0.join("page.ppm");
    let o = folio(&["--debug", "render", &fixture("twocol.pdf"), "0", out.to_str().unwrap()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let bytes = std::fs::read(&out).unwrap();
    let header = String::from_utf8_lossy(&bytes[..20.min(bytes.len())]).into_owned();
    assert!(header.starts_with("P6\n1224 1584\n255\n"), "expected a 2× render of a letter page, got {header:?}");
}
