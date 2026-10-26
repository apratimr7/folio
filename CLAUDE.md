# folio — context for Claude

folio is a keyboard-first PDF reader for technical documents (a sioyek-like tool), written in Rust, for people who
read papers and books and write LaTeX/Typst. Goals: a fully keyboard-centric, clean modern UI that's customizable,
extensible by scripting (Lua), and designed for research reading. **AI features come next** (see `ROADMAP.md` § 0.3).

Version 0.2.1 (see `CHANGELOG.md`; work since goes under `[Unreleased]`). Runs on Linux, macOS and Windows.
Branches: `main` is the latest state everyone should use (CI green on all platforms); `dev` is where work happens
and is merged into `main` once CI passes. The repository is public on GitHub (`apratimr7/folio`) since v0.2.0.

## Status as of 2026-10-26

- **0.2.1** adds tabs (`tabs.rs`, restored on start, files handed to the running window), background opening
  (`loader.rs`), the system file dialogs (`dialogs.rs`, rfd), Finder opens on macOS (`open_events.rs`), `o` as a key
  prefix with a key menu, editable highlights (`highlights.rs`), selections across pages, rotation, password
  prompts, an app icon (`packaging/icons/`), and an mdBook site whose reference tables are generated
  (`reference.rs`, `folio --update-docs`). `feed_keys` lets `--remote` send keys, so key paths can be tested.
- Test launches of the GUI must pass `--new`, or `folio FILE` hands the file to whatever folio is running (the
  user's own instance included).

## Status as of 2026-10-21

- **PDF engine is MuPDF** (was poppler), statically linked; folio has no runtime dependencies. License is
  **AGPL-3.0-or-later** because of MuPDF. `vendor/mupdf` is a patched mupdf-rs (see `FOLIO-PATCHES.md`).
- **Platforms:** Linux (the original; hand-tested before the MuPDF switch, CI since), macOS Apple Silicon (full
  hand test pass + CI), macOS Intel (CI), **Windows** (new: CI builds, tests and zips `folio.exe`; first testers
  are trying it; remote control and signal handling are Unix-only so far).
- **Tests:** 140 unit + 18 CLI tests on committed fixture PDFs (`docs/TESTING.md`), run in CI on all platforms.
- Test passes so far: Lua/automation on Linux and a full macOS pass (`docs/testing/`). Their bugs are fixed except
  what's still open in `TODO.md`.
- **Recently added:** visual mode starts with row → word hints (`v`; `v v` = old start); this works well, so caret
  mode is only an optional idea. Dark-mode link contrast, window-geometry memory, drag and drop, clean
  quit on Cmd-Q and signals, config warnings, link-scheme safety, marks deletion.
- `TODO.md` is the single list of open work. Notable sections: "Configuration space (to build in one go)" lists
  every hard-coded tunable (dimming amounts first) to be built in one pass later; "Later →
  Polishing" holds a sensible default config and UI enhancements. `ROADMAP.md` is the long term.
- Unchecked on real hardware: Retina sharpness, the Intel Mac build, Windows (with first testers), a real Finder
  drag, and window-geometry restore (not observable under a tiling window manager).
- **Priorities:** get it solid on each platform; then decide between AI features and SyncTeX; build the
  configuration space in one go; then new window/tab, polishing (default config, UI enhancements, blur), and a
  website last. Ask before starting a big item.

## Picking it up on another machine

```sh
git clone https://github.com/apratimr7/folio && cd folio && git switch dev   # or stay on main to just use it
rustup toolchain install 1.95                                                 # minimum Rust (egui 0.37)
cargo +1.95 test                                                              # first build compiles MuPDF (minutes)
```

Build prerequisites per platform are in `docs/BUILDING.md`: Linux needs a C compiler, `make`, `clang`/`libclang` and
the xkbcommon/GL headers; macOS needs the Xcode command-line tools; Windows needs the VS 2022 Build Tools (C++) and
LLVM. Ready-made binaries for all platforms are the artifacts of the latest green CI run
(`gh run download -n folio-<platform>…`, see `docs/BUILDING.md`).

## Releasing

Version tags publish releases: see `docs/BUILDING.md` § Releasing (bump `Cargo.toml`, cut the CHANGELOG section,
`git tag vx.y.z && git push origin main vx.y.z`; `.github/workflows/release.yml` does the rest). v0.2.0 (2026-10-21)
is the first public release; the repository is public from then on, so never commit personal paths, documents or
anything private (tests use `tests/fixtures/` only).

## Renaming

The name may change. What follows the package name in `Cargo.toml` automatically: the binary, `config::APP_NAME`
(config and data folders, socket names, the `<NAME>_SOCKET` variable, window title and app ID, the home screen),
the macOS bundle and its Info.plist (`@NAME@`), the Linux `.desktop` file (`packaging/linux/app.desktop.in`) and the
release archive names. A rename still needs, by hand:

1. `name` (and `repository`) in `Cargo.toml`; rename the GitHub repository (GitHub redirects the old URL).
2. Text: `grep -rn -i folio` over `*.md`, `src/main.rs` (`USAGE`), log prefixes (`eprintln!("folio: …")`), the
   workflow artifact names and release title, and the badges/links in `README.md`.
3. The Lua global `folio` is API: keep `folio` as an alias of the new name so existing `init.lua` files work.
4. Existing users' folders move with the name (`~/.config/<name>`, `~/.local/share/<name>`): on first start,
   move or read the old `folio` folders if the new ones don't exist.
5. The macOS bundle ID (`io.github.apratimr7.<name>`) changes too, so macOS treats it as a new app.

## Working on the Mac

- Build: `./scripts/build-macos.sh` (Xcode CLT and rustup with Rust ≥ 1.95; the first build compiles MuPDF from C
  and takes a few minutes). Dev loop: `cargo build && ./target/debug/folio ~/some-paper.pdf`. If the default
  toolchain is older, use `cargo +1.95 …`.
- CI artifact instead of building: `gh run download -n folio-macos-macos-15`, unzip, right-click → Open.
- Cmd works as Ctrl (`ui/frame.rs::chords()`); Cmd-Q goes through the app menu, so quitting is handled in
  `App::on_exit` too. The remote socket is in `$TMPDIR`; macOS limits socket paths to 104 bytes.
- Finder opens (double-click, Open With, `open -a folio f.pdf`) arrive as `application:openURLs:`, added to
  `NSObject` in `open_events.rs`; they open as tabs.

## Working on Windows

- Build: `./scripts/build-windows.ps1` (PowerShell). Config `%APPDATA%\folio`, data `%LOCALAPPDATA%\folio`.
- `ipc/windows.rs` is a stub (no `--remote`); `close_on_signals` is a no-op. Unix-only tests are `#[cfg(unix)]`.
- The binary is a GUI-subsystem program that reattaches to the parent console for CLI output (`main.rs`).

## Commands

```sh
cargo build                      # dev profile is opt-level 1 (deps at 3), fast enough to use
cargo test                       # unit + CLI tests on tests/fixtures (docs/TESTING.md); ~1 s after the build
cargo clippy && cargo fmt        # rustfmt.toml: max_width 130
cargo build --release
./target/debug/folio --debug outline|text|links|refs|findref|search FILE [PAGE|QUERY]   # headless backend checks (PAGE is 0-based)
./target/debug/folio --update-docs                  # refresh the generated tables in docs/ (commands, settings, Lua API)
```

### Testing without the user

Prefer automated tests: `docs/TESTING.md` covers the suite, the fixture PDFs, and how to add a test for a bug.
What needs the running app (the GUI, real input, `App` commands) is tested as follows.

The Lua/automation pass used this approach, and it's the template for regression tests. Put a test `init.lua` (and
`config.toml`) in throwaway dirs (`XDG_CONFIG_HOME`/`XDG_DATA_HOME`). Have it log `PASS`/`FAIL` lines to a file,
expose test commands (`folio.command("t_read", …)`), trigger them with `folio --remote`, and check UI results with
the in-app `screenshot`. Never write to the user's real `~/.config/folio` or `~/.local/share/folio` in tests.
On Linux, real key events were sent with Hyprland's `send_shortcut` to a window on a temporary headless output.
On macOS, start isolated instances in the background with `open -g -n -a <app> --env XDG_CONFIG_HOME=… --env
XDG_DATA_HOME=… --args file.pdf` (a plain background process steals focus), target them with
`FOLIO_SOCKET=$TMPDIR/folio-<pid>.sock`, and ask the user before sending real keys (System Events) or mouse events
(CGEvents). Under a tiling window manager, window geometry can't be set or observed.
A built-in `folio --test-script` mode that runs such a suite headlessly would be a good addition for CI.

### Seeing and driving the running app

The UI is egui, so you can't inspect it directly. Use remote control plus the built-in screenshot:

```sh
folio paper.pdf &                                   # isolate state with XDG_CONFIG_HOME=/tmp/x XDG_DATA_HOME=/tmp/y
folio --remote 'goto_page 28' 'smart_jump'          # any command line; ';' also separates commands
folio --remote 'screenshot /tmp/shot.png'           # PNG of the window's own framebuffer, then Read it
folio --remote "lua io.stderr:write(folio.state().page .. '\n')"   # probe state (stderr of the app)
```

Keys can be injected with `folio --remote 'feed_keys gg'` (vim notation); unit tests in `keymap.rs` cover the
dispatcher. Don't take whole-screen screenshots of the user's desktop, and don't launch windows that steal focus
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
  Run `folio --update-docs` after changing commands, settings (`config::GENERAL_DOCS`) or the Lua API
  (`script::LUA_API`, `HOOKS`); `tests/docs.rs` fails until the docs match. The site is `mdbook build` (`book.toml`).
- Commits end with a `Co-Authored-By: Claude …` line. Ask before pushing.

## Likely next work

1. Get the Mac build running and fix platform issues (above). Then `TODO.md` "From user testing": mark deletion,
   hint-based visual-mode start, and the Lua API gaps.
2. AI features (ROADMAP 0.3): assistant panel, explain selection or equation with context resolved through smart
   references, citation lookup (arXiv/Semantic Scholar), summaries, semantic search. Build them as commands and
   expose them in Lua. Ask the user which provider and model to use, and keep anything from leaving the machine
   unless they explicitly run a command that sends it.
3. Items in `TODO.md`: SyncTeX, two-page mode, cross-page selection, highlight re-anchoring after reflow.
