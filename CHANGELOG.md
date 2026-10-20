# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and versions follow [Semantic Versioning](https://semver.org/) (pre-1.0: minor versions may break config or APIs).

## [Unreleased]

### Fixed
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

### Changed
- `folio.state()` is filled in while `init.lua` runs, so `mode` and `theme` are no longer empty.
- The `search` hook also fires for `search …` run from Lua or `--remote`.
- The first `page` hook fires after the saved position is restored, not for page 1.
- `--remote` doesn't split `lua …` lines on `;`.
- `folio.prompt` calls back with `nil` when canceled. `folio.pick` passes `(index, item)`, or `nil` when canceled.

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
