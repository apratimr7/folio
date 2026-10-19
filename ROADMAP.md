# Roadmap

Where folio is headed. Concrete near-term tasks live in [TODO.md](TODO.md). Shipped work is in
[CHANGELOG.md](CHANGELOG.md).

## 0.2: research workflow

- SyncTeX forward and inverse search (LaTeX, and Typst once it supports it).
- Portals: pin a figure, equation or definition into a side view that stays visible while you read the text
  referring to it.
- Bibliography awareness: parse the reference list into structured entries (authors, title, arXiv ID, DOI).
  Peek a citation and get the parsed entry with *open on arXiv / copy BibTeX / open local PDF*.
- A per-document notes file (Markdown) next to the highlights, editable from the text panel.
- A library: recent documents grouped by tag or folder, plus full-text search across previously opened papers.

## 0.3: AI features

The architecture already has the pieces an assistant needs:

| need | already in place |
|---|---|
| read the document | `page_text`, `selection`, `outline`, `highlights` (in Lua and on the UI thread) |
| act in the reader | the command registry (`exec`, remote control) |
| show answers | the side text panel with streaming `append` |
| run without blocking | async `spawn` / worker threads |

Planned:

- **Built-in assistant panel**: ask about the selection, the page, or the whole paper. Answers stream into the
  text panel and cite page numbers that are themselves clickable/hintable jumps.
- **Explain**: an equation, figure, or paragraph (`v` → `ga`), with the surrounding context (section,
  referenced equations resolved via smart references) automatically included.
- **Reference resolution**: "what is [23]?" fetches the entry and summarizes the cited paper (arXiv / Semantic
  Scholar).
- **Summaries**: per section and per paper, cached in the document's state file.
- **Semantic search**: embed page chunks once (in the background, cached on disk) and search by meaning next to
  the literal `/` search.
- **Agent tools**: expose folio commands as tools, so an external agent can navigate, highlight and annotate over
  the remote socket (`--remote` with JSON replies).
- **Provider-agnostic**: a small `ai` config section (provider, model, API key environment variable). Lua can
  override or add providers, so local models work too.
- **Privacy**: nothing leaves the machine unless a command that sends it is explicitly run. A per-document
  "never send" flag.

## Later

- More formats: EPUB and DjVu (via a MuPDF backend behind the existing `Backend` trait), and plain images.
- Handwritten/ink annotations and a drawing layer.
- Tabs or multiple windows, and a split view of the same document.
- Sync of highlights/notes across machines (plain JSON files, so a git or cloud folder works).
- Windows support (the code is mostly portable; IPC needs a named-pipe variant).

## Versioning

folio uses semantic versioning. Until 1.0, minor versions (0.x) may change config keys, key defaults or the Lua
API. Every such change is listed in the changelog. The version is in `Cargo.toml` and shown by `folio --version`.
