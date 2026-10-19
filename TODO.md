# TODO

Short-term work and known issues. Longer-term direction is in [ROADMAP.md](ROADMAP.md).

## Known issues

- [ ] **macOS: opening from Finder** doesn't load the file, because the Apple Event `odoc` isn't handled (winit
      doesn't expose it). Workaround: `open -a folio --args file.pdf`. Fix: a small `objc2` hook on
      `application:openURLs:`.
- [ ] **macOS build is untested on real hardware.** The code type-checks for `aarch64-apple-darwin`, and CI builds
      it, but nobody has run it yet. Check HiDPI (Retina) sharpness, Cmd-key bindings and font discovery.
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
