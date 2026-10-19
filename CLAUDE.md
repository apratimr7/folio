# folio — context for Claude

folio is a keyboard-first PDF reader for technical documents (a sioyek-like tool), written in Rust. The owner is a
physics researcher (AdS/CFT, black holes) who reads papers and books and writes LaTeX/Typst. Goals: a fully
keyboard-centric, clean modern UI that's customizable, extensible by scripting (Lua), and designed for research
reading. **AI features come next** (see `ROADMAP.md` § 0.3).

Version 0.1.0 was built and tested on Linux (Arch, Hyprland/Wayland). **Work continues on macOS.**

## Start here on the Mac

1. Build it: `./scripts/build-macos.sh` (needs Xcode CLT, Homebrew, rustup; installs `poppler` and `pkg-config` via
   brew). Dev loop: `cargo build && ./target/debug/folio ~/some-paper.pdf`. If pkg-config can't find poppler:
   `export PKG_CONFIG_PATH="$(brew --prefix)/lib/pkgconfig:$PKG_CONFIG_PATH"`.
2. The macOS build has **never been run on real hardware**. It type-checks for `aarch64-apple-darwin` and that's all.
   Verify first, and fix what breaks:
   - Retina sharpness: tiles are rendered at `zoom * pixels_per_point`, and the page origin is snapped to physical pixels.
   - Cmd as Ctrl: `ui/frame.rs::chords()` maps `modifiers.command` to `ctrl`. Check `Cmd-d`, `Cmd-q`, and that
     plain `Ctrl` still works.
   - Font discovery (`ui/style.rs::find_font`): there's no fontconfig on macOS, so it falls back to `/System/Library/Fonts/SFNS.ttf`.
   - Smooth trackpad scrolling and pinch zoom (`canvas` in `ui/frame.rs`).
   - The IPC socket is in `$TMPDIR` (no `XDG_RUNTIME_DIR` on macOS). Stale-socket cleanup is Linux-only by design.
3. Known gap: opening a PDF from Finder doesn't load it (Apple Event `odoc` isn't handled). See `TODO.md`.
4. CI (`.github/workflows/build.yml`) has never run. Check the Actions tab after a push. The Ubuntu job may need a
   newer poppler-glib.

## Commands

```sh
cargo build                      # dev profile is opt-level 1 (deps at 3), fast enough to use
cargo test                       # keymap + view unit tests
cargo clippy && cargo fmt        # rustfmt.toml: max_width 130
cargo build --release
./target/debug/folio --debug outline|text|links|refs|findref|search FILE [PAGE|QUERY]   # headless backend checks (PAGE is 0-based)
./target/debug/folio --list-commands > /tmp/c.md   # regenerate docs/COMMANDS.md body after changing commands
```

### Seeing and driving the running app

The UI is egui, so you can't inspect it directly. Use remote control plus the built-in screenshot:

```sh
folio paper.pdf &                                   # isolate state with XDG_CONFIG_HOME=/tmp/x XDG_DATA_HOME=/tmp/y
folio --remote 'goto_page 28' 'smart_jump'          # any command line; ';' also separates commands
folio --remote 'screenshot /tmp/shot.png'           # PNG of the window's own framebuffer, then Read it
folio --remote "lua io.stderr:write(folio.state().page .. '\n')"   # probe state (stderr of the app)
```

Keys can't be injected through `--remote`. Test key paths with unit tests in `keymap.rs`, or call the command the
key maps to. Don't take whole-screen screenshots of the user's desktop, and don't launch windows that steal focus
while the user is typing. Use the in-app `screenshot` command.

## Architecture (details in `docs/ARCHITECTURE.md`)

- **Everything is a named command** (`commands.rs` table + `App::run_builtin` in `app.rs`). Keys, the palette, Lua,
  `--remote` and future AI tools all call commands. A new feature means adding a command, then binding it.
- `doc/`: `Backend` trait plus the poppler implementation. Coordinates are PDF points with a top-left origin.
  The safe `poppler-rs` covers rendering; text layout, links and outline use raw FFI (`poppler::ffi`).
  `insert_gap_spaces` repairs missing spaces and newlines in poppler's text. Backends are **not `Send`**, so every
  thread opens its own.
- `render.rs`: worker pool. The UI publishes the wanted `TileKey` list every frame and workers take the first one not
  in flight. 1024² tiles, generations for reloads, `recolor` for dark/tinted modes.
- `text.rs`: background search (streams hits) and smart-reference resolution. `refs.rs`: detection regexes and
  definition heuristics.
- `view.rs`: layout and camera, pure. `keymap.rs`: modal key engine, pure. Both are unit-tested.
- `session.rs`: per-document state. `app.rs`: commands, pickers, prompts, hints, peek. `ui/frame.rs`: the egui frame loop.
- `script.rs`: Lua (mlua, vendored 5.4). Scripts read a `Snapshot` and queue `Effect`s that `App` applies afterwards.
  Never let Lua borrow app state directly.
- `store.rs`: JSON state in `~/.local/share/folio/docs/<fnv1a(path)>.json`. Config is in `~/.config/folio/`
  (XDG-style on macOS too).

## Gotchas

- **egui/eframe 0.37** (new API). `App::ui(&mut self, ui, frame)`, not `update`. Panels are `egui::Panel::bottom/left/right(..).show(ui, ..)`
  and `CentralPanel::no_frame()`. `Id::new` is deprecated, so use `Id::unique`. `TextEdit::frame` takes a `Frame`.
  Read the crate source under `~/.cargo/registry/src` when unsure; don't guess from older egui docs.
- Edition 2024: `gen` is a reserved word. Let-chains are used (Rust ≥ 1.88).
- `eframe` uses the `glow` backend (default features off) for faster builds.
- Keys that open a text field in the same frame would also type into it. `handle_keys` strips key events after
  dispatch to prevent that. Keep it that way.
- Poppler link areas use a bottom-left origin and get flipped in `links()`. Text layout is already top-left.

## Conventions

- Match the surrounding style: terse doc comments on modules and non-obvious functions, no comment noise.
- When behavior changes, update `CHANGELOG.md` (under `[Unreleased]`), `TODO.md`, and the relevant `docs/*.md`.
  Regenerate `docs/COMMANDS.md` with `--list-commands` (keep its header paragraph).
- Commits end with a `Co-Authored-By: Claude …` line. Ask before pushing.

## Likely next work

1. Get the Mac build running and fix platform issues (above).
2. AI features (ROADMAP 0.3): assistant panel, explain selection or equation with context resolved through smart
   references, citation lookup (arXiv/Semantic Scholar), summaries, semantic search. Build them as commands and
   expose them in Lua. Ask the user which provider and model to use, and keep anything from leaving the machine
   unless they explicitly run a command that sends it.
3. Items in `TODO.md`: SyncTeX, two-page mode, cross-page selection, highlight re-anchoring after reflow.
