# Using folio

```sh
folio paper.pdf          # open a document
folio                    # no file: shows the recent-documents picker
```

folio is modal, like vim. The status bar shows the current mode on the left:
**NORMAL**, **VISUAL** (selecting text), **HINT** (choosing a label), **PEEK** (preview open),
**RULER** (reading ruler active) or **INPUT** (typing into a prompt).

Most commands take a **count**: `5j` scrolls five steps, `3J` moves three pages, `12gg` goes to page 12.

## Moving around

| keys | |
|---|---|
| `j` `k` / arrows | scroll a step (`scroll_step` in config) |
| `h` `l` | scroll sideways when zoomed in |
| `Ctrl-d` `Ctrl-u` | half a screen |
| `Space` `Ctrl-f` `PageDown` / `Shift-Space` `Ctrl-b` `PageUp` | a screen |
| `J` `K` | next / previous page (the first `K` goes to the top of the current page) |
| `gg` `G` | first / last page; with a count, that page |
| `gp` or `:42` | go to a page; page labels such as `xii` work too |
| `t` | fuzzy-search the table of contents; it opens on the current section |
| `x` | toggle the outline sidebar (click to navigate) |

## Zoom

`+`/`=` and `-` zoom around the center of the screen. `w` fits the page width (the default, and it follows window
resizes), `W` fits the whole page, and `z0` is 100%. `Ctrl`+scroll or a pinch zooms around the pointer.
`:zoom 150` sets an exact level.

## The jump list and marks

Big moves (go to page, TOC, links, references, marks, the first search jump) are recorded.
`Ctrl-o` (or `Backspace`) goes back, `Ctrl-i` (or `Tab`) goes forward. Use this after following a reference.

`m` then a letter sets a mark, and `'` or `` ` `` then the letter jumps to it. Lowercase marks belong to the document.
**Uppercase marks are global**: `'A` reopens the document the mark is in. `''` jumps back. `gm` lists marks;
`Ctrl-d` there deletes the selected one. `:delete_mark a` deletes one mark, `:delete_marks` all of this document's
lowercase marks.

## Search

`/` searches forward and `?` backward. Matches are found **as you type**, and the view jumps to the first match
after where you started. `Enter` keeps the search, `Esc` cancels and returns you to the starting point,
and `↑`/`↓` recall previous searches. `n`/`N` go to the next/previous match. The status bar shows `“query” 3/27`.

Search is case-insensitive unless the query has an uppercase letter (smart case). It ignores ligatures (ﬁ, ﬂ),
typographic quotes and dashes, line breaks, and words hyphenated across lines.

`*` searches for the current selection. `Esc` clears search highlights.

## Links and smart references

| keys | |
|---|---|
| `f` | label every link on screen; type a label to follow it |
| `F` | same, but **peek** at the target instead of jumping |
| `s` | label every *reference* in the text on screen and jump to its definition |
| `S` | same, but peek |

Typing a label in **uppercase** peeks instead of jumping, whichever key started the hints.

*Smart references* work from the text itself, so they also work in PDFs without hyperlinks. They recognize:

- citations `[12]`, `[3, 4]`, `[5–7]`. The target is found in the next bibliography after the citation, so
  per-chapter reference lists work. Both `[12] Author…` and `12. Author…` styles are supported.
- figures `Fig. 3`, `Figure 2.1b`, and tables `Table 4`, found by their captions
- equations `Eq. (2.4)`, `eqs. 3.1`, and bare `(2.8)`, found by the equation number at the end of a line
- `Theorem`, `Lemma`, `Proposition`, `Corollary`, `Definition`, `Remark`, `Example`, `Conjecture`,
  `Assumption`, `Algorithm`, `Claim` + number, defined as `Theorem 2.36.` or number-first `2.36 Theorem.`
- `Section 3.2`, `Sec. 4`, `§2`, `Appendix A`, `Chapter 5`, resolved through the outline first

**Peek** opens a floating preview of the target with the target outlined. `j`/`k` scroll inside it, `J`/`K` change
page, `+`/`-` zoom, `Enter` jumps there (and records it in the jump list), and `Esc` closes it.
Right-clicking a link peeks it too.

Web links (`http`, `https`) and `mailto:` open in the default handler. Other link targets in a PDF (`file:`, custom
schemes, paths) aren't opened, since they could launch files or programs; folio shows the target and copies it.

## Selecting, copying and highlighting

`v` starts **visual mode** in two quick steps, so you can begin anywhere on screen without the mouse:

1. Every text row on screen gets a label in the left margin, like line numbers. Type a row's label.
2. The other rows dim and each word of that row gets a short label. Type one: the selection starts on that word.

`v` again (instead of a label) starts at the first word on screen, and `Esc` cancels. If a search match is on screen
or the reading ruler is on, `v` starts there right away, without labels. Then:

| keys | |
|---|---|
| `w`/`e`, `b` | extend by a word forward / back |
| `l`, `h` | by a character |
| `j`, `k` | to the line below / above |
| `$`, `0` | end / start of the line |
| `)` | to the end of the sentence |
| `o` | swap the ends |
| `y` / `Ctrl-c` | copy and leave visual mode |
| `H` | highlight (`:highlight green` picks a color) |
| `a` | highlight with a note |
| `*` | search for the selection |
| `Esc` / `v` | cancel |

Navigation keys from normal mode (`Ctrl-d`, `J`, …) still work in visual mode.
With the mouse: drag to select, double-click to select a word, click to clear.

`Y` copies the whole current page. `gh` lists highlights (`Enter` jumps, `Ctrl-d` deletes), and `ge`
exports all highlights, notes and bookmarks as Markdown to the clipboard and the text panel.
Highlights and notes are saved per document.

## Bookmarks

`b` bookmarks the current position. It suggests the current section title as the label. `B` lists bookmarks
(`Ctrl-d` deletes).

## Reading ruler

`r` toggles a band that follows a text line and dims what you've already read. While it's on, `j`/`k` move it
line by line, across pages, and the view scrolls to keep it in a comfortable spot. `zz` centers it.

## Colors and themes

| keys | |
|---|---|
| `i` | dark pages: lightness inverted, **hue preserved**, so plots and colored links stay recognizable |
| `I` | tinted pages: black→`page_fg`, white→`page_bg` from the theme (sepia, low-contrast…) |
| `T` | next UI theme (`midnight`, `paper`, `nord`, `gruvbox`); `:theme` opens a picker |

## Files

`o` opens a file browser that starts in the current document's directory. Type to filter, `Tab` completes,
`Enter` on a directory descends into it, and you can type or paste a full path. `O` lists recent documents.
Position, zoom, marks, highlights and bookmarks are restored when you reopen a file.

**Auto-reload:** when the open file changes on disk (for example `latexmk -pvc` or `typst watch`), folio reloads it
and keeps your position. `R` forces a reload.

## Pickers

The command palette, TOC, files, recent documents, highlights, bookmarks, marks and the help screen (`F1`)
all share one fuzzy picker. `↑`/`↓`, `Ctrl-n`/`Ctrl-p` or `Ctrl-j`/`Ctrl-k` move, `Ctrl-f`/`Ctrl-b` page,
`Enter` picks, `Tab` completes (a path, or a command name so you can type arguments), and `Esc` closes.

In the **command palette**, `:zoom 200`, `:goto_page 12` or plain `:12` run directly.

## Panels

| keys | |
|---|---|
| `x` | outline sidebar |
| `Ctrl-n` | text panel (used by scripts, highlight export, and future AI answers) |
| `zs` | status bar |
| `F11` | fullscreen |

## Remote control

```sh
folio --remote 'goto_page 12'
folio --remote 'search Hawking radiation' 'center_line'
folio --remote 'toggle_dark; zoom 140'
```

This talks to the most recently started instance through `folio.sock` in `$XDG_RUNTIME_DIR`, or in `$TMPDIR`
when `XDG_RUNTIME_DIR` isn't set (the usual case on macOS). If that instance is gone, the newest running one is
used. Set `FOLIO_SOCKET` to target a specific one, e.g. `FOLIO_SOCKET=$TMPDIR/folio-<pid>.sock`.
Sockets are private to your user (mode 0600). A socket path can be at most 104 bytes on macOS (108 on Linux); if
it's longer, remote control is disabled and folio shows a warning.
