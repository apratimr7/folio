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

## Windows

With [rustup](https://rustup.rs), the Visual Studio 2022 Build Tools (the *Desktop development with C++* workload,
for MSVC and MSBuild) and LLVM (`winget install LLVM.LLVM`, for the MuPDF bindings), in PowerShell:

```powershell
./scripts/build-windows.ps1
target\release\folio.exe paper.pdf
```

The script builds `folio.exe` and zips it with the README and license into
`target/release/folio-<version>-windows-x86_64.zip`. MuPDF is linked statically; the binary uses the standard MSVC
runtime, which Windows 10 and 11 normally have (otherwise install the "Visual C++ Redistributable").

- Config is in `%APPDATA%\folio` and data in `%LOCALAPPDATA%\folio`. `XDG_CONFIG_HOME`/`XDG_DATA_HOME` override them,
  as on the other platforms.
- `folio.exe` is a GUI program (no console window from Explorer). From a terminal, `--help`, `--version` and
  `--debug` still print.
- **Not yet available on Windows:** remote control (`--remote`; needs a named-pipe transport) and clean shutdown on
  console signals. Everything else is the same as on Linux and macOS.
- SmartScreen warns about unsigned downloads: *More info* → *Run anyway*.

## CI

`.github/workflows/build.yml` builds and runs the tests on Ubuntu, macOS (arm64 and x86_64) and Windows (x86_64).
It uploads the Linux binary, the zipped `.app` bundles and the zipped Windows `.exe` as artifacts. Push to GitHub,
or run the workflow manually, to get builds for platforms you don't have. A build cache keeps later runs fast; the
first run on a runner compiles MuPDF and takes 5–25 minutes.
