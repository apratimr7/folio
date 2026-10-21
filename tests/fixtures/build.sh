#!/bin/sh
# Regenerate the test PDFs from tests/fixtures/src/*.tex (needs pdflatex). The PDFs are committed, so this is
# only needed after editing a source. encrypted.pdf is made by an ignored test, see README.md.
set -eu
cd "$(dirname "$0")"
export SOURCE_DATE_EPOCH=0 FORCE_SOURCE_DATE=1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for tex in src/*.tex; do
  name=$(basename "$tex" .tex)
  # Twice, so the table of contents and references resolve.
  for _ in 1 2; do
    pdflatex -interaction=nonstopmode -halt-on-error -output-directory "$tmp" "$tex" >"$tmp/$name.stdout" ||
      { cat "$tmp/$name.stdout"; exit 1; }
  done
  cp "$tmp/$name.pdf" "$name.pdf"
  echo "built $name.pdf"
done
