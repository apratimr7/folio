# Architecture

```
            keys ─┐   palette ─┐   Lua ─┐   folio --remote ─┐
                  ▼            ▼        ▼                   ▼
             ┌─────────────────────────────────────────────────┐
             │   command registry  (commands.rs, app.rs)       │
             └───────────────┬─────────────────────────────────┘
                             ▼
     ┌──────────── Session (session.rs) — one open document ───────────┐
     │ View (layout, camera)   caches: textures, page text, links      │
     │ search · selection · hints · peek · ruler · DocState (persisted) │
     └───────┬───────────────────────────────┬─────────────────────────┘
             │ wanted tiles                  │ search / find-ref requests
             ▼                               ▼
   RenderPool (render.rs)              TextWorker (text.rs)
   N threads, each with its own        own document; streams search hits,
   Backend; renders TileKeys           resolves smart references
             ▼                               ▼
                    Backend trait (doc/mod.rs)
                    └─ PopplerDoc (doc/poppler.rs, poppler-glib + raw FFI)
```

## Modules

| file | responsibility |
|---|---|
| `doc/mod.rs` | `Backend` trait and document types: `PageText` (one box per char), `Link`, `OutlineItem`, `DocPos`. All coordinates are PDF points with a top-left origin. |
| `doc/poppler.rs` | poppler implementation. The safe bindings cover rendering; text layout, link mappings and outline actions use raw FFI (`poppler-sys`). |
| `render.rs` | render worker pool and page recoloring (dark / tinted modes) |
| `text.rs` | text worker: full-document search with normalization, reference lookup |
| `refs.rs` | smart reference detection (regexes over page text) and definition heuristics |
| `view.rs` | continuous vertical layout, zoom/fit, scroll animation. Pure and unit-tested. |
| `keymap.rs` | key chords, vim-notation parser, `Dispatcher` (sequences, counts, `{char}` args, timeouts). Unit-tested. |
| `commands.rs` | built-in command table (name, argument hint, description) |
| `app.rs` | `App` state, command execution, pickers, prompts, hint/peek/selection logic |
| `session.rs` | `Session`: everything tied to one open document |
| `script.rs` | Lua runtime: the `folio` API, effect queue, hooks, async processes |
| `store.rs` | JSON persistence of per-document state and global data |
| `ipc.rs` | Unix-socket server and `--remote` client |
| `ui/frame.rs` | the egui frame loop: input routing, worker polling, canvas painting, panels, peek, toasts |
| `ui/picker.rs` | the fuzzy picker overlay (nucleo matcher) |
| `ui/style.rs` | theme → egui visuals, font discovery |
| `config.rs` | `config.toml` schema, theme presets |

## Key design decisions

**Everything is a command.** Key bindings, the palette, Lua, remote control and (later) AI agents call the same
named commands with string arguments. A new feature only has to be added once to be reachable from all of them.

**Backends are not `Send`.** poppler/GObject objects stay on the thread that created them, so each worker opens its
own document. The UI thread keeps one too, for cheap synchronous queries (page text, links).

**Tiles, not pages.** Pages are rendered in 1024² device-pixel tiles keyed by
`(page, scale, tile x/y, color mode, generation)`. Each frame the UI publishes the full list of tiles it wants,
most important first: thumbnails of visible pages, then visible tiles from the center out, then neighbors.
Workers always take the first tile not already in flight, so stale requests (scrolled past, old zoom) are
dropped without explicit cancellation. While a sharp tile is missing, any lower-resolution texture of the page
(including a small thumbnail) is drawn stretched underneath, so zooming never shows blank pages. Unused
textures are evicted after a few seconds or when over budget (320 MB).

**Generations.** Reloading a file bumps a generation number. Old tiles and in-flight results are ignored, and
workers reopen the file lazily.

**Scripts queue effects.** Lua functions read a snapshot and push `Effect`s. The app applies them after the call
returns, so scripts can't observe or create half-updated state, and there are no `RefCell` re-entrancy panics.

**State keyed by path.** Per-document data is keyed by canonical path rather than content hash, so a PDF that is
rebuilt by LaTeX keeps its highlights and position.

## Smart references

`refs::detect` runs regexes over a page's text (with a byte→char map back to per-char boxes) and returns
`(kind, label, span)`. When one is picked, the text worker scans pages for a *definition*:

- **citation**: a line starting with `[N]` (strong), or `N.`/`N Author` on a page that looks like a bibliography
  (weak). The scan starts at the citing page and wraps, so per-chapter bibliographies resolve correctly.
- **figure/table**: a line starting with `Figure N`/`Fig. N`/`Table N`. Strong if followed by `:`/`.`.
- **equation**: a line ending with `(N)`, the equation number.
- **theorem-like**: a line starting with `Theorem N`, etc.
- **section**: the outline first, then a line `N Title`.

The reference itself is excluded, so a figure caption doesn't resolve to itself.

## Rendering pipeline

`PopplerDoc::render(page, scale, x, y, w, h)` paints into a cairo `Rgb24` surface with a white background,
translated and clipped to the tile, then converts it to RGBA. `render::recolor` applies the page color mode.
The UI uploads results as egui textures (linear filtering). Page origins are snapped to the physical pixel grid,
so tiles at the native scale map 1:1 to screen pixels.

## Testing

- `cargo test`: key parsing and dispatch, and layout math.
- `folio --debug {render,outline,text,links,refs,findref,search} FILE …`: headless backend checks.
- `folio --remote 'screenshot out.png'`: captures the real window contents, for visual checks.
