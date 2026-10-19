# folio

A keyboard-first PDF reader for technical documents, in the spirit of [sioyek](https://sioyek.info), written in
Rust. **v0.1.0**. Linux and macOS.

- **Smart references.** Press `s` and every `[12]`, `Fig. 3`, `Eq. (2.4)`, `(2.8)`, `Theorem 1.2` or `Sect. 3.1` on
  screen gets a label. Type it to jump to the definition, or type it in uppercase to **peek** at it in a floating
  preview without losing your place. This works even in PDFs without hyperlinks.
- **Modal, vim-style keys**: counts, sequences, marks, a jump list, all rebindable.
- **Link hints** (`f`/`F`): follow or peek at any hyperlink without the mouse.
- **Fast, sharp rendering**: tiled and multi-threaded, with prefetching and no blank pages while zooming.
- **Incremental search** with smart case and ligature/hyphenation normalization.
- **Keyboard selection, highlights with notes, bookmarks**, and Markdown export.
- **Reading ruler**, **hue-preserving dark pages**, four themes, every color configurable.
- **Fuzzy pickers** for commands, TOC, files, recent documents, highlights, marks and key help.
- **Auto-reload** for live LaTeX/Typst builds.
- **Lua scripting** and **remote control** (`folio --remote 'goto_page 12'`), the base for upcoming AI features.

## Quick start

```sh
# Linux (Arch: pacman -S poppler-glib; Debian/Ubuntu: apt install libpoppler-glib-dev libcairo2-dev)
cargo build --release
./target/release/folio paper.pdf

# macOS (Homebrew)
./scripts/build-macos.sh
open target/release/folio.app --args paper.pdf

folio --init-config      # write ~/.config/folio/config.toml and init.lua to customize
```

Press `F1` in the app for every key binding, or `:` for the command palette.

| | | | |
|---|---|---|---|
| `j` `k` `Ctrl-d` `Ctrl-u` `Space` | scroll | `/` `?` `n` `N` | search |
| `J` `K` `gg` `G` `12gg` `:12` | pages | `t` / `x` | TOC picker / sidebar |
| `+` `-` `w` `W` | zoom, fit | `f` `F` | follow / peek link |
| `Ctrl-o` `Ctrl-i` | jump back/forward | `s` `S` | smart reference jump / peek |
| `m{x}` `'{x}` | marks | `v` → `y` `H` `a` | select → copy, highlight, annotate |
| `b` `B` `gh` `ge` | bookmarks, highlights, export | `r` | reading ruler |
| `i` `I` `T` | dark / tinted pages, theme | `o` `O` | open, recent |

## Documentation

| | |
|---|---|
| [docs/USAGE.md](docs/USAGE.md) | how everything works, mode by mode |
| [docs/COMMANDS.md](docs/COMMANDS.md) | every command with its default keys (generated) |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | `config.toml`: options, theme colors, key bindings |
| [docs/SCRIPTING.md](docs/SCRIPTING.md) | the Lua API, with examples |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | how it's built, for contributors |
| [docs/BUILDING.md](docs/BUILDING.md) | Linux, macOS and CI builds |
| [CHANGELOG.md](CHANGELOG.md) | versions and what changed |
| [TODO.md](TODO.md) | known issues and next tasks |
| [ROADMAP.md](ROADMAP.md) | future plans, including the AI features |

## Status

0.1.0 is the first usable version, tested on Linux (Hyprland/Wayland). The macOS build type-checks and has a build
script and CI job, but hasn't been run on a Mac yet; see [TODO.md](TODO.md).
