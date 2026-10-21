# TODO

Short-term work and known issues. Longer-term direction is in [ROADMAP.md](ROADMAP.md).

## Known issues

- [ ] **macOS: opening from Finder** doesn't load the file, because the Apple Event `odoc` isn't handled (winit
      doesn't expose it). Workaround: `open -a folio --args file.pdf`. Fix: a small `objc2` hook on
      `application:openURLs:`.
- [ ] CI passes (Linux, macOS arm64/x86_64), but the workflow's actions use Node 20 and need bumping
      (`actions/checkout`, `actions/upload-artifact`).
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
- [ ] **Visual mode can't start where you want.** `v` starts at the search match, then the ruler line, then the
      first word on screen (usually the top). Add hint-based placement: `v` labels lines/words on screen and the
      chosen label sets the start (like `f` for links). Keep the current fallbacks for when no hint is chosen
      (e.g. `v` `v`). Also: no way to move the selection start without extending (only `o` swaps ends); consider a
      caret mode where motions move a cursor until a second `v` anchors it.
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
- [ ] **7. Minimum Rust is 1.95, not 1.88** (egui 0.37). Fix `rust-version` in `Cargo.toml`, `BUILDING.md`,
      `CLAUDE.md`.

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
- [ ] 18. `--remote` exits 0 on unknown commands and errors (see `--remote` replies below).
- [ ] Ctrl/Cmd + wheel zoom is very sensitive with synthetic wheel events (10 events: 139% → 1200%). Pinch zoom
      on the trackpad feels fine; check with a real mouse wheel.
- [ ] Cosmetic: `folio FILE --version` ignores flags after a file. (The `Rgba::rgba` clippy warning is fixed.)

## MuPDF follow-ups

- [ ] Upstream the two `vendor/mupdf` fixes to mupdf-rs, then drop the vendored copy.
- [ ] Share one document across render threads (MuPDF display lists + cloned contexts) instead of opening it
      per thread; cuts memory on large books.
- [ ] Use MuPDF's block structure for column-aware selection and smart references in two-column papers.
- [ ] EPUB/CBZ/XPS: enable the crate features (EPUB needs reflow `layout()` and a font size setting).
- [ ] Write highlights into the PDF as real annotations (`pdf` module).

## Missing basics

- [ ] **Password-protected PDFs:** they now fail with a clear "password-protected" error; add a prompt
      (`Document::authenticate` in MuPDF) and a `--password` / Lua option.
- [x] **Drag and drop** a PDF onto the window (not yet checked with a real Finder drag).
- [x] **Remember window size and position** (eframe persistence, `window.ron` in the data dir). Saving is
      verified; restoring couldn't be observed here because a tiling window manager (AeroSpace) places windows.
- [ ] **Printing** (or "open in system viewer to print" as a stopgap).
- [ ] **Scrollbar / position indicator** you can drag, at least on hover.
- [ ] **App icon** (macOS `.icns`, Linux icon for the `.desktop` file).
- [ ] **Set as default PDF viewer** works only after the Finder `odoc` fix (Known issues).
- [ ] Printed page numbers: `:8` is always the physical page; add a syntax for the printed page (e.g. `:p8`).

## Next up

- [ ] **New window and new tab.** Open another document (or the same one) in a new window or a tab, with
      commands for both (`new_window`, `new_tab`, `next_tab`/`prev_tab`, `close_tab`) and keys. `--remote` and
      Lua need to address a window/tab. ROADMAP "Later" also lists a split view of the same document.
- [ ] **Windows build.** The code is mostly portable. Needs: a named-pipe (or TCP localhost) variant of `ipc.rs`,
      config/data dirs (`%APPDATA%`), poppler from vcpkg or MSYS2, font discovery, and a CI job that uploads a
      zipped `.exe` with its DLLs.
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
- [ ] Tests for `refs.rs` detection/definitions on a small corpus of generated PDFs.
- [ ] Packaging: AUR `PKGBUILD`, Homebrew formula, `.desktop` file and icon.

## Later

- [ ] **UI appearance config:** translucent and blurred panels/background (macOS vibrancy, KDE/Hyprland blur
      via a transparent window), panel opacity, shadows and animation settings, set in a `[ui]` section of
      `config.toml` and from Lua.
- [ ] **Project website** (last): what folio is, screenshots/short clips of smart references, peek and Lua
      automation, install instructions per platform, links to the docs and GitHub releases.
