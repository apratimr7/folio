#!/usr/bin/env bash
# Render the app icon (packaging/icons/app.svg) into the files the packages use. macOS only (AppKit renders the
# SVG, iconutil makes the .icns); the results are committed, so builds don't need this.
#
#   ./scripts/build-icons.sh
set -euo pipefail
cd "$(dirname "$0")/.."

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
swiftc -O scripts/tools/svg2png.swift -o "$tmp/svg2png"
render() { "$tmp/svg2png" packaging/icons/app.svg "$2" "$1"; }

set_dir="$tmp/app.iconset"
mkdir -p "$set_dir"
for s in 16 32 128 256 512; do
  render $s "$set_dir/icon_${s}x${s}.png"
  render $((s * 2)) "$set_dir/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$set_dir" -o packaging/icons/app.icns
render 256 packaging/icons/app-256.png
echo "wrote packaging/icons/app.icns and app-256.png"
