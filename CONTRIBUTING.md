# Contributing to folio

Thanks for helping. Bug reports from real use (especially on Windows and Linux distributions other than Arch) are
the most valuable thing right now.

## Reporting a bug

Open an issue with the bug template. Include your platform, the folio version (`folio --version`), what you did,
what you expected and what happened. If it's about one PDF and you can share it (or a page of it), attach it.
Crashes: on macOS the report is in `~/Library/Logs/DiagnosticReports/folio-*.ips`; running `folio` from a terminal
shows errors on stderr on every platform.

## Building and testing

See [docs/BUILDING.md](docs/BUILDING.md) for your platform. Then:

```sh
cargo test                  # unit + CLI tests on the fixture PDFs, about a second after the first build
cargo clippy --all-targets  # should be warning-free
cargo fmt                   # rustfmt.toml: max_width 130
```

[docs/TESTING.md](docs/TESTING.md) explains the fixtures and how to add a test for a bug, and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) how the code is organized. Work happens on the `dev` branch; send pull
requests against `dev`.

## Conventions

- Everything is a named command (`commands.rs` + `App::run_builtin`); a new feature is a command first, then a key.
- Match the surrounding style: terse doc comments, no comment noise.
- When behavior changes, update `CHANGELOG.md` (`[Unreleased]`), `TODO.md` and the relevant `docs/*.md`.
  After changing commands, `[general]` settings or the Lua API, run `folio --update-docs` to refresh the generated
  tables in `docs/` (a test fails until you do). The documentation site is built with `mdbook build`.

## License

folio is licensed under the GNU AGPL v3.0 or later. By contributing you agree that your contributions are licensed
the same way.
