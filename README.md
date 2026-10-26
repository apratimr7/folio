# folio

[![build](https://github.com/apratimr7/folio/actions/workflows/build.yml/badge.svg)](https://github.com/apratimr7/folio/actions/workflows/build.yml)
[![release](https://img.shields.io/github/v/release/apratimr7/folio)](https://github.com/apratimr7/folio/releases/latest)
[![license: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)

A keyboard-first PDF reader for technical documents, in the spirit of [sioyek](https://sioyek.info), written in
Rust. For Linux, macOS and Windows.

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
- **Lua scripting** and **remote control** (`folio --remote 'goto_page 12'`, Linux and macOS), the base for upcoming
  AI features.
- **One self-contained binary**: MuPDF and Lua are compiled in, so nothing else needs to be installed.

## Install

Download the archive for your platform from the **[latest release](https://github.com/apratimr7/folio/releases/latest)**.
Each is a single self-contained program; nothing else needs to be installed.

| Platform | Download | Then |
|---|---|---|
| **macOS**, Apple Silicon (M1–M4) | `folio-…-macos-arm64.zip` | Unzip, move `folio.app` to Applications. The first time, right-click it → *Open* (the app isn't notarized yet). |
| **macOS**, Intel | `folio-…-macos-x86_64.zip` | Same as above. |
| **Windows** 10/11 (x86_64) | `folio-…-windows-x86_64.zip` | Unzip anywhere and run `folio.exe`. If SmartScreen says the publisher is unknown: *More info* → *Run anyway*. |
| **Linux** (x86_64, glibc 2.35+) | `folio-…-linux-x86_64.tar.gz` | `tar xzf folio-*.tar.gz`, then `install -Dm755 folio-*/folio ~/.local/bin/folio` (and optionally the `.desktop` file to `~/.local/share/applications/`). Needs the usual desktop libraries (`libxkbcommon`, OpenGL, Wayland or X11). |

Open a PDF with `folio paper.pdf` (on macOS: `open -a folio --args paper.pdf`; opening from Finder by double-click
isn't supported yet), or start `folio` and press `oo`. Checksums are in `SHA256SUMS` on the release page.

## Build from source

```sh
# Linux (needs a C compiler, make and libclang; Debian/Ubuntu: apt install clang libclang-dev libxkbcommon-dev libgl1-mesa-dev)
cargo build --release
./target/release/folio paper.pdf

# macOS (Xcode command line tools)
./scripts/build-macos.sh
open target/release/folio.app --args paper.pdf

# Windows (Visual Studio Build Tools with C++, LLVM, rustup), in PowerShell
./scripts/build-windows.ps1
target\release\folio.exe paper.pdf

folio --init-config      # write config.toml and init.lua to customize (~/.config/folio, or %APPDATA%\folio)
```

Press `F1` in the app for every key binding, or `:` for the command palette.

| | | | |
|---|---|---|---|
| `j` `k` `Ctrl-d` `Ctrl-u` `Space` | scroll | `/` `?` `n` `N` | search |
| `J` `K` `gg` `G` `12gg` `:12` | pages | `t` / `x` | TOC picker / sidebar |
| `+` `-` `w` `W` | zoom, fit | `f` `F` | follow / peek link |
| `Ctrl-o` `Ctrl-i` | jump back/forward | `s` `S` | smart reference jump / peek |
| `m{x}` `'{x}` | marks | `v` → row → word, then `y` `H` `a` | select → copy, highlight, annotate |
| `b` `B` `gh` `ge` | bookmarks, highlights, export | `r` | reading ruler |
| `i` `I` `T` | dark / tinted pages, theme | `oo` `of` `O` | open (system dialog / type a path), recent |

## Documentation

| | |
|---|---|
| [docs/USAGE.md](docs/USAGE.md) | how everything works, mode by mode |
| [docs/COMMANDS.md](docs/COMMANDS.md) | every command with its default keys (generated) |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | `config.toml`: options, theme colors, key bindings |
| [docs/SCRIPTING.md](docs/SCRIPTING.md) | the Lua API, with examples |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | how it's built, for contributors |
| [docs/BUILDING.md](docs/BUILDING.md) | Linux, macOS, Windows and CI builds |
| [docs/TESTING.md](docs/TESTING.md) | the test suite, fixtures, and adding a test for a bug |
| [CHANGELOG.md](CHANGELOG.md) | versions and what changed |
| [TODO.md](TODO.md) | known issues and next tasks |
| [ROADMAP.md](ROADMAP.md) | future plans, including the AI features |
| [CONTRIBUTING.md](CONTRIBUTING.md) | reporting bugs, building, testing, conventions |

## Status

- **Linux** (Arch, Hyprland/Wayland): the original platform, tested by hand before the switch to MuPDF and in CI
  since; worth a manual run.
- **macOS** (Apple Silicon): fully tested by hand (a full test pass, see `docs/testing/`) and in CI. Intel builds in
  CI. Not yet checked: Retina sharpness, opening from Finder (not implemented yet).
- **Windows**: new. Builds and passes the test suite in CI; not yet run by hand. Remote control (`--remote`) isn't
  available on Windows yet. Feedback from testers is very welcome.

Open issues and next steps are in [TODO.md](TODO.md); the long-term plan is in [ROADMAP.md](ROADMAP.md).

## License

folio is free software under the [GNU AGPL v3.0 or later](LICENSE). It renders with [MuPDF](https://mupdf.com)
(AGPL-3.0, Artifex) through the [mupdf-rs](https://github.com/messense/mupdf-rs) bindings.
