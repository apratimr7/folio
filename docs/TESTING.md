# Testing

folio has an automated suite of unit tests and CLI tests that runs headless on a small committed set of PDFs. The
GUI is tested by hand (see "Not covered" below).

## Running the tests

```sh
cargo test                                   # everything except ignored tests (seconds, once built)
cargo test --bin folio tests::refs           # one module of the unit tests
cargo test --bin folio number_first          # tests whose name contains a string
cargo test --test cli                        # only the CLI tests
cargo test -- --ignored                      # only the ignored tests (known bugs, see below)
cargo test -- --nocapture                    # show eprintln! output
```

Use a toolchain of at least Rust 1.95 (`cargo +1.95 test` if your default is older). The first build compiles
MuPDF from C and takes a few minutes. After that, the whole suite runs in about a second. CI runs `cargo test`
on Linux, both macOS architectures and Windows. A few tests are Unix-only (`#[cfg(unix)]`: the Lua `spawn` tests use
`sh`/`cat`, and remote control has no Windows transport yet). `.gitattributes` keeps LF line endings so the tests
that compare docs with command output also pass on Windows checkouts.

## Layout

| where | what |
|---|---|
| `src/tests/` | unit tests that use the fixtures or span several modules, one file per area (below) |
| `src/keymap.rs`, `src/view.rs` | inline `#[cfg(test)]` tests of the key engine and the layout/camera |
| `tests/cli.rs` | runs the built `folio` binary with flags that exit (`--debug`, `--list-commands`, …) |
| `tests/fixtures/` | the PDFs, their LaTeX sources, `build.sh`, and a README listing the facts tests rely on |

`src/tests/mod.rs` has the shared helpers: `fixture`/`open_fixture`, `line_strings`, `line_at` (the line under a
target rect), `synthetic_text` (a `PageText` from strings, for tests that don't need a PDF), `TempDir`, and
`IsolatedEnv`.

What each area covers:

- **`backend.rs`** (`MupdfDoc` through `doc::open`):
  - page counts and sizes, page labels (roman then arabic, `None` for an empty label)
  - outline titles, depths and targets
  - internal, external and `file:` links
  - visual lines (a heading number and its title on one line; an equation and its number on one line)
  - one rect per char, inside the page; two columns kept apart
  - metadata
  - rendering: a white background, dark text, and tiles that match one large render
  - encrypted PDFs give an error mentioning the password
  - truncated, zeroed and garbage files never panic or crash
- **`search.rs`**: smart case, ligatures both ways, words hyphenated across lines, typographic quotes and dashes,
  several hits with their rects, and the `normalize_page` char map.
- **`refs.rs`**: detection of each kind of reference on fixture pages. Resolution through `TextWorker`, as
  `--debug findref` does it: citations to bibliography entries, figure and table captions, equations (forwards
  and backwards), theorems and lemmas, and the section text fallback. Also `find_definitions` strong/weak rules.
- **`store.rs`**: `DocState` and `Global` JSON round trips. Missing, corrupt and minimal files load defaults.
  Recent-file ordering.
- **`config.rs`**:
  - the shipped `config/config.toml` equals the defaults; every theme preset loads
  - general and theme overrides apply; type errors are errors
  - `Config::load` from `XDG_CONFIG_HOME`; `Rgba::parse`
  - every default key binding parses and names a built-in command, with no duplicates
- **`commands.rs`**: every built-in command is in `docs/COMMANDS.md`, and every command listed there exists.
- **`script.rs`** (a `Script` with a test `Snapshot` and a real document):
  - `folio.state()`, `page_text`, `outline`, `highlights`
  - `command` with args and counts; `map` to commands and functions
  - every effect-queuing call, in order and deferred; `prompt`/`pick` callbacks
  - hooks (an error in one doesn't stop the next)
  - `spawn` (stdout, stderr, exit code, stdin, a missing program)
  - `require` next to `init.lua`
  - the shipped `config/init.lua`
- **`tests/cli.rs`**:
  - `--version`, `--help`, unknown flags
  - `--list-commands` equals the table in `docs/COMMANDS.md`
  - `--print-config` is valid TOML; `--print-init`
  - `--init-config` writes both files and doesn't overwrite
  - `--remote` with no instance fails with a clear message
  - every `--debug` subcommand on the fixtures

## Isolation rules

- Never touch the real `~/.config/folio` or `~/.local/share/folio`. Unit tests that read config or state hold an
  `IsolatedEnv`. It points `XDG_CONFIG_HOME` and `XDG_DATA_HOME` at fresh temp dirs and holds a lock, because the
  environment is process-wide. CLI tests set both variables on every child process.
- Never start the GUI. A CLI test must only use flags that exit. `folio FILE` opens a window.
- Damage tests run in-process on purpose: if MuPDF segfaults on a damaged file, the whole test binary fails, and
  that's the signal.
- Wait for background work with a channel and a timeout (see `spawn_and_wait`), not fixed sleeps.

## Fixtures

The PDFs in `tests/fixtures/` are committed, so neither CI nor contributors need LaTeX. `tests/fixtures/README.md`
lists each file's contents and the exact facts the tests assert, such as page numbers, labels and line texts.

- To change a fixture, edit its source in `tests/fixtures/src/` and run `tests/fixtures/build.sh` (needs
  `pdflatex`; the output is deterministic). Then update the README and any test whose facts changed.
- `encrypted.pdf` is made with MuPDF:
  `FOLIO_REGENERATE_FIXTURES=1 cargo test generate_encrypted_fixture -- --ignored`.
- Damaged files are generated in a temp dir at test time and never committed.
- Keep fixtures to a few pages. Real papers and books (as in the macOS test report) stay out of the repository.

## Known bugs as ignored tests

A bug that's reported but not fixed yet gets a test that asserts the **correct** behavior and is marked
`#[ignore = "pending: bug N in TODO.md"]`. `cargo test` stays green, and `cargo test -- --ignored` lists exactly
the open bugs. The fix then removes the `#[ignore]` line. Current ones:

| test | bug |
|---|---|
| `refs::number_first_theorem_resolves`, `refs::number_first_definition_resolves` | 3: `2.36 Theorem.` headers |
| `script::page_text_is_nil_for_negative_or_fractional_pages` | 11: `page_text(-1)` raises |
| `refs::captions_and_theorem_headers_are_not_references` | 12: a caption is hinted as a reference to itself |
| `refs::section_fallback_skips_table_of_contents` | 13: `Section 3.2` lands on the TOC line |

`backend::generate_encrypted_fixture` is also ignored. It's a generator, not a test, and does nothing without
`FOLIO_REGENERATE_FIXTURES=1`.

## Adding a test for a new bug

1. Reproduce it with the smallest input you can. Use `synthetic_text` for text logic, an existing fixture page,
   or a few lines added to a fixture source (rebuild it and update `tests/fixtures/README.md`).
2. Write one test for the one behavior, in the file for its area. Give it a name that states the correct behavior
   (`section_fallback_skips_table_of_contents`, not `bug13`), and an assertion message that shows what came out.
3. If the fix isn't part of the same change, add `#[ignore = "pending: bug N in TODO.md"]` and list it above.
4. Unit tests can reach any `pub` item through `crate::…`. If something is private, prefer testing it through a
   public caller. Otherwise make it `pub(crate)`, without changing behavior.

## Not covered

- **The GUI**: egui drawing, panels, pickers, prompts, hints, peek, toasts, smooth scrolling, Retina sharpness.
- **Real input**: keys, mouse, trackpad, pinch zoom, and Cmd on macOS. `keymap.rs` tests the key engine, but not
  the egui event path into it.
- **`App` commands** (`run_builtin` in `app.rs`) and remote control against a running instance. These need a
  window. The method used so far (isolated XDG dirs, `--remote`, a test `init.lua` logging PASS/FAIL lines,
  in-app screenshots, and on macOS System Events and CGEvent tools for real input) is described in
  `docs/testing/2026-10-20-macos-test-report.md` under "Test method notes". A headless `folio --test-script`
  mode would bring this into CI.
- Auto-reload timing, rendering workers, `render::recolor`, and the clipboard.
