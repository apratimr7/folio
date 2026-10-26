# TODO

Short-term work and known issues. Longer-term direction is in [ROADMAP.md](ROADMAP.md).

## Known issues

- [ ] **macOS: opening from Finder** doesn't load the file, because the Apple Event `odoc` isn't handled (winit
      doesn't expose it). Workaround: `open -a folio --args file.pdf`. Fix: a small `objc2` hook on
      `application:openURLs:`.
- [ ] **macOS: still unchecked on real hardware** after the 2026-10-20 test pass (which covered everything else, see
      below; trackpad scrolling and pinch zoom were confirmed by hand): Retina sharpness and the Intel build. (Since the
      switch to MuPDF the app has no Homebrew/dylib dependencies.)
- [ ] After an auto-reload where the text reflowed (an edited LaTeX source), saved highlights keep their old
      positions and char offsets. Re-anchor them by searching for their text near the old place.
- [ ] Selections can't span pages (visual mode stays on the page where it started).
- [ ] Smart references are heuristic. Known gaps: author–year citations (`Smith et al. 2020`), equation numbers
      that aren't at the end of their text line, two-column layouts where poppler merges lines from both columns.
- [ ] Bare equation references like `(2.8)` are only detected with a dot (to avoid noise like `(1)`).
- [ ] Page rotation (`/Rotate`) is handled by poppler's page size, but no rotate command exists yet.
- [ ] Huge documents: page sizes are read up front (fine for thousands of pages; could be lazy).
- [ ] PDF link-border annotations (red hyperref boxes) are drawn as authored; add an option to hide them.
- [x] License: AGPL-3.0-or-later (MuPDF requires AGPL compatibility).

## From user testing (2026-10-20)

- [x] **Marks can't be deleted.** Done: `Ctrl-d` in the `gm` marks list, `delete_mark {char}`, and
      `delete_marks` (the document's lowercase marks).
- [x] **Visual mode can't start where you want.** `v` now labels the rows on screen (in the margin), then the
      words of the chosen row; `v` `v` keeps the old start. Still open: no way to move the selection start without
      extending (only `o` swaps ends): caret mode is listed as optional under Later.
- [ ] **Lua API gaps:**
  - [ ] Bookmarks and marks: read, add, delete (`folio.bookmarks()`, `folio.marks()`, …).
  - [ ] Highlights: delete and recolor, plus create directly instead of only via `exec`.
  - [ ] Timers: `folio.defer(ms, fn)` / `folio.every(ms, fn)`.
  - [ ] Read config values (`folio.get(option)`), not only set them.
  - [ ] Positions: text with boxes (`folio.page_words(n)` → `{text, x0, y0, x1, y1}`), the current scroll position,
        and drawing overlays on pages from Lua.
  - [ ] Richer hook arguments: `page` as a number; `highlight` as a table `{page, text, color, note}`.
- [x] **Bugs from the Lua/automation test pass**: all fixed, see `CHANGELOG.md` [Unreleased] and
      `docs/testing/2026-10-20-lua-api-report.md`. Still open from that pass:
  - [ ] Debounce the `selection` hook (or add a `selection_done` event) for heavy handlers such as AI calls.
  - [x] Recheck `folio.clipboard` by hand. Works on macOS (`y`, `Y`, `Cmd-c`, `folio.clipboard`, export).

## From macOS testing (2026-10-20)

Details, reproduction and evidence: `docs/testing/2026-10-20-macos-test-report.md` (numbers match).

High:

- [x] **1. Cmd-Q, Dock → Quit and logout lose the reading position and skip the `quit` hook.** Fixed with
      `App::on_exit` (checked with an Apple Event quit: position saved, hook ran).
- [x] **2. Auto-reload can crash (SIGSEGV in poppler) on a file that's still being written.** `file_changed` now
      waits for size+mtime to be stable across two polls and `%%EOF` in the last 1 KB.
  - [ ] Still open: open the new document off the UI thread and swap it in on success.

Medium:

- [x] **3. Smart refs miss number-first theorem styles** (`2.36 Theorem.`, Rudin/Apostol style). Fixed; Rudin
      p61 resolves 7/7.
- [x] **4. Dark page mode makes hyperref blue links nearly invisible** (~1.6:1 contrast). Fixed: saturated colors are lifted
      to a readable luminance in `render::recolor` (unit-tested).
- [x] **5. `clear` doesn't close an open picker or prompt** (matters for remote/Lua/agents); hints then start under
      the picker. Close both, calling Lua prompt/pick callbacks with `nil`.
- [x] **6. Stale sockets on macOS** after a crash or kill: `folio.sock` points at a dead socket and `--remote`
      fails while another instance runs. Probe sockets by connecting (portable) instead of `/proc`; client falls
      back to the newest live `folio-*.sock`. Handle SIGTERM as a normal close (it skips saving today).
  - [x] Portable cleanup by connecting, client fallback, socket mode 0600.
  - [x] SIGTERM/SIGHUP/SIGINT close normally (save, `quit` hook, socket removed).
- [x] **7. Minimum Rust is 1.95, not 1.88** (egui 0.37). Fixed in `Cargo.toml`, `BUILDING.md` and `CLAUDE.md`.

Low:

- [x] 8. `folio.state().mode` is only `normal`/`visual`, never `hint`/`peek`/`ruler`/`input` like the status bar.
- [x] 9. `config.toml` mistakes are silent: unknown settings, bindings to unknown commands, malformed keys
      (`"<Bad"`, `"<Foo>"`).
- [x] 10. Bad values are accepted silently: `highlight nosuchcolor`, `page_mode drak` (→ normal),
      `theme.accent = "notacolor"`, `goto_page 0` (→ page 1).
- [x] 11. `folio.page_text(-1)` / `(2.5)` raise errors instead of returning `nil`.
- [x] 12. A figure caption (`Fig. 1.`) is hinted as a reference to itself and resolves to "not found".
- [x] 13. Without an outline, `Section 3.5` resolves to its table-of-contents line, not the section.
- [x] 14. Palette Tab-completion ranks `zoom_in` above the exact command `zoom` for `zoo`.
- [x] 15. (Fixed by MuPDF's joined visual lines.) `sel_line_next` from a section number moves within the heading (poppler splits `2   Strings…` into two
      lines). Group lines by baseline for motions.
- [x] 16. Status bar shows empty `()` for pages with an empty label.
- [x] 17. `USAGE.md` socket path is wrong on macOS (`$TMPDIR`); a socket path over 104 bytes silently disables
      remote control (stderr only). Doc fixed, bind failures show a toast.
- [x] 18. `--remote` exits 0 on unknown commands and errors. Now it waits for each command, prints output
      (`lua` results) and exits 1 with the message on failure.
- [ ] Ctrl/Cmd + wheel zoom is very sensitive with synthetic wheel events (10 events: 139% → 1200%). Pinch zoom
      on the trackpad feels fine; check with a real mouse wheel.
- [x] Cosmetic: `folio FILE --version` ignores flags after a file. (The `Rgba::rgba` clippy warning is fixed.)

## Found by the test suite

- [x] References whose keyword is hyphenated across a line break weren't detected (`refs::detect`).
- [x] A truncated PDF opened with zero pages and no error (the page tree is at the end of the file).
- [x] `--debug findref` prints an empty `«»` line for equation targets (it matches lines by the first char's y).
      Debug output only; the resolved target is correct.

## MuPDF follow-ups

- [ ] Upstream the two `vendor/mupdf` fixes to mupdf-rs, then drop the vendored copy.
- [ ] Share one document across render threads (MuPDF display lists + cloned contexts) instead of opening it
      per thread; cuts memory on large books.
- [ ] Use MuPDF's block structure for column-aware selection and smart references in two-column papers.
- [ ] EPUB/CBZ/XPS: enable the crate features (EPUB needs reflow `layout()` and a font size setting).
- [ ] Write highlights into the PDF as real annotations (`pdf` module).

## Missing basics

- [ ] **Native file dialog for opening documents** (user request, 2026-10-21). Today a file outside the current
      folder or the recent list needs its path typed or pasted into `o`, which is too much hassle. Add an
      `open_dialog` command that shows the system's own file chooser, filtered to PDFs and starting in the current
      document's folder: GTK/KDE through the XDG desktop portal on Linux, `NSOpenPanel` on macOS, the common file
      dialog on Windows. The `rfd` crate covers all three. Run it asynchronously so rendering doesn't freeze while
      it's open, and add the chosen file to recents. Decide on a key (and whether `o` should open the native dialog,
      via a `file_dialog = "native" | "builtin"` setting); keep the keyboard picker either way.
- [ ] **Opening and managing files** (review, 2026-10-21; do the first three together with the native dialog):
  - [ ] Open from the file manager: macOS Finder (`odoc`, see Known issues) and a Linux `.desktop` file so folio
        appears under "Open with" (see Packaging in Next up).
  - [ ] Single instance: `folio other.pdf` while folio is running hands the file to the open window (over the
        remote socket) instead of starting a second one; a flag to force a new window.
  - [ ] Open by URL or arXiv ID: `open_url`-style command that downloads a pasted `arxiv.org`/PDF link or an ID
        like `1409.3575`, saves it to a configurable folder and opens it.
  - [ ] Better titles in the recent list: arXiv files show their file name ("1409.3575v4.pdf") because the PDF
        has no title; take the title from the first page's largest text when the metadata is empty.
  - [ ] Recent list: `Ctrl-d` removes an entry, pin favorites to the top, and say how many missing files were
        hidden instead of dropping them silently.
  - [ ] Library search: a fuzzy picker over every PDF in configured folders (e.g. `library = ["~/Papers"]`),
        indexed in the background, so any paper opens from a few letters of its name.
  - [ ] Folder shortcuts in the keyboard file picker (home, Downloads, configured library folders).
  - [ ] Switch back to the previous document (`alternate_file`, `<C-^>` like vim) until tabs exist (see "New window
        and new tab" in Next up).
  - [ ] Load documents off the UI thread so a large PDF doesn't freeze the window (also under macOS testing,
        "open the new document off the UI thread").
  - [ ] Save dialogs: export highlights to a Markdown file (today only clipboard and panel) and let `screenshot`
        ask where to save instead of requiring a path.
  - [ ] `reveal_file` (show the document in the file manager) and `copy_path` commands.
- [ ] **Password-protected PDFs:** they now fail with a clear "password-protected" error; add a prompt
      (`Document::authenticate` in MuPDF) and a `--password` / Lua option.
- [x] **Drag and drop** a PDF onto the window (not yet checked with a real Finder drag).
- [x] **Remember window size and position** (eframe persistence, `window.ron` in the data dir). Saving is
      verified; restoring couldn't be observed on the test machine because a tiling window manager places windows.
- [ ] **Printing** (or "open in system viewer to print" as a stopgap).
- [ ] **Scrollbar / position indicator** you can drag, at least on hover.
- [ ] **App icon** (macOS `.icns`, Linux icon for the `.desktop` file).
- [ ] **Set as default PDF viewer** works only after the Finder `odoc` fix (Known issues).
- [ ] Printed page numbers: `:8` is always the physical page; add a syntax for the printed page (e.g. `:p8`).

## Configuration space (to build in one go)

Everything below is hard-coded today. Build it as one pass: settings in `config.toml`, the same names readable and
writable from Lua (`folio.get`/`folio.set`, see "Lua API gaps"), validated with warnings like the existing keys,
and documented in `docs/CONFIGURATION.md`. Current value and location in parentheses.

**First (asked for):**
- [ ] **Dimming amounts:** the visual-mode word-hint dim (alpha 90 dark / 60 light, `ui/frame.rs`), the reading
      ruler's "already read" dim (40 / 18, `ui/frame.rs`), and an on/off for each.

Hints and visual mode:
- [ ] Visual start: `hints` (row → word) | `first_word` (old behavior); whether a search match or the ruler skips
      the hints.
- [ ] Hint label font size (12.5 rows/links, 11 words), badge placement (left/above/over), outline boxes on/off.
- [ ] Separate `hint_chars` per hint kind (links, refs, rows, words).

Reading and navigation:
- [ ] Default fit for new documents (`width` today), zoom limits (10–1200%, `view.rs` `MIN_ZOOM`/`MAX_ZOOM`).
- [ ] Smooth-scroll speed (exponential factor 22, `view.rs`), Ctrl/Cmd+wheel and pinch zoom sensitivity.
- [ ] Where jumps land on screen (5–30% from the top depending on the jump, `app.rs`), and where the ruler sits.
- [ ] Jump-list size (200, `session.rs`), recent-files size (200, `store.rs`), search-history size.
- [ ] Search: smart case on/off, default direction; later regex and whole word (see Next up).
- [ ] Page layout (continuous today; single page, two-page, horizontal later).

Peek:
- [ ] Peek window size (70% of the window width), initial zoom, scroll step (40/200 pt), zoom step (1.15).

Interface:
- [ ] Toasts: lifetime (3.8 s), max count (8), position, minimum level shown.
- [ ] Status bar: height (28), which fields and their order (mode, title, section, search, zoom, page, label, %).
- [ ] Sidebar and text panel default widths and side; picker width (62%, 380–760 px) and font size (16).
- [ ] UI scale (egui zoom factor) separate from `ui_font_size`; a monospace font setting; heading/small sizes
      (×1.3 / ×0.82, `ui/style.rs`).
- [ ] Panel shadow strength, badge and panel corner radius beyond `radius`, window title format.
- [ ] Theme: follow the system light/dark setting and accent color (see Polishing).

Page rendering:
- [ ] Dark-mode link lift strength (`MIN_LUMA` 150, `render.rs`), dark-mode algorithm choice, page shadow on/off.
- [ ] Texture budget (320 MB, `ui/frame.rs`), tile size (1024, `render.rs`), cache eviction (4 s), prefetch
      distance. Sensible automatic defaults from RAM/VRAM; overrides for low-memory machines.

Files and documents:
- [ ] Autosave interval (8 s, `ui/frame.rs`), auto-reload poll interval (0.7 s) and stability check.
- [ ] Remember window geometry on/off; per-document vs global zoom.
- [ ] Highlights: default color (first of `[theme.highlight]`), Markdown export template.
- [ ] Bookmark label default (current section title).

System and safety:
- [ ] Link schemes that open without asking (`http`, `https`, `mailto` today) and the opener command
      (`open`/`xdg-open`).
- [ ] Remote control on/off and socket path; whether `--remote lua …` is allowed.
- [ ] Lua: command-loop limit (24), `selection` hook debounce (see Lua report items).

## CI and release speed

Measured on the Windows job: about 49 min cold (MuPDF compiled twice, no cache yet), 16.5 min with a warm cache.
macOS arm64 and Linux take 1–4 min warm. In order of value:

- [x] **Pin the Rust toolchain in CI** (`1.95` instead of `stable`): every new stable release changes the cache key
      and forces a full MuPDF rebuild. Bump it deliberately.
- [x] **Compile MuPDF once per run**: run the tests in release mode (`cargo test --release`) so the packaging step
      reuses the same build instead of building MuPDF again with the release profile (roughly halves a cold build).
- [x] **Skip CI for docs-only changes** (`paths-ignore: ['**.md', 'docs/**']` on `push`/`pull_request`).
- [x] **Cancel superseded runs** (`concurrency: { group: ${{ github.workflow }}-${{ github.ref }},
      cancel-in-progress: true }`), so a new push to `dev` stops the previous run.
- [ ] **Parallel MSBuild for MuPDF on Windows**: `mupdf-sys` calls MSBuild without `/m`, so the third-party
      projects build one after another. Patch it (vendor `mupdf-sys` like `mupdf`) and send the fix upstream;
      likely 2–4× faster cold Windows builds.
- [ ] Optional: build Windows only on `main`, pull requests and tags (not every `dev` push), or use larger runners.

## Documentation

Goal: good docs for users and contributors that can't drift from the code. Plan (needs a decision before
starting):

- [ ] **A documentation site with mdBook**, built from the existing `docs/*.md` (no rewrite) and published to GitHub
      Pages by a workflow on `main`: navigation, search, dark mode. It can grow into the project website later.
- [ ] **Generated reference pages**, each checked by a test so a change in code without docs fails CI (as
      `docs/COMMANDS.md` already is):
  - [ ] commands and default keys (`folio --list-commands`, exists);
  - [ ] configuration reference generated from the config schema and its doc comments (a `--list-config` that
        prints Markdown: key, type, default, description), replacing the hand-written tables;
  - [ ] Lua API reference generated from the functions registered in `script.rs` (name, arguments, returns,
        description kept next to each registration), replacing the tables in `SCRIPTING.md`.
- [ ] **Getting started** page with screenshots or short clips (smart references, peek, `v` hints, dark mode), a FAQ
      (first-run warnings, where config lives per platform), and per-platform install pages.
- [ ] **Contributor docs**: `cargo doc --document-private-items` published under the site (`/dev/`), and doc comments
      on the main types (`App`, `Session`, `Backend`, `View`, `Script`).
- [ ] Link checks for the docs in CI (`lychee` or `mdbook-linkcheck`).

## Next up

- [ ] **New window and new tab.** Open another document (or the same one) in a new window or a tab, with
      commands for both (`new_window`, `new_tab`, `next_tab`/`prev_tab`, `close_tab`) and keys. `--remote` and
      Lua need to address a window/tab. ROADMAP "Later" also lists a split view of the same document.
- [x] **Windows build.** Builds and tests in CI (`windows-latest`, MSVC) and uploads a zipped `folio.exe`
      (`scripts/build-windows.ps1`). Config in `%APPDATA%\folio`, data in `%LOCALAPPDATA%\folio`. Follow-ups:
  - [ ] First manual runs on real Windows machines (first testers): rendering, fonts (Segoe UI fallback),
        HiDPI scaling, keyboard (Ctrl bindings, AltGr layouts), file picker with `C:\` paths and backslashes,
        drag and drop, opening links, window-geometry restore, `--help`/`--debug` output in a terminal.
  - [ ] Remote control: a named-pipe transport for `ipc/` (`--remote`, `FOLIO_SOCKET`), e.g. with the
        `interprocess` crate. `ipc/windows.rs` is a stub today.
  - [ ] Clean quit on console Ctrl-C / logoff (`SetConsoleCtrlHandler` or the `ctrlc` crate), like the Unix
        signal handling.
  - [ ] Packaging: an icon and version info in the `.exe` (`winres`), optionally an installer (MSI/winget), and
        code signing to avoid SmartScreen warnings.
  - [ ] The Lua `spawn` tests are Unix-only (`sh`, `cat`); add Windows equivalents (`cmd /C`).
- [ ] Two-page / book mode, and a horizontal-scroll mode for slides.
- [ ] Presentation mode (one page, fit page, no chrome).
- [ ] SyncTeX: forward search via `--remote 'synctex file.tex:line'` and inverse search to an editor command.
- [ ] Cross-page and column-aware selection (use poppler's text attributes for column detection).
- [ ] Highlight editing: change color, edit note, delete under cursor.
- [ ] Search options: regex, whole word, search within the current section.
- [ ] Persistent search history and command history.
- [ ] Portals (sioyek-style): link a place in the text to another (e.g. a figure) and keep it in a side view.
- [ ] Export highlights into the PDF as real annotations (poppler supports writing annotations).
- [ ] Thumbnails overview grid (`gt`).
- [ ] Config hot-reload on file change.
- [ ] `folio --remote` replies (query state, return JSON) for richer editor/agent integration.
- [x] Tests for `refs.rs` detection/definitions on a small corpus of generated PDFs (`docs/TESTING.md`).
- [ ] Packaging: AUR `PKGBUILD`, Homebrew formula, `.desktop` file and icon.

## Later

Optional:

- [ ] **Caret mode for visual selection.** The row → word hints work well, so this is optional: after landing,
      motions move a caret without selecting until `v` anchors it (exact start positions, vim-like).

### Polishing

- [ ] **A sensible default config.** Once the configuration space above exists, choose defaults deliberately
      (today's values are fine for now) and ship them as the commented `config/config.toml`.
- [ ] **UI enhancements** (from the native-feel discussion):
  - [ ] Follow the system light/dark mode and accent color; respect reduced-motion and contrast settings.
  - [ ] Icons (status bar, sidebar, palette), consistent spacing and type scale, subtle shadows and short
        animations for panels and peek.
  - [ ] Thumbnails sidebar; a slim scrollbar shown on hover (also in Missing basics).
  - [ ] Native file dialog: moved to "Missing basics".
  - [ ] Screen reader support (egui AccessKit).
  - [ ] macOS: native menu bar with standard shortcuts, transparent title bar with content under the window
        buttons, SF Pro and the system accent, recent documents in the Dock menu, `Cmd-,` for settings.
  - [ ] Windows: Mica/Acrylic backdrop, dark title bar, Segoe UI Variable, system accent.
  - [ ] Linux: dark mode and accent from the desktop portal, header bar on Wayland (portal file chooser: see the
        native file dialog in "Missing basics").
- [ ] **UI appearance config:** translucent and blurred panels/background (macOS vibrancy, KDE/Hyprland blur
      via a transparent window), panel opacity, shadows and animation settings, set in a `[ui]` section of
      `config.toml` and from Lua.

### Other
- [ ] **Project website** (last): what folio is, screenshots/short clips of smart references, peek and Lua
      automation, install instructions per platform, links to the docs and GitHub releases.
