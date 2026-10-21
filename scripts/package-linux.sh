#!/usr/bin/env bash
# Build folio for Linux and pack it with the README, license and a .desktop file.
#
#   ./scripts/package-linux.sh
#
# Requirements: a C compiler, make, clang/libclang, the xkbcommon and GL headers (see docs/BUILDING.md), rustup.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=$(uname -m)
NAME="folio-$VERSION-linux-$ARCH"
DIR="target/release/$NAME"
rm -rf "$DIR"
mkdir -p "$DIR"
cp target/release/folio README.md LICENSE packaging/linux/folio.desktop "$DIR/"
strip "$DIR/folio" 2>/dev/null || true
tar -C target/release -czf "target/release/$NAME.tar.gz" "$NAME"
echo
echo "built target/release/$NAME.tar.gz"
echo "Install: tar xzf $NAME.tar.gz && install -Dm755 $NAME/folio ~/.local/bin/folio"
echo "         install -Dm644 $NAME/folio.desktop ~/.local/share/applications/folio.desktop"
