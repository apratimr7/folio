# folio patches to mupdf-rs 0.8.0

This is the `mupdf` crate 0.8.0 from crates.io (AGPL-3.0, https://github.com/messense/mupdf-rs) with two fixes.
Remove this directory and the `[patch.crates-io]` entry in `Cargo.toml` once they're upstream.

1. `src/document.rs` `walk_outlines`: an outline item with a null title crashed (`strlen(NULL)`). Damaged or
   truncated PDFs have these. Now an empty title. Link resolution errors no longer abort the whole outline.
2. `src/page.rs` `LinkIter::next`: `LinkDestination::from_uri(..).unwrap()` panicked on links MuPDF can't
   resolve, and a null `uri` crashed. Now the link is kept with `dest: None` (or skipped when it has no URI).
