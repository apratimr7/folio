# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and versions follow [Semantic Versioning](https://semver.org/) (pre-1.0: minor versions may break config or APIs).

## [Unreleased]

### Added
- `:p42` goes to the page printed 42 (its label); `:42` stays the physical page.
- `alternate_file` (`Ctrl-6`, like vim's `Ctrl-^`) switches to the previously opened document.
- `reveal_file` shows the document in the file manager; `copy_path` copies its path.
- The recent list removes an entry with `Ctrl-d` and says how many missing files it hides.
- `rotate` (`>` clockwise, `<` back) turns the pages by 90°; the rotation is saved per document, and text,
  search, links and highlights follow it.
- Password-protected PDFs ask for their password (masked, never saved); `folio.open(path, password)` in Lua.

### Fixed
- `folio --remote` waits for each command and exits with status 1 (printing the message) when one fails;
  output such as `lua` results is printed to stdout.

### Changed
- CI: the Rust toolchain is pinned (1.95), tests run in release mode so MuPDF compiles once per job, docs-only
  pushes skip the build, a new push cancels the previous run, and the actions are on Node 24.

## [0.2.0] — 2026-10-21

First public release. folio now uses MuPDF (and is licensed AGPL-3.0-or-later), runs on Linux, macOS and Windows,
and comes with a test suite. Highlights: smart references for Rudin-style books and left-numbered equations,
choosing where a selection starts with row → word hints, readable links in dark mode, and many fixes from a full
macOS test pass.

### Added
- **Windows build.** folio builds and runs on Windows (x86_64, MSVC); CI tests it and uploads a zipped `folio.exe`.
  Config lives in `%APPDATA%\folio` and data in `%LOCALAPPDATA%\folio`. Remote control isn't available on Windows yet.
- `v` lets you start a selection anywhere on screen: pick a row (labels in the left margin), then a word on it
  (the rest of the page dims). `v` `v` starts at the first word on screen as before; a search match on screen or the
  ruler still starts right away.
- Drop a PDF onto the window to open it.
- Marks can be deleted: `Ctrl-d` in the marks list (`gm`), `delete_mark {letter}` and `delete_marks` (all of the
  document's lowercase marks).
- An automated test suite (`cargo test`): about 135 unit and CLI tests covering the MuPDF backend, search, smart
  references, state files, config, commands and the Lua API. The tests run on small fixture PDFs built from LaTeX
  sources in `tests/fixtures/`. See `docs/TESTING.md`.

### Changed
- The window size and position are remembered between sessions (`window.ron` in the data directory).
- **PDF engine: MuPDF replaces poppler.** Rendering is about 40% faster and text extraction (search, smart
  references) about 2× faster. MuPDF is compiled in statically, so folio has no runtime dependencies: no Homebrew
  poppler on macOS, no `poppler-glib`/`cairo` packages on Linux, and the macOS app no longer bundles dylibs.
  Building needs a C compiler, `make` and libclang. Text lines are joined across MuPDF's wide-gap splits, and
  columns stay apart in reading order. Password-protected PDFs now give a clear "password-protected" error.
- **License: GNU AGPL v3.0 or later** (required by MuPDF). See `LICENSE`.
- Minimum Rust is 1.95 (egui 0.37 needs it; the docs said 1.88).
- CI: no system PDF libraries, newer `actions/checkout`/`upload-artifact` (Node 20 deprecation), and a build cache.
- `folio.state()` is filled in while `init.lua` runs, so `mode` and `theme` are no longer empty.
- The `search` hook also fires for `search …` run from Lua or `--remote`.
- The first `page` hook fires after the saved position is restored, not for page 1.
- `--remote` doesn't split `lua …` lines on `;`.
- `folio.prompt` calls back with `nil` when canceled. `folio.pick` passes `(index, item)`, or `nil` when canceled.
- PDF links only open `http`, `https` and `mailto` URLs. Other targets (`file:`, custom schemes, paths) aren't
  opened; folio shows a warning and copies them. `open_url` applies the same rule.
- The remote-control socket is private (mode 0600). If it can't be created (e.g. a path over 104 bytes on macOS),
  a warning toast says so.
- Config mistakes are reported: unknown `config.toml` settings, malformed keys (`"<C-"`, `"<Bad"`), unknown key
  names (`<Foo>`), bindings to commands that don't exist (checked after `init.lua`), and unknown `[keys.<mode>]`.
- Bad values warn instead of being accepted: `highlight <unknown color>` (no highlight is made), `page_mode <unknown>`
  (mode unchanged), `folio.set("theme.<key>", <not a color>)`, `goto_page 0`.

### Fixed
- Smart references: keywords hyphenated across a line break (`Defi-` / `nition 3.9`) are detected. Left-numbered
  equations (`leqno`, Rudin) resolve, and their numbers aren't hinted as references; a sentence that starts with
  `(2.1)` still is (told apart by the wide gap after an equation number).
- A PDF with no pages (typically one pdflatex is still writing) fails to open with a clear message instead of
  opening empty.
- `kill` (SIGTERM), SIGHUP and Ctrl-C in the terminal close folio normally: the position is saved, the `quit` hook
  runs and the remote-control socket is removed.
- Dark pages: hyperref blue (and other saturated) links were nearly invisible (about 1.6:1 contrast). Strongly
  colored pixels are now lifted to a readable luminance with their hue kept (≥ 4.5:1, unit-tested).
- The `mupdf` bindings crashed on damaged outlines (an item without a title) and panicked on links MuPDF can't
  resolve. Patched in `vendor/mupdf` (see `FOLIO-PATCHES.md` there).
- macOS: Cmd-Q, Dock → Quit and logout save the reading position and run the `quit` hook (`App::on_exit`).
- Auto-reload waits until the file looks complete (size and mtime stable across two polls, `%%EOF` at the end), so
  a PDF that's still being written isn't parsed (this crashed poppler once).
- Smart references: number-first theorem styles (`2.36 Theorem.`, Rudin/Apostol) resolve. Captions and theorem
  headers (`Fig. 1.`, `Theorem 2.3:` at a line start) are no longer hinted as references to themselves. Without an
  outline, `Section 3.5` prefers the heading over its table-of-contents line.
- `clear` also closes an open picker or prompt (Lua callbacks get `nil`, as with Esc), and works without a document.
- Stale remote-control sockets are cleaned up on every platform (probed by connecting instead of `/proc`), and
  `--remote` falls back to the newest live instance when `folio.sock` is dead.
- `folio.state().mode` reports `hint`, `peek`, `ruler`, `input` and `picker` like the status bar, not only
  `normal`/`visual`.
- `folio.page_text(n)` returns `nil` for negative and non-integer `n` instead of raising an error.
- Palette: an exact command name ranks first, then shorter prefix matches (`zoo` + Tab → `zoom`).
- The status bar no longer shows `()` for an empty page label.
- `J` / `next_page` moved only one page; later presses stayed put. It now steps every time, including during smooth
  scrolling, and with counts (`3J`).
- A Lua command that called itself through `folio.exec` crashed folio (stack overflow). Runaway chains are now
  stopped with a "command loop detected" error.
- `<C-x>`, `<C-c>` and `<C-v>` (Cmd on macOS) can be bound. egui delivered them as clipboard events. The visual-mode
  `Ctrl-c` copy works again.
- The `quit` hook runs when the window is closed (not only for the `quit` command), and only once.
- `folio.page_text(0)` returns `nil` instead of page 1.
- Hint labels can use any characters from `hint_chars` (e.g. `;`), not just letters and digits.
- Notifications: up to 8 are kept, repeats collapse into one, long messages are truncated so they stay on screen,
  and Lua errors show the message without the stack traceback (the full trace goes to stderr).

## [0.1.0] — 2026-10-19

First release.

### Reading
- Continuous vertical layout with fit-width (default), fit-page, free zoom, and smooth keyboard scrolling.
- Tiled, multi-threaded poppler rendering with center-out priority, neighbor prefetch, low-res placeholders
  during zoom, and a GPU texture budget.
- Position, zoom and fit mode restored per document. Auto-reload when the file changes on disk.
- Page labels (roman numerals etc.) in the status bar and for `goto_page`.

### Keyboard
- Modal vim-style engine: multi-key sequences, counts, `{char}` arguments, ambiguity timeout.
- Normal and visual modes. All defaults rebindable in `config.toml`.
- Jump list (`Ctrl-o`/`Ctrl-i`), local and global marks, bookmarks.

### Navigation for technical documents
- Link hints (`f`/`F`) for every hyperlink on screen.
- Smart references (`s`/`S`): citations, figures, tables, equations, theorem-like environments and sections,
  detected from the text and resolved heuristically, so they work in PDFs without hyperlinks.
- Peek: a floating preview of a link or reference target, scrollable, with jump-to.
- Fuzzy table of contents opened at the current section, and an outline sidebar.

### Search and text
- Incremental, streaming full-document search with smart case and ligature/hyphenation/quote normalization.
- Keyboard text selection by char/word/line/sentence, mouse drag and double-click.
- Highlights with colors and notes, Markdown export, copy page text.
- Reading ruler with line-by-line movement.

### Interface
- Themes `midnight`, `paper`, `nord`, `gruvbox`, every color overridable. Hue-preserving dark pages and
  theme-tinted pages.
- One fuzzy picker for commands, TOC, files, recent documents, highlights, bookmarks, marks, themes and key help.
- Status bar (mode, section breadcrumb, page/label, zoom, search state, pending keys), toasts, side text panel.

### Extensibility
- Lua 5.4 scripting: commands, key maps, event hooks (`open`, `page`, `selection`, `search`, `highlight`, `quit`),
  document access, prompts/pickers, the text panel, async `spawn`.
- Remote control: `folio --remote '<command>'` over a Unix socket.
- Debug tooling: `--debug {render,outline,text,links,refs,findref,search}`, `--list-commands`, `screenshot`.

### Platforms
- Linux (Wayland/X11).
- macOS: build script producing an ad-hoc-signed `folio.app` (optionally with bundled dylibs), and CI on
  arm64/x86_64.
