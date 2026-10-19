# folio

A keyboard-first PDF reader for technical documents, in the spirit of
[sioyek](https://sioyek.info), written in Rust.

- **Modal, vim-style keys**: counts (`12j`), sequences (`gg`), marks (`ma`, `'a`), a jump list (`Ctrl-O`/`Ctrl-I`).
- **Smart references** (`s` / `S`): hint every `[12]`, `Fig. 3`, `Eq. (2.4)`, `(2.8)`, `Theorem 1.2` or `Sect. 3.1` on screen.
  Pick one to jump to its definition, or **peek** at it in a floating preview without losing your place.
  This works even in PDFs without hyperlinks.
- **Link hints** (`f` / `F`): follow or peek any hyperlink without the mouse.
- **Fast, sharp rendering**: tiled, multi-threaded, with prefetching and low-res placeholders while zooming.
- **Incremental full-text search** with smart case, ligature/hyphenation normalization and a match counter.
- **Keyboard text selection** (`v`, then `w b j k $ 0 )`), **highlights** with notes, bookmarks, and Markdown export.
- **Reading ruler** (`r`): `j`/`k` move a focus band line by line.
- **Themes and page colors**: `midnight`, `paper`, `nord`, `gruvbox`; hue-preserving dark pages (`i`) or theme-tinted pages (`I`). Every color can be overridden.
- **Fuzzy everything**: command palette (`:`), table of contents (`t`), files (`o`), recent documents (`O`), highlights, bookmarks, marks, key bindings (`F1`).
- **Auto-reload** when the file changes, so it pairs well with `latexmk -pvc` / `typst watch`.
- **Lua scripting** (`~/.config/folio/init.lua`): custom commands, key maps, event hooks, async processes, a side text panel.
- **Remote control**: `folio --remote 'goto_page 12'` drives the running instance (editor integration, scripts, agents).

## Build

Requires Rust and the poppler-glib development files (`poppler-glib`, `cairo`, `glib2`), which most Linux distributions ship.

```sh
cargo build --release
./target/release/folio paper.pdf
folio --init-config     # writes ~/.config/folio/config.toml and init.lua
```

## Keys (normal mode)

| keys | action | keys | action |
|---|---|---|---|
| `j` `k` `h` `l` | scroll | `/` `?` `n` `N` | search, next/prev |
| `Ctrl-d` `Ctrl-u` | half page | `*` | search selection |
| `Space` `Ctrl-f` `Ctrl-b` | screen | `t` | table of contents |
| `J` `K` | next/prev page | `x` | outline sidebar |
| `gg` `G` `N gg` | top, bottom, page N | `:` `Ctrl-p` | command palette |
| `gp`, `:42` | go to page | `o` `O` | open file, recent |
| `+` `-` `w` `W` `z0` | zoom, fit width/page, 100% | `f` `F` | follow / peek link |
| `m{a-z}` `'{a-z}` | marks (`A-Z` are global) | `s` `S` | smart jump / peek |
| `Ctrl-o` `Tab` | jump back/forward | `v` | visual selection |
| `b` `B` | bookmark, list | `y` `Y` | copy selection, page |
| `H` `gh` `ge` | highlight, list, export | `r` | reading ruler |
| `i` `I` `T` | dark, tinted, next theme | `R` | reload |
| `zs` `Ctrl-n` | status bar, text panel | `F11` | fullscreen |
| `F1` `g?` | all bindings | `ZZ` `Ctrl-q` | quit |

In hint mode, type a label to jump, or type it in **uppercase** to peek. In peek, `j`/`k` scroll, `J`/`K` change page,
`Enter` jumps there and `Esc` closes. In visual mode, `w b e h l j k $ 0 )` extend the selection, `o` swaps ends,
`y` copies, `H` highlights, `a` annotates. Right-click on a link previews it. Middle-drag pans.

Every key is configurable in `config.toml`:

```toml
[keys.normal]
"<Space>t" = "toc"
"q" = "quit"
"s" = ""            # unbind
```

## Scripting

`init.lua` gets a `folio` table (page numbers are 1-based):

```lua
folio.command("wordcount", function(args, count)
  local n = 0
  for _ in (folio.page_text() or ""):gmatch("%S+") do n = n + 1 end
  folio.notify(n .. " words")
end, "Count words on this page")

folio.map("gd", function()
  folio.spawn({ "dict", folio.selection() }, function(out) folio.show("define", out) end)
end, { mode = "visual" })

folio.on("highlight", function(text) folio.append("• " .. text .. "\n") end)
```

| function | |
|---|---|
| `folio.command(name, fn, desc)` | add a command (shows up in the palette) |
| `folio.map(keys, cmd_or_fn, {mode, desc})` | bind keys |
| `folio.exec("cmd args")` | run any command |
| `folio.on(event, fn)` | `open`, `page`, `selection`, `search`, `highlight`, `quit` |
| `folio.state()` | path, title, page, page_count, zoom, mode, selection, search… |
| `folio.selection()` `folio.page_text(n)` `folio.outline()` `folio.highlights()` | read the document |
| `folio.show(title, text)` `folio.append(text)` | the side text panel |
| `folio.prompt(label, fn)` `folio.pick(title, items, fn)` | ask the user |
| `folio.spawn(argv, fn(stdout, stderr, code), {stdin})` | run a process without blocking the UI |
| `folio.notify(msg, level)` `folio.clipboard(text)` `folio.open(path)` `folio.set(option, value)` | misc |

`folio --print-init` prints a fuller example.

## Architecture

```
src/
  doc/          Backend trait + poppler implementation (text layout, links, outline via FFI)
  render.rs     render worker pool: each thread owns a document; UI publishes the wanted tile set
  text.rs       text worker: streaming full-document search, reference resolution
  refs.rs       smart-reference detection and definition heuristics
  view.rs       continuous layout + camera (pure, unit tested)
  keymap.rs     modal key engine: sequences, counts, {char} args (unit tested)
  commands.rs   the command registry — keys, palette, Lua and remote all go through it
  script.rs     Lua API (mlua, vendored Lua 5.4); scripts queue effects, never touch state directly
  session.rs    one open document: view, caches, search, selection, hints, peek
  app.rs        command execution, pickers, prompts
  ui/           egui frame loop, canvas, picker, styling
  store.rs      per-document JSON state (~/.local/share/folio)
  ipc.rs        unix-socket remote control
```

Everything the user can do is a named command. That is the extension point for the planned AI features:
an assistant can read the document through the same APIs Lua has (`page_text`, `selection`, `outline`, `highlights`),
act through commands, and render answers in the text panel.

## Debugging

```sh
folio --debug outline paper.pdf
folio --debug text paper.pdf 12        # page text with per-line boxes (0-based page)
folio --debug refs paper.pdf 12        # detected references
folio --debug findref paper.pdf 12     # resolve them
folio --debug search paper.pdf "black hole"
folio --remote 'screenshot /tmp/folio.png'
```
