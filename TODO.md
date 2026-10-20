# TODO

Short-term work and known issues. Longer-term direction is in [ROADMAP.md](ROADMAP.md).

## Known issues

- [ ] **macOS: opening from Finder** doesn't load the file, because the Apple Event `odoc` isn't handled (winit
      doesn't expose it). Workaround: `open -a folio --args file.pdf`. Fix: a small `objc2` hook on
      `application:openURLs:`.
- [ ] CI passes (Linux, macOS arm64/x86_64), but the workflow's actions use Node 20 and need bumping
      (`actions/checkout`, `actions/upload-artifact`).
- [ ] **macOS build is untested on real hardware.** The code type-checks for `aarch64-apple-darwin`, and CI builds
      it, but nobody has run it yet. Check HiDPI (Retina) sharpness, Cmd-key bindings and font discovery.
- [ ] After an auto-reload where the text reflowed (an edited LaTeX source), saved highlights keep their old
      positions and char offsets. Re-anchor them by searching for their text near the old place.
- [ ] Selections can't span pages (visual mode stays on the page where it started).
- [ ] Smart references are heuristic. Known gaps: author–year citations (`Smith et al. 2020`), equation numbers
      that aren't at the end of their text line, two-column layouts where poppler merges lines from both columns.
- [ ] Bare equation references like `(2.8)` are only detected with a dot (to avoid noise like `(1)`).
- [ ] Page rotation (`/Rotate`) is handled by poppler's page size, but no rotate command exists yet.
- [ ] Reloading after a partially written file shows a warning toast. It retries until the file is complete,
      but it could debounce instead.
- [ ] Huge documents: page sizes are read up front (fine for thousands of pages; could be lazy).
- [ ] PDF link-border annotations (red hyperref boxes) are drawn as authored; add an option to hide them.
- [ ] No license chosen yet. Pick one before publishing.

## From user testing (2026-10-20)

- [ ] **Marks can't be deleted.** Add `Ctrl-d` in the `gm` marks list (like highlights and bookmarks), a
      `delete_mark {char}` command, and `delete_marks` to clear all of them. Today the only options are
      overwriting a mark or editing the state JSON.
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
  - [ ] Recheck `folio.clipboard` by hand (it can't be tested from the hidden test window).

## Next up

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
