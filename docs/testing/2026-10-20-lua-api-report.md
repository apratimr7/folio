# Lua API, automation and customization test report — 2026-10-20

> **Status (same day):** all 7 bugs are fixed, along with the startup-snapshot, `search`-hook, `page`-hook,
> `--remote ;`, `prompt` and `pick` gaps. Each fix was re-verified with the same harness. Still open: the
> `selection`-hook debounce and a manual clipboard check. See `CHANGELOG.md` [Unreleased].

Build: folio 0.1.0, release, commit `2e2caa8` (Linux, Hyprland). Method: a test `init.lua` and `config.toml` in
throwaway config/data directories. The app ran on a hidden virtual monitor, driven by `folio --remote` and by
real key events sent through the compositor. Checks wrote PASS/FAIL lines to a log, and UI results were verified
with in-app screenshots.

**Summary:** every API function, all six hooks, the `config.toml` options and the four automations were exercised.
Almost everything works. There are **7 bugs** and **9 design gaps**, and **1 thing couldn't be tested** in this
setup (the clipboard).
Two bugs are serious: `J` (`next_page`) stops after one page, and a self-calling Lua command crashes folio.

## Bugs (fix these)

| # | severity | bug | cause / fix |
|---|---|---|---|
| 1 | **high** | **`J` / `next_page` only works once.** The second and later presses stay on the same page, with or without smooth scroll. `K` works. | `next_page` scrolls to half a page-gap *above* the target page. The next call reads the current page from `pos_at(0.0)`, which is still in the gap, so it gets the previous page and targets the same page again. Compute the current page from a point just below the viewport top (or from `target_y + gap`). |
| 2 | **high** | **A Lua command that calls `folio.exec` on itself crashes folio** (stack overflow abort). Up to ~8 s of unsaved position/state is lost, and the socket file is left behind. | `process_effects → apply_effect(Exec) → run_line → with_script → process_effects` recurses. Add a recursion-depth guard (or queue nested effects instead of recursing) and report "command loop detected" as an error toast. |
| 3 | medium | **`<C-x>`, `<C-c>`, `<C-v>` can never fire as key bindings.** This includes the documented visual-mode `Ctrl-c` = copy (`y` still works). | egui-winit turns Ctrl/Cmd+X/C/V into `Event::Cut/Copy/Paste` instead of key events. Map those three events to Ctrl chords in `ui/frame.rs::chords()` when no text field has focus. |
| 4 | medium | **The `quit` hook doesn't run when the window is closed** (WM close, title-bar ×, or Cmd-Q on macOS). It only runs for the `quit` command. | `close_requested` calls `save_all()` but not `emit("quit")`. Emit it there. |
| 5 | low | **`folio.page_text(0)` returns page 1's text** instead of `nil`. | `saturating_sub(1)` maps 0 to 0. Treat `page < 1` as out of range. |
| 6 | low | **Non-alphanumeric `hint_chars` (e.g. `"jkl;"`) produce untypeable labels.** Typing `;` cancels the hints. | `hint_key` only accepts alphanumeric characters. Accept any character that's in `hint_chars`. |
| 7 | low | **Toasts:** only the 5 newest are kept (older ones vanish without being seen), and long messages, like Lua errors with a path, run off the bottom of the window and push other toasts off-screen. | Raise the cap or collapse duplicates. Wrap and limit toast height (show the first lines, full text in the text panel). |

## Design gaps found while testing

- **`folio.state()` during startup** returns empty `mode` and `theme` (the snapshot isn't filled until the first command). Fill it before running `init.lua`.
- **The `search` hook** fires only for searches typed into the `/` prompt, not for `search …` run via `exec`/`--remote`.
- **The `selection` hook fires on every extension** (3 times for `v w w`). That's fine for logging, but heavy work (AI, network) needs a debounce. Consider a `selection_done` event, or debouncing to ~300 ms.
- **The `page` hook fires for page 1 at startup**, just before the saved position is restored.
- **`--remote` splits on every `;`**, including inside `:lua` code, so remote Lua can't contain semicolons. Only split when the line isn't a `lua` command (or support quoting).
- **The `open` hook gets the canonical path** (symlinks resolved), so pattern-matching on a symlink's name fails. Document it, or pass both paths.
- **Canceling `folio.prompt` with Esc** never calls the callback. Scripts can't tell that the user canceled. Consider calling it with `nil`.
- **`folio.pick`'s callback gets only the index**, not the chosen string.
- **`:reload_config` creates a fresh Lua state**: script globals reset, and pending `spawn` callbacks from the old state are dropped. This is reasonable, but undocumented.

## Couldn't be tested here

- **Clipboard** (`folio.clipboard`, and built-in copy commands). On Wayland only a focused window may set the clipboard, and the hidden test window can't take focus. Nothing reached the clipboard in this setup, even for built-in commands. You confirmed earlier that `y` copies in normal use. Recheck `folio.clipboard` by hand.

## What works

### Startup and environment
- `folio`, `folio.version`, `folio.config_dir`
- `require` finds modules in `~/.config/folio/lua/`
- standard libraries (`io`, `os`, `string`, `table`, `math`)
- an invalid key in `config.toml` shows a clear warning toast at startup; folio keeps running

### Commands and keys
- `folio.command` with `args` and `count`: `t_echo a b c` → args `a b c`
- `folio.map` to a command string with arguments
- `folio.map` to a function, with counts: `3<Space>f` → `count = 3`
- visual-mode maps see `folio.selection()`
- `folio.map(keys, nil)` unbinds
- **a Lua command with a built-in's name replaces it**, including from keys (`Y` ran the Lua `yank_page`)
- `folio.exec` of built-ins and Lua commands; actions are deferred until the function returns, as documented
- `config.toml`:
  - bindings with arguments (`gz` → `zoom 150`)
  - `""` unbinds a default (`T`)
  - theme preset and accent override
  - added highlight color
  - `hint_chars`, `scroll_step`, `smooth_scroll`
- `--remote` runs built-ins, Lua commands, several commands, and `;` chains (responds in ~18 ms even while a `spawn` is running)

### Hooks
- `open` (on launch and on `folio.open`), `page`, `selection`, `highlight`, `search` (typed searches), `quit` (via the command)
- an error inside a hook shows a toast and doesn't stop other hooks
- `:reload_config` re-registers hooks without duplicates

### Reading the document
- `state()`: all ten fields correct after a document opens
- `page_text()` / `page_text(n)` / out of range → `nil`
- `outline()` (175 entries, with title/depth/page)
- `highlights()`, including newly created ones with all fields
- `selection()` and `state().selection` agree in visual mode

### Output and input
- `notify` (info/warn/error), `print`
- `show` + `append` (text panel)
- `prompt` with initial text; typing appends at the cursor; Enter delivers, Esc cancels
- `pick` with fuzzy filtering and arrow keys (correct index)
- `folio.set`: `theme`, `page_mode`, `theme.accent`, `statusbar`, `smooth_scroll`
- unknown options and themes give clear warnings

### Processes
- `folio.spawn`: stdout, stderr, exit code, piped stdin
- **2 MB stdin without deadlock**
- missing binary → `code=-1` with an error message
- empty argv → Lua error
- the UI stays responsive during long jobs

### Robustness
- runtime errors in commands become red toasts with file and line
- invalid `:lua` and unknown commands produce toasts, not crashes
- a page hook that triggers `next_page` doesn't hang (it ran into bug 1 instead)

### Automations from the conversation
- **open hook** (time-of-day logic ran)
- **highlight → Markdown reading log** (wrote `- **title** p.28: text`)
- **`gR` chain** (found References via `outline()`, jumped, set mark `r`, `'r` returned there)
- **sleep-based repeating timer** (ticked at 3 s, 6 s, 9 s, then stopped as programmed)

## Suggested order

1. Bugs 1 and 2: a quick fix each, and they affect everyone.
2. Bugs 3 and 4, plus the startup-snapshot and `search`-hook gaps (all small).
3. The remaining low-severity bugs, with Lua tests for each.
4. Turn this harness into `tests/lua_api.lua`, runnable with a `folio --test-script` mode, so it runs in CI.
