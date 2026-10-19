-- folio init.lua — ~/.config/folio/init.lua
--
-- The `folio` table is the scripting API. Page numbers are 1-based.
--
--   folio.map(keys, command_or_function, { mode = "normal"|"visual", desc = "..." })
--   folio.command(name, function(args, count) end, description)
--   folio.exec("goto_page 12")          run any command line
--   folio.on(event, function(arg) end)  events: open, page, selection, search, highlight, quit
--   folio.state()                       { path, title, page, page_count, zoom, mode, selection, search, ... }
--   folio.selection()                   selected text or nil
--   folio.page_text(n?)                 text of page n (default: current)
--   folio.outline()  folio.highlights()
--   folio.notify(msg, "info"|"warn"|"error")
--   folio.show(title, text)  folio.append(text)   the side text panel (Ctrl-N toggles)
--   folio.prompt(label, function(input) end, initial?)
--   folio.pick(title, { "a", "b" }, function(index) end)
--   folio.spawn({ "cmd", "arg" }, function(stdout, stderr, code) end, { stdin = "..." })  -- async
--   folio.clipboard(text)  folio.open(path)  folio.set(option, value)
--
-- Anything you define with folio.command shows up in the command palette (:).

-- Word count of the current page.
folio.command("wordcount", function()
  local text = folio.page_text() or ""
  local n = 0
  for _ in text:gmatch("%S+") do n = n + 1 end
  folio.notify(("page %d: %d words"):format(folio.state().page, n))
end, "Count words on the current page")

-- Look up the selected word in a dictionary (needs `dict` or `sdcv` installed).
folio.command("define", function()
  local word = folio.selection()
  if not word then
    return folio.notify("select a word first (v)", "warn")
  end
  word = word:match("[%w%-]+") or word
  folio.spawn({ "dict", word }, function(out, err, code)
    if code ~= 0 then
      folio.notify("dict failed: " .. (err ~= "" and err or "not installed?"), "error")
    else
      folio.show("define: " .. word, out)
    end
  end)
end, "Define the selected word with dict(1)")
folio.map("gd", "define", { mode = "visual" })

-- Search the selection on arXiv / Google Scholar.
local function open_url(url)
  folio.spawn({ "xdg-open", url }, function() end)
end
local function urlencode(s)
  return (s:gsub("[^%w%-_%.~]", function(c) return ("%%%02X"):format(c:byte()) end))
end
folio.command("scholar", function()
  local q = folio.selection() or folio.state().title
  open_url("https://scholar.google.com/scholar?q=" .. urlencode(q))
end, "Search the selection (or title) on Google Scholar")

-- Copy a citation-style reference to the current position.
folio.command("cite_here", function()
  local s = folio.state()
  folio.clipboard(("%s, p. %d"):format(s.title, s.page))
  folio.notify("copied reference to clipboard")
end, "Copy 'title, p. N' to the clipboard")

-- A tiny reading log in the text panel.
folio.on("highlight", function(text)
  folio.append(("• p.%d  %s\n"):format(folio.state().page, text))
end)

-- Example of a mapping to a Lua function.
folio.map("<Space>w", function() folio.exec("wordcount") end, { desc = "word count" })
