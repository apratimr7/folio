# FAQ

**Where are my settings and reading positions?** Settings are in `~/.config/folio/` on Linux and macOS and in
`%APPDATA%\folio\` on Windows. Reading positions, highlights, bookmarks and the recent list are in
`~/.local/share/folio/` (Windows: `%LOCALAPPDATA%\folio\`). Setting `XDG_CONFIG_HOME` or `XDG_DATA_HOME` moves them.
See [Configuration](CONFIGURATION.md).

**macOS says folio can't be opened.** The app isn't notarized yet. Right-click it and choose *Open* once, or run
`xattr -dr com.apple.quarantine /Applications/folio.app`.

**Windows SmartScreen warns about an unknown publisher.** The program isn't code-signed yet. Choose *More info*, then
*Run anyway*.

**`oo` does nothing on Linux.** The system file dialog needs the XDG desktop portal (`xdg-desktop-portal` with a
GTK, GNOME or KDE backend), which most desktops have. Without it, use `of` for the keyboard file picker.

**I opened a file from the terminal and no new window appeared.** folio opens it as a tab in the window that's
already running. `folio --new paper.pdf` starts a separate window.

**Do my highlights survive a LaTeX rebuild?** Yes. State is keyed by the file's path, so a rebuilt PDF keeps its
highlights, marks and position, and folio reloads it when it changes. If the text moved a lot, highlights can end up
slightly off (fixing this is planned).

**A smart reference jumps to the wrong place.** Detection is heuristic. Please report it with the PDF (or a similar
public one): the output of `folio --debug findref paper.pdf PAGE` helps.

**How do I change a key?** Add it under `[keys.normal]` in `config.toml`, for example `"q" = "quit"`. An empty
string unbinds a default. `F1` shows every binding.
