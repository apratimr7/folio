# Building

folio is a single Rust binary that links poppler-glib (PDF rendering) and cairo. Lua 5.4 is compiled in, so it
doesn't need to be installed.

You need Rust ≥ 1.88: `curl https://sh.rustup.rs -sSf | sh`.

## Linux

Install the poppler-glib development files:

| distro | packages |
|---|---|
| Arch | `pacman -S poppler-glib cairo pkgconf` |
| Debian/Ubuntu | `apt install libpoppler-glib-dev libcairo2-dev libxkbcommon-dev libgl1-mesa-dev pkg-config` |
| Fedora | `dnf install poppler-glib-devel cairo-devel libxkbcommon-devel mesa-libGL-devel` |

```sh
cargo build --release
install -Dm755 target/release/folio ~/.local/bin/folio
```

It runs natively on Wayland and X11.

## macOS

On the Mac (Apple Silicon or Intel), with the Xcode command-line tools (`xcode-select --install`) and Homebrew:

```sh
./scripts/build-macos.sh
open target/release/folio.app --args ~/papers/paper.pdf
```

The script installs `poppler` and `pkg-config` with Homebrew if needed, builds a release binary, wraps it in
`target/release/folio.app` (ad-hoc signed), and zips it.

- `BUNDLE=1 ./scripts/build-macos.sh` also copies the Homebrew dylibs into the app (via `dylibbundler`), so the
  `.app` runs on Macs without Homebrew's poppler.
- For the command line: `ln -sf "$PWD/target/release/folio.app/Contents/MacOS/folio" /usr/local/bin/folio`.
- Config and data use `~/.config/folio` and `~/.local/share/folio`, the same as on Linux.
- On macOS **Cmd works like Ctrl** in key bindings (`Cmd-d` = `<C-d>`, `Cmd-q` quits).
- An app that isn't notarized is blocked the first time. Right-click → *Open*, or
  `xattr -dr com.apple.quarantine folio.app`.

**Known limitation:** opening a PDF from Finder (double-click / *Open With*) starts folio but doesn't load the file
yet, because Apple Events aren't handled. Use `open -a folio --args file.pdf` or the CLI for now
(see [TODO.md](../TODO.md)).

## CI

`.github/workflows/build.yml` builds and tests on Ubuntu and on macOS (arm64 and x86_64). It uploads the Linux
binary and the zipped `.app` bundles as artifacts. Push to GitHub, or run the workflow manually, to get macOS
builds without a Mac.
