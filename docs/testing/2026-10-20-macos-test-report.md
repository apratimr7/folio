# macOS full test report — 2026-10-20

Build: the CI artifact `folio-0.1.0-macos-arm64.zip` from run 39491397516, commit `2c367ca`, with bundled dylibs (poppler 26.10).
I also made a local dev build of the same commit for `cargo test` and `--debug`.
Machine: Apple Silicon, macOS 27, one 2560×1440 external display at 1× (not Retina).

Method:
- **Isolation.** Each instance ran with throwaway `XDG_CONFIG_HOME`/`XDG_DATA_HOME` and was started with `open -g -n --env …` so it wouldn't take focus.
- **Driving.** Instances were driven by `folio --remote` and a test `init.lua` that logs PASS/FAIL lines. UI results were checked with in-app screenshots.
- **Real input.** Keys and mouse were tested with events sent to the focused window: System Events keystrokes, plus a small Swift CGEvent tool for scroll, drag and clicks.
- **Test PDFs.** Two arXiv papers (36 and 53 pages), Rudin's *Principles of Mathematical Analysis* (330 pages), 't Hooft's GR notes (59 pages, no outline), and two 1400+ page textbooks (up to 195 MB).

**Summary:** the Mac build works well. Every command except `open_url` was run, and so were every Lua API function, all six hooks, the `config.toml` options, remote control, auto-reload, and the key, mouse and Cmd paths.
There are **2 high-severity bugs**: **Cmd-Q loses your reading position**, and **one crash in auto-reload**.
There are also **5 medium** and **15 low-severity** issues, mostly small or doc-level. Two items carried over from the last report are resolved: the clipboard works on macOS, and all 7 earlier fixes still hold on macOS.

## Bugs

| # | severity | bug | cause / fix |
|---|---|---|---|
| 1 | **high** | **Cmd-Q, Dock → Quit and logout don't save the reading position or run the `quit` hook.** On reopen you land wherever the last 8-second autosave put you. Reproduced 4 of 4 times: on page 36, the saved page was 33. Ctrl-Q, `:quit` and the red close button all work. | Cmd-Q is handled by the native app menu (`terminate:`), not by folio's keymap, and so is the Apple Event quit. Neither raises `close_requested`. eframe calls `App::on_exit` on every exit path, but folio doesn't implement it. Fix: implement `on_exit` with `save_all()` and `emit_quit()`. |
| 2 | **high** | **Crash (SIGSEGV) during auto-reload** while the PDF was being rewritten. Seen once. I couldn't reproduce it in about 200 more rewrites: in-place, atomic, progressive and random partial writes. | The crash is in poppler `XRef::constructXRef`, called from `Session::reload` → `doc::open` on the UI thread. Report: `~/Library/Logs/DiagnosticReports/folio-2026-10-20-221547.ips`. `file_changed()`'s doc comment says it checks the file "looks complete", but it only compares mtimes, so a file that pdflatex is still writing gets parsed. Fix: reload only when size and mtime are stable across two polls and the file ends with `%%EOF`. Better still, open the new document off the UI thread and swap it in once that succeeds. |
| 3 | medium | **Smart references fail on number-first theorem styles.** In Rudin, 17 of 21 references didn't resolve (`Theorem 2.36`, `Definition 3.9`, …), because the book writes `2.36 Theorem.` Apostol, Rudin's other books and many math texts use this style. | `refs::find_definitions` only accepts `Theorem 2.36` at the line start. Also accept `<label> <Kind>` (e.g. `2.36 Theorem`, `2.36 Definition`), strong when followed by `.` or `(`. |
| 4 | medium | **Dark page mode makes hyperref links nearly invisible.** Blue `[1]` / `§3.1` comes out as RGB (27, 30, 207) on (27, 30, 39), a contrast ratio of about 1.6:1. WCAG asks for at least 4.5:1, so citation numbers almost vanish. Tinted mode is readable. | The hue-preserving inversion keeps pure blue's low luminance. In `render::recolor`, raise saturated colors to a minimum lightness against `page_bg` (e.g. clamp L ≥ 0.6 after the inversion). |
| 5 | medium | **`clear` (Esc as a command) doesn't close an open picker or prompt.** A picker opened over remote or Lua stays up through any number of `clear` calls. Hints and the ruler then start underneath it. This matters for automation and the planned AI agents. Pressing the real Esc key works. | The `"clear"` branch in `run_builtin` resets search, selection, hints, peek and pending refs, but not `self.picker` or `self.prompt`. Close those too (canceling a Lua prompt or picker with `nil`, as Esc does). |
| 6 | medium | **Stale sockets on macOS.** After a crash, `kill -9` or a plain `kill` (SIGTERM), `folio-<pid>.sock` stays in `$TMPDIR`, and `folio.sock` still points at it. `--remote` then fails with "Connection refused" even though another instance is running. SIGTERM also skips saving. | Cleanup checks `/proc`, so it's Linux-only. Use a portable check: connect to each `folio-*.sock`, and remove it on `ECONNREFUSED`. In the client, if `folio.sock` is dead, fall back to the newest live `folio-*.sock`. Handle SIGTERM by requesting a normal close. |
| 7 | medium | **The documented minimum Rust version is wrong.** `BUILDING.md` and `Cargo.toml` (`rust-version = "1.88"`) say 1.88, but egui/eframe 0.37 need **1.95** (and poppler-sys 1.92). The build on this Mac failed until I installed 1.95. | Set `rust-version = "1.95"`, update `BUILDING.md` and the "Rust ≥ 1.88" note in `CLAUDE.md`. |
| 8 | low | `folio.state().mode` only ever returns `normal` or `visual`, never the HINT, PEEK, RULER or INPUT shown in the status bar and documented in `USAGE.md`. | `Mode` has two variants. The pill is derived from `hints`/`peek`/`ruler`/`prompt`. Derive `mode` for the snapshot the same way. |
| 9 | low | **`config.toml` mistakes are silent.** Unknown settings (`bogus_key`, a typo such as `hint_char`) are ignored. Bindings to commands that don't exist (`q = "nosuchcmd"`) are accepted. Malformed key names (`"<Bad"`, `"<C-"`) bind as literal characters, and `"<Foo>"` is accepted too. Wrong *types* are reported well (an error toast with the line). | Use `#[serde(deny_unknown_fields)]`, or collect unknown keys with `serde_ignored` and warn about them. Check binding targets against built-ins plus Lua commands after `init.lua` runs. Treat an unclosed `<…` as an error. |
| 10 | low | **Bad values are accepted without warning.** `highlight nosuchcolor` creates a highlight with an unknown color. `page_mode drak` resets to normal. `folio.set("theme.accent", "notacolor")` is ignored without a message. `goto_page 0` goes to page 1, while `goto_page 999` warns. | Validate and warn, like `theme bogus` already does. |
| 11 | low | `folio.page_text(-1)` and `page_text(2.5)` raise a Lua error. The docs say the result is `nil` outside `1..page_count`. | Take an `i64` (or a Lua number) and range-check it. |
| 12 | low | A caption is offered as a reference to itself. In 't Hooft the `Fig. 1.` caption gets a hint label, and picking it gives "not found". | In `refs::detect`, skip figure, table and theorem matches at the start of a line that are followed by `.` or `:`. |
| 13 | low | Without an outline, `Section 3.5` resolves to the table-of-contents line `3.5 Generalizations 29` (p2), not the section itself. This only affects PDFs without bookmarks. I saw it with `--debug findref`; the app tries the outline first. | Skip lines that end in a page number, or prefer a later match whose next line isn't another TOC entry. |
| 14 | low | Palette: typing `zoo` and pressing Tab completes to `zoom_in`, not the exact command `zoom`. | Rank exact and shorter prefix matches first. |
| 15 | low | `sel_line_next` starting on a section number (`2` in `2 Strings and branes`) moves to the `S` on the same visual line. Poppler splits the heading into two "lines" because of the wide gap. | Group `lines()` by baseline (y overlap) before using them for motions. |
| 16 | low | The status bar shows empty `()` on pages with an empty page label (Rudin p3). | Hide the label when it's empty. |
| 17 | low | `USAGE.md` says the remote socket is in `$XDG_RUNTIME_DIR`. On macOS it falls back to `$TMPDIR`. If `XDG_RUNTIME_DIR` is set to a path longer than about 90 characters, the socket can't be created (`SUN_LEN`). Remote control is then disabled, with only a line on stderr. | Fix the doc. Show a warning toast when the socket fails to bind. |
| 18 | low | `--remote` exits 0 for unknown commands and errors. The `TODO.md` item on remote replies covers this. | — |

Cosmetic: one clippy warning (`Rgba::rgba` has the same name as its type). `folio FILE --version` ignores the flags after the file and opens the GUI. A missing file also opens the GUI, with an error toast.

## Design gaps (unchanged, or confirmed on macOS)

- **Opening from Finder** still doesn't load the file: the window opens empty. This is the known `odoc` gap in `TODO.md`. With bug 1, it's the main thing that makes folio feel un-Mac-like.
- **Ctrl/Cmd + scroll zoom is very sensitive.** Ten 60-px wheel events went from 139% to the 1200% maximum. This may be fine for a trackpad; check by hand with a mouse wheel.
- **A number in `goto_page` is always the physical page.** In books, `:8` doesn't go to the page printed "8" (in Rudin that's PDF page 20). Labels like `viii` work. Consider a printed-page syntax such as `:p8`.
- **The `selection` hook still fires on every extension** (3 events for `v w w`). The debounce item is still open.
- **Marks still can't be deleted.** Ctrl-d in `gm` does nothing (`TODO.md`).
- **`v` always starts at the first word on screen** (`TODO.md`).

## Couldn't be tested here

- **Retina sharpness.** The only display is 1×. At 1× tiles map 1:1 and look sharp, including at 400%. Check on a Retina screen.
- **Trackpad gestures** (pinch zoom, smooth two-finger scrolling) can't be synthesized. They were checked by hand and work.
- The **Intel build** and a Mac **without Homebrew** (the bundled-dylib case). `otool -L` shows that every non-system library resolves into `Contents/Frameworks`.
- `open_url`. I didn't run it, so that no browser would open.

## What works

### Platform (macOS)
- The bundled `.app` runs. After the quarantine flag is cleared, no Homebrew is needed at runtime.
- **Cmd works as Ctrl**: `Cmd-d`/`Cmd-u`/`Cmd-f`/`Cmd-p`/`Cmd-=`/`Cmd--`, `Cmd-c` in visual mode. Plain Ctrl still works (`Ctrl-d`/`Ctrl-u`/`Ctrl-o`/`Ctrl-i`/`Ctrl-n`/`Ctrl-c`/`Ctrl-q`).
- Font discovery: the UI font renders correctly with no fontconfig.
- **Clipboard**: `y`, `Y`, visual `Cmd-c`/`Ctrl-c`, `folio.clipboard` and export all reach the system pasteboard. This closes the open item from the Lua report.
- The red close button saves and runs the `quit` hook.
- A background window (`open -g`) still processes remote commands and renders (App Nap doesn't stall it). Idle CPU is 0.1%.
- `folio.spawn` inherits a full `PATH`, so bare `brew` resolves (when launched with `open`).
- Fullscreen fills the display.

### Reading and navigation
- `j`/`k`/`h`/`l`, half and full screens, Space/Shift-Space/PageUp/PageDown.
- `J` repeatedly, plus `3J`; `K` (first to the page top).
- `gg`/`G`, `12gg`/`20gg`, the lone-`g` timeout, `gp`, `:17`, `goto_page`.
- Page labels (`viii`, `vii`) work.
- Zoom: `+`/`-`/`w`/`W`/`z0`/`zoom N`/`zoom N%`, clamped at 10–1200%. `move_left`/`move_right` pan.
- The jump list works with `Ctrl-o`/`Ctrl-i`, Backspace/Tab, and across history.
- Position, zoom and fit mode are restored on reopen; `restore_position = false` turns that off.
- Large documents:
  - 1479 pages / 195 MB opens in about 0.3 s.
  - 280 MB RSS after opening, peaking at about 750 MB while jumping through the book and settling around 520–585 MB.
  - Sharp tiles about 0.5 s after a zoom to 400%.

### Search
- Incremental `/` and `?` via keys. Enter keeps the search, and Esc cancels and returns to the starting page.
- `↑` recalls history.
- `n`/`N` wrap. Backward search reverses `n`.
- The status counter (`"Hawking" 6/16`) is correct.
- Smart case and ligatures (`ﬁeld`) work.
- A rapid re-search keeps the last query, and a search with no hits is handled cleanly.
- Speed: about 0.15 s for 53 pages and 4.5 s for 1479 pages (headless), and results stream in.

### Links, smart references, peek
- `f` + label follows links. Right-click peeks a link, and left-click follows it.
- `s` + label jumps (a citation to the bibliography, p6 → p45), and Ctrl-o returns.
- An uppercase label, or `S`/`F`, peeks. Inside the peek, `j`/`k`/`J` move and Enter jumps.
- Esc and a non-label key cancel hints.
- Resolution (headless, sample pages):
  - **arXiv paper:** 34 of 34 citations, equations and sections resolved.
  - **'t Hooft notes (no outline):** 24 of 25 equations resolved. The miss is bug 12.
  - **Rudin:** only `(3.21)`-style equations resolved (bug 3).

### Selection, highlights, bookmarks, marks
- Visual mode:
  - motions `w e b l h j k $ 0 ) o`
  - `y`, `H`, `highlight red`
  - `a` with a typed note; Esc cancels the note
  - `*` / `search_selection`
  - normal-mode navigation still works
- Mouse: drag-select enters visual mode, a click clears it, and a double-click selects a word.
- Highlights draw over the text, and notes show a marker. `gh` lists them, and Ctrl-d deletes.
- `b` suggests the current section title. `B` lists bookmarks.
- Marks:
  - local `m a` / `'a` and `''`
  - global `B` within the same document
  - global `R` reopens the other document
- `export_highlights` writes Markdown with highlights, notes and bookmarks.
- Everything persists to JSON and is restored.

### UI
- Themes `midnight`/`paper`/`nord`/`gruvbox`, plus `T` and `:theme`. A bogus theme gives a clear warning.
- Dark and tinted pages (`i`/`I`/`page_mode`).
- Sidebar, text panel, status bar toggles; the TOC opens on the current section.
- Palette, help, marks, bookmarks, highlights, recent files and the file picker.
- The ruler: `r`, `j`/`k` line steps, `zz`.
- Toasts: a cap of 8, duplicates collapse, long messages are truncated, and Lua errors are shown without the traceback.

### Lua API and automation
- `state()` (all fields), `page_text()` / `(n)` / `(0)` / `(count + 1)`, `outline()`, `highlights()`, `selection()`.
- `command` with args; `map` to a command or a function, with counts (`3<Space>f` → count = 3).
- `exec` is deferred, as documented.
- **Self-recursion** is stopped with "command loop detected", and the app survives.
- `require` from `lua/`, `folio.version`, `folio.config_dir`.
- **Hooks:** `open`, `page` (after the restore), `selection`, `search` (prompt and remote), `highlight`, and `quit` on `:quit`, Ctrl-Q and the close button. Not on Cmd-Q (bug 1).
- An error in a hook or command shows a toast and other hooks still run. `reload_config` resets globals without duplicating hooks.
- `prompt` (initial text, typed input, `nil` on Esc) and `pick` (`(index, item)`, filtering, arrow keys, `nil, nil` on Esc).
- `show`/`append`, `notify` levels, `print`.
- `spawn`: stdout, stderr, exit code, 2 MB of stdin, a missing binary → `-1`, empty argv → error, and the async callback.
- `folio.set` for every listed option. Unknown options and theme keys give warnings.
- `--remote`:
  - Lua containing `;`, and 100 KB arguments
  - several instances via `FOLIO_SOCKET`; the default socket goes to the newest instance
  - a clear error when nothing is running
- Auto-reload:
  - in-place rewrites, atomic renames, progressive writes and partial writes (it keeps the old document until the file is valid)
  - deletion and reappearance of the file
  - 6 rapid rebuilds settle on the final file; the position is kept
  - `auto_reload = false` turns it off; `R` still works
- `config.toml`:
  - theme preset and overrides, `page_mode`, an extra highlight color
  - `hint_chars = "jkl;"`, `scroll_step`, `page_gap`
  - bindings with arguments, `""` unbinds
  - a type error shows a precise toast and keeps the previous config
- `--init-config` (won't overwrite), `--print-config`, `--print-init`. `--list-commands` matches `docs/COMMANDS.md`.

### Unit tests
- `cargo test`: 6 of 6 pass with Rust 1.95.

## Test method notes for macOS (for the next pass)

- On macOS a socket path must be at most 104 bytes. Point `XDG_RUNTIME_DIR` at a short directory, or leave it unset (`$TMPDIR`) and target an instance with `FOLIO_SOCKET=$TMPDIR/folio-<pid>.sock`.
- `open -g -n -a folio.app --env XDG_CONFIG_HOME=… --env XDG_DATA_HOME=… --stderr log --args file.pdf` starts an isolated instance without taking focus.
- Real keys: `osascript -e 'tell application "System Events" to keystroke "J"'` after making the process frontmost. Mouse, scroll and right-click need CGEvents (a 40-line Swift script). Terminal needs Accessibility permission.
- `folio FILE` with no XDG variables uses the **real** config and data, even for a nonexistent file. Always set both.

## Suggested order

1. Bug 1 (`on_exit`). It's a few lines and affects every Mac session.
2. Bug 2 (stable-file check before reload, and off-thread open), plus bug 6 (portable socket cleanup, SIGTERM).
3. Bug 3 (number-first theorems) and bug 4 (dark-mode link contrast): the two that affect reading math and physics the most.
4. Bug 5 and bug 7, then the low-severity validation items (9–11), which can be batched.
5. The Finder `odoc` handler. Then turn this harness into `folio --test-script` so the platform checks run in CI.
