# Configuration

folio reads two optional files from `~/.config/folio/` on Linux and macOS, `%APPDATA%\folio\` on Windows, or
`$XDG_CONFIG_HOME/folio/` when that's set (on every platform):

| file | |
|---|---|
| `config.toml` | settings, theme colors, key bindings |
| `init.lua` | scripts (see [SCRIPTING.md](SCRIPTING.md)) |

`folio --init-config` writes commented versions of both (it never overwrites). `folio --print-config` prints the
defaults. `:reload_config` applies changes without restarting.

Mistakes show a warning when the config loads: a type error (the previous config is kept), an unknown setting
(e.g. a typo like `hint_char`), a malformed key (`"<C-"`, `"<Foo>"`), a binding to a command that doesn't exist
(checked after `init.lua`, so Lua commands count), or a `[keys.<mode>]` table for a mode other than `normal` and
`visual`.

Per-document state lives in `~/.local/share/folio/` (`%LOCALAPPDATA%\folio\` on Windows, or `$XDG_DATA_HOME/folio/`):
one JSON file per document under `docs/`, plus `global.json` for recent files and global marks, and `window.ron`
for the window size and position. The files are plain text, so they're easy to back up or process.

## `[general]`

| key | default | |
|---|---|---|
| `theme` | `"midnight"` | `midnight`, `paper`, `nord`, `gruvbox` |
| `page_mode` | `"normal"` | `normal`, `dark` (hue-preserving invert), `custom` (theme tint) |
| `statusbar` | `true` | |
| `smooth_scroll` | `true` | animate keyboard scrolling |
| `scroll_step` | `70` | `j`/`k` distance in screen points |
| `zoom_step` | `1.15` | factor per `+`/`-` |
| `page_gap` | `14` | space between pages |
| `auto_reload` | `true` | reload when the file changes |
| `restore_position` | `true` | reopen documents where you left off |
| `render_threads` | `2` | background render workers |
| `render_oversample` | `1.0` | render above screen density (sharper, more memory) |
| `hint_chars` | `"asdfghjklqwertyuiop"` | alphabet for hint labels (any typeable characters, e.g. `"jkl;"`) |
| `ruler_dim` | by theme | how dark the reading ruler makes the text above it: `0` (off) to `1` (black); 0.16 on dark themes, 0.07 on light |
| `hint_dim` | by theme | how dark the page gets around the row whose words `v` labels: `0` to `1`; 0.35 dark, 0.24 light |
| `key_timeout_ms` | `600` | wait for a longer sequence (`g` vs `gg`) before running the shorter one |
| `ui_font_size` | `14` | |
| `ui_font` | auto | path to a TTF/OTF. Default: Inter, Noto Sans, Cantarell (via fontconfig), or the system font on macOS |

## `[theme]`

Overrides on top of the preset. Colors are `#rrggbb` or `#rrggbbaa`.

```toml
[theme]
accent = "#e78a4e"
bg = "#101014"            # around the pages
surface = "#18181d"       # panels, palette
surface_alt = "#222228"   # inputs, toasts
border = "#2c2c33"
text = "#e6e6eb"
text_dim = "#8a8a96"
accent_text = "#101014"   # text drawn on accent (mode pill)
search_match = "#ffd04d58"
search_current = "#ff8a3d90"
selection = "#7aa2ff50"
ruler = "#7aa2ff2a"
hint_bg = "#ffd04d"
hint_text = "#101014"
page_bg = "#1b1e27"       # paper color for dark/custom page modes
page_fg = "#d5d9e3"       # ink color for dark/custom page modes
page_shadow = "#00000070"
radius = 10               # corner radius of panels

[theme.highlight]         # named highlight colors; the first is the default for `H`
yellow = "#ffd04d68"
red = "#ff5c5c60"
```

Lua can change these at runtime with `folio.set("theme.accent", "#ff8800")`.

## `[keys.<mode>]`

Modes are `normal` and `visual`. A visual-mode key that isn't bound falls back to the normal-mode binding.

```toml
[keys.normal]
"<Space>t" = "toc"
"gz" = "zoom 150"
"q" = "quit"
"<C-s>" = "smart_peek"
"s" = ""                   # an empty string unbinds a default

[keys.visual]
"<C-h>" = "highlight pink"
"gd" = "define"            # a Lua command from init.lua
```

Key notation (vim style):

| | |
|---|---|
| `j`, `J`, `+`, `/` | a typed character (case matters) |
| `gg`, `ZZ`, `<Space>t` | sequences |
| `<C-d>` `<A-x>` `<S-Tab>` | Ctrl, Alt, Shift + key (on macOS, Cmd also counts as Ctrl) |
| `<Esc> <Enter> <Tab> <Backspace> <Delete> <Space> <Up> <Down> <Left> <Right> <PageUp> <PageDown> <Home> <End> <F1>…<F12>` | named keys |
| `<lt>` | a literal `<` |

`{char}` in a command takes the next typed key as its argument, e.g. `"M" = "set_mark {char}"`.

A leading digit starts a count unless that digit alone is bound as a key. `0` never starts a count; once a count has
started, every digit extends it, so `10j` works in every mode.

All commands are listed in [COMMANDS.md](COMMANDS.md).
