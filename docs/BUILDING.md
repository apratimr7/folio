# Building

folio is a single, self-contained Rust binary. MuPDF (PDF rendering) and Lua 5.4 are compiled in from source,
so nothing needs to be installed at runtime. Building MuPDF needs a C compiler, `make` and libclang (for the
generated bindings). The first build takes a few minutes; later builds reuse it.

You need Rust ≥ 1.95: `curl https://sh.rustup.rs -sSf | sh`.

## Linux

Install a C toolchain, libclang and the windowing headers:

| distro | packages |
|---|---|
| Arch | `pacman -S base-devel clang libxkbcommon` |
| Debian/Ubuntu | `apt install build-essential clang libclang-dev libxkbcommon-dev libgl1-mesa-dev` |
| Fedora | `dnf install gcc make clang-devel libxkbcommon-devel mesa-libGL-devel` |

```sh
cargo build --release
install -Dm755 target/release/folio ~/.local/bin/folio
```

It runs natively on Wayland and X11.

## macOS

On the Mac (Apple Silicon or Intel), with the Xcode command-line tools (`xcode-select --install`):

```sh
./scripts/build-macos.sh
open target/release/folio.app --args ~/papers/paper.pdf
```

The script builds a release binary, wraps it in `target/release/folio.app` (ad-hoc signed), and zips it. The app
only depends on macOS itself, so the zip runs on any Mac of the same architecture.

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
