# folio — context for Claude

folio is a keyboard-first PDF reader for technical documents (a sioyek-like tool), written in Rust. The owner is a
physics researcher (AdS/CFT, black holes) who reads papers and books and writes LaTeX/Typst. Goals: a fully
keyboard-centric, clean modern UI that's customizable, extensible by scripting (Lua), and designed for research
reading. **AI features come next** (see `ROADMAP.md` § 0.3).

Version 0.1.0 (plus the fixes under `[Unreleased]` in `CHANGELOG.md`) was built and tested on Linux (Arch,
Hyprland/Wayland). **Work continues on macOS.**

## Status as of 2026-10-20

- The user tested all basic commands by hand. They work, except two gaps logged in `TODO.md` under
  "From user testing": marks can't be deleted, and visual mode can't start where you want.
- A full Lua API / automation / customization test pass found 7 bugs. **All are fixed and re-verified**
  (`docs/testing/2026-10-20-lua-api-report.md`, `CHANGELOG.md` [Unreleased]). Still open from it: debouncing the
  `selection` hook, and a manual `folio.clipboard` check.
- CI passes on Linux, macOS arm64 (`macos-15`) and macOS x86_64 (`macos-15-intel`). The zipped `.app` is an
  artifact of each run (`gh run download <id> -n folio-macos-macos-15`).
- The user's own priorities, in their words so far: first get it running on the Mac, then decide between AI
  features and SyncTeX (they write LaTeX/Typst).

## Start here on the Mac

1. Build it: `./scripts/build-macos.sh`. Or skip building and use the CI artifact:
   `gh run download -n folio-macos-macos-15` (latest run), unzip, then right-click → Open (not notarized).
   To build yourself, run `./scripts/build-macos.sh` (needs Xcode CLT and rustup with Rust ≥ 1.95; MuPDF is built from
   source and linked statically). Dev loop: `cargo build && ./target/debug/folio ~/some-paper.pdf`.
2. The macOS build **compiles and passes tests in CI but has never been run on a real Mac**. Verify first, and fix
   what breaks:
   - Retina sharpness: tiles are rendered at `zoom * pixels_per_point`, and the page origin is snapped to physical pixels.
   - Cmd as Ctrl: `ui/frame.rs::chords()` maps `modifiers.command` to `ctrl`. Check `Cmd-d`, `Cmd-q`, and that
     plain `Ctrl` still works.
   - Font discovery (`ui/style.rs::find_font`): there's no fontconfig on macOS, so it falls back to `/System/Library/Fonts/SFNS.ttf`.
   - Smooth trackpad scrolling and pinch zoom (`canvas` in `ui/frame.rs`).
   - The IPC socket is in `$TMPDIR` (no `XDG_RUNTIME_DIR` on macOS). Stale sockets are found by connecting to them.
3. Known gap: opening a PDF from Finder doesn't load it (Apple Event `odoc` isn't handled). See `TODO.md`.
4. CI (`.github/workflows/build.yml`) is green. Its actions still use Node 20 (deprecation warning); bump
   `actions/checkout` / `actions/upload-artifact` when convenient.

## Commands

```sh
cargo build                      # dev profile is opt-level 1 (deps at 3), fast enough to use
cargo test                       # keymap + view unit tests
cargo clippy && cargo fmt        # rustfmt.toml: max_width 130
cargo build --release
./target/debug/folio --debug outline|text|links|refs|findref|search FILE [PAGE|QUERY]   # headless backend checks (PAGE is 0-based)
./target/debug/folio --list-commands > /tmp/c.md   # regenerate docs/COMMANDS.md body after changing commands
```

### Testing without the user

The Lua/automation pass used this approach, and it's the template for regression tests. Put a test `init.lua` (and
`config.toml`) in throwaway dirs (`XDG_CONFIG_HOME`/`XDG_DATA_HOME`). Have it log `PASS`/`FAIL` lines to a file,
expose test commands (`folio.command("t_read", …)`), trigger them with `folio --remote`, and check UI results with
the in-app `screenshot`. Never write to the user's real `~/.config/folio` or `~/.local/share/folio` in tests.
On Linux, real key events were sent with Hyprland's `send_shortcut` to a window on a temporary headless output.
On macOS, test through `--remote` and unit tests, and ask the user before driving their screen.
A built-in `folio --test-script` mode that runs such a suite headlessly would be a good addition for CI.

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
- `doc/`: `Backend` trait plus the MuPDF implementation (`doc/mupdf.rs`, the `mupdf` crate; a patched copy lives in
  `vendor/mupdf`, see `FOLIO-PATCHES.md` there). Coordinates are points with a top-left origin.
  `join_visual_lines` merges MuPDF's line pieces that share a baseline. Backends are **not `Send`**, so every
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
- Edition 2024: `gen` is a reserved word. Let-chains are used. Minimum Rust is 1.95 (egui 0.37).
- `eframe` uses the `glow` backend (default features off) for faster builds.
- Keys that open a text field in the same frame would also type into it. `handle_keys` strips key events after
  dispatch to prevent that. Keep it that way.
- MuPDF page space is already top-left; `MupdfDoc` subtracts each page's bounds origin (crop boxes) so pages
  start at (0, 0). MuPDF splits one visual line at wide gaps; `join_visual_lines` puts it back together.
- The `mupdf` crate is AGPL and so is folio. Fixes to the crate go into `vendor/mupdf` and `FOLIO-PATCHES.md`.

## Conventions

- Match the surrounding style: terse doc comments on modules and non-obvious functions, no comment noise.
- When behavior changes, update `CHANGELOG.md` (under `[Unreleased]`), `TODO.md`, and the relevant `docs/*.md`.
  Regenerate `docs/COMMANDS.md` with `--list-commands` (keep its header paragraph).
- Commits end with a `Co-Authored-By: Claude …` line. Ask before pushing.

## Likely next work

1. Get the Mac build running and fix platform issues (above). Then `TODO.md` "From user testing": mark deletion,
   hint-based visual-mode start, and the Lua API gaps.
2. AI features (ROADMAP 0.3): assistant panel, explain selection or equation with context resolved through smart
   references, citation lookup (arXiv/Semantic Scholar), summaries, semantic search. Build them as commands and
   expose them in Lua. Ask the user which provider and model to use, and keep anything from leaving the machine
   unless they explicitly run a command that sends it.
3. Items in `TODO.md`: SyncTeX, two-page mode, cross-page selection, highlight re-anchoring after reflow.
