# Scripting with Lua

folio embeds Lua 5.4. `~/.config/folio/init.lua` runs at startup and on `:reload_config`. `require` finds modules in
`~/.config/folio/` and `~/.config/folio/lua/`. `folio --print-init` prints a commented example.

Everything goes through the global `folio` table. **Page numbers are 1-based** in Lua.

## How scripts interact with the app

Scripts never mutate the app while it's in the middle of something. Read functions (`state`, `selection`,
`page_text`…) return a consistent snapshot. Actions (`exec`, `notify`, `show`…) are queued and applied right
after your function returns, in order. In practice: write code top to bottom as usual, but `folio.state()`
called right after `folio.exec("next_page")` still shows the old page.

Errors in scripts show up as red notifications (the message only; the full traceback goes to stderr) and never
crash the reader. A command that keeps calling itself through `folio.exec` is stopped after a few levels with a
"command loop detected" error. `print(...)` shows a notification and also writes to stderr.

`:reload_config` starts a fresh Lua state: `init.lua` runs again, script globals reset, and pending `spawn`
callbacks from the old state are dropped.

## API

### Commands and keys

```lua
folio.command(name, function(args, count) ... end, description)
```
Registers a command. It appears in the command palette, can be bound in `config.toml`, run with `folio.exec`
or `folio --remote`. `args` is the text after the command name and `count` is the numeric prefix (or `nil`).

```lua
folio.map(keys, command_or_function, { mode = "normal", desc = "..." })
```
Binds keys (vim notation, see [CONFIGURATION.md](CONFIGURATION.md)). `command_or_function` is a command line
string (`"zoom 150"`) or a Lua function. `nil` unbinds.

```lua
folio.exec("goto_page 12")
```
Runs any command line (built-in or Lua). See [COMMANDS.md](COMMANDS.md).

In short:

<!-- generated: lua: Commands and keys -->
| | |
|---|---|
| `folio.command(name, function(args, count) end, description)` | define a command (palette, keys, `exec`, `--remote`) |
| `folio.map(keys, command_or_function, { mode = "normal", desc = "…" })` | bind keys (vim notation); `nil` unbinds |
| `folio.exec(command_line)` | run any command line, built-in or Lua |
| `folio.on(event, function(arg) end)` | run a function on an event (see Events) |
<!-- end generated -->

### Events

```lua
folio.on(event, function(arg) ... end)
```

<!-- generated: events -->
| event | argument |
|---|---|
| `open` | path of the opened document (canonical: symlinks resolved) |
| `page` | new current page (string, 1-based). The first one fires after the saved position is restored. |
| `selection` | selected text, after a mouse drag or **each** visual-mode change |
| `selection_done` | selected text, once the selection has stayed the same for 0.4 s (use this for slow work, e.g. asking a model) |
| `search` | the query, when a search is confirmed in the prompt or started with `search …` from Lua/remote |
| `highlight` | text of a new highlight |
| `quit` | — |
<!-- end generated -->

### Reading the document

<!-- generated: lua: Reading the document -->
| | |
|---|---|
| `folio.state()` | `{ path, title, page, page_count, zoom, mode, selection, search, page_mode, theme, tab, tab_count }` (`page` and `tab` are 1-based) |
| `folio.selection()` | selected text or `nil` |
| `folio.page_text(n?)` | text of page `n` (default: current), words re-joined across hyphenated line breaks; `nil` outside `1..page_count` |
| `folio.outline()` | `{ { title, depth, page }, ... }` |
| `folio.highlights()` | `{ { page, text, color, note }, ... }` |
<!-- end generated -->

`state().mode` follows the status bar: `normal`, `visual`, `hint`, `peek`, `ruler`, `input` (a prompt is open) or
`picker` (a picker is open). Key maps exist only for `normal` and `visual`.

### Output and input

<!-- generated: lua: Output and input -->
| | |
|---|---|
| `folio.notify(msg, level?)` | toast; `level` is `"info"`, `"warn"` or `"error"` |
| `folio.show(title, text)` | open the side text panel with this content |
| `folio.append(text)` | append to the text panel (streaming output) |
| `folio.prompt(label, function(input) end, initial?)` | single-line input at the bottom; `input` is `nil` if the user pressed Esc |
| `folio.pick(title, items, function(index, item) end)` | fuzzy picker over a list of strings; `nil, nil` on Esc |
| `folio.clipboard(text)` | copy to the clipboard |
<!-- end generated -->

### System

<!-- generated: lua: System -->
| | |
|---|---|
| `folio.spawn(argv, function(stdout, stderr, code) end, { stdin = "..." })` | run a process **without blocking**; the callback runs on the UI thread when it exits |
| `folio.open(path [, password])` | open another document (a protected one without a password asks for it) |
| `folio.set(option, value)` | `statusbar`, `sidebar`, `smooth_scroll`, `scroll_step`, `zoom_step`, `page_gap`, `hint_chars`, `ruler_dim`, `hint_dim`, `theme`, `page_mode`, `theme.<color>` |
| `folio.config_dir` | path of the config directory |
| `folio.version` | folio version string |
<!-- end generated -->

The standard Lua libraries (`io`, `os`, `string`, `table`…) are available.

## Examples

Define the selected word:

```lua
folio.command("define", function()
  local word = (folio.selection() or ""):match("[%w%-]+")
  if not word then return folio.notify("select a word first", "warn") end
  folio.spawn({ "dict", word }, function(out, err, code)
    if code == 0 then folio.show("define: " .. word, out) else folio.notify(err, "error") end
  end)
end, "Define the selected word")
folio.map("gd", "define", { mode = "visual" })
```

Keep a reading log of highlights:

```lua
folio.on("highlight", function(text)
  local f = io.open(os.getenv("HOME") .. "/reading-log.md", "a")
  f:write(("- %s, p.%d: %s\n"):format(folio.state().title, folio.state().page, text))
  f:close()
end)
```

Jump to a section by number:

```lua
folio.command("sec", function(args)
  for _, item in ipairs(folio.outline()) do
    if item.title:find("^" .. args:gsub("%.", "%%.") .. "%s") and item.page then
      return folio.exec("goto_page " .. item.page)
    end
  end
  folio.notify("no section " .. args, "warn")
end, "Go to section N")
```

Ask an external tool about the selection and stream its answer into the panel. This is the pattern the AI
features will build on:

```lua
folio.command("ask", function()
  local context = folio.selection() or folio.page_text()
  folio.prompt("ask", function(question)
    folio.show("answer", "…")
    folio.spawn({ "my-llm-cli", question }, function(out) folio.show("answer", out) end, { stdin = context })
  end)
end, "Ask about the selection")
```
