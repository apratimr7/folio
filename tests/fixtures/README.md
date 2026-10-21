# Test fixtures

Small PDFs for the test suite (`docs/TESTING.md`). The PDFs are committed, so running the tests needs no LaTeX.
Sources are in `src/`. To rebuild after editing a source, run `./build.sh` (needs `pdflatex`). The build is
deterministic: no dates or IDs in the output, and the page size is pinned to US letter (612×792 pt), whatever the
local TeX default. If you change a source, check the facts below and update the tests that rely on them.

Page numbers below are 0-based, as in the backend and `--debug`. The `--debug` output prints them 1-based (`p5`).

## refs.pdf (7 pages, hyperref)

- Page labels: none (empty) on page 0, `i` and `ii` on pages 1–2, then `1`–`4` on pages 3–6.
- Metadata: title `Folio Test Paper`, author `Folio Tests`.
- Outline: `Introduction` (page 3) › `Motivation` (page 3, depth 1), `Results` (page 4), `Proofs` (page 5).
- Page 3:
  - The heading `1 Introduction` (MuPDF splits it at the wide gap; it must come back as one line).
  - Citations `[1]`, `[2, 3]`, `[4–6]` (en dash).
  - References to `Table 2`, `Figure 1`, `Theorem 1.2`, `Lemma 3.1` and `Section 2`.
  - Typographic text `“quoted” folklore – and its exceptions—like`.
  - `renormaliza-` / `tion` hyphenated across a line break.
  - An external link `https://example.org/folio`, a `file:notes.txt` link, and 10 internal links.
  - Theorem headers `Theorem 1.1.` and `Theorem 1.2.`, and the caption `Figure 1: The experimental setup.`
- Page 4:
  - Displayed equations `E = mc2 (2.1)` and `F = ma. (2.2)`, each number on the same line as its equation.
  - References `Eq. (2.1)` and a bare `(2.1)` at a line start.
  - Captions `Table 1: A first table.` and `Table 2: Measured data.`
- Page 5: `Lemma 3.1.` header, and a proof citing `Theorem 1.1` and `Eq. (2.2)`.
- Page 6: the `References` heading, then entries `[1] A. Alpha, …` to `[6] Z. Zeta, …`, one per line.

## rudin_style.pdf (2 pages, no hyperref)

- Number-first theorem headers: `2.35 Definition.`, `2.36 Theorem.`, `2.37 Corollary.` (page 0), and
  `3.9 Definition.` (page 1).
- Page 1: an equation tagged `(3.21)` at the line end, and in-text references `Theorem 2.36`, `Definition 3.9` and
  `(3.21)`.

## nooutline.pdf (3 pages, no hyperref, no bookmarks)

- Page 0: a table of contents. Its entries end in page numbers, with dotted leaders on the subsection lines
  (`3.2 Results . . . 3`). `3.2.1` is not listed (`tocdepth` 2).
- Page 1: `1 Introduction`, with references to `Section 3.2` and `Section 3.2.1`.
- Page 2: headings `2 Background`, `3 Analysis`, `3.1 Setup`, `3.2 Results` and `3.2.1 Details`.

## twocol.pdf (1 page, two columns)

- Every word in the left column is `alpha`, and every word in the right column is `omega`. No line may contain both.

## encrypted.pdf (1 page)

- `twocol.pdf` encrypted with AES-128. The user password is `folio` and the owner password is `folio-owner`.
- It's made with MuPDF, since qpdf isn't installed everywhere. To regenerate it, run
  `FOLIO_REGENERATE_FIXTURES=1 cargo test generate_encrypted_fixture -- --ignored`. The output differs on every
  run (random IV).

## Generated at test time

- Damaged files: copies of the fixtures truncated to 0/10/50/90/99%, or with 2 KB zeroed at 5–95%. They're
  written to a temp dir by `src/tests/backend.rs` and never committed.
