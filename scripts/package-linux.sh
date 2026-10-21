#!/usr/bin/env bash
# Build for Linux (named after the package in Cargo.toml) and pack it with the README, license and a .desktop file.
#
#   ./scripts/package-linux.sh
#
# Requirements: a C compiler, make, clang/libclang, the xkbcommon and GL headers (see docs/BUILDING.md), rustup.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

APP=$(grep -m1 '^name' Cargo.toml | cut -d'"' -f2)
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=$(uname -m)
NAME="$APP-$VERSION-linux-$ARCH"
DIR="target/release/$NAME"
rm -rf "$DIR"
mkdir -p "$DIR"
cp "target/release/$APP" README.md LICENSE "$DIR/"
sed "s/@NAME@/$APP/g" packaging/linux/app.desktop.in > "$DIR/$APP.desktop"
strip "$DIR/$APP" 2>/dev/null || true
tar -C target/release -czf "target/release/$NAME.tar.gz" "$NAME"
echo
echo "built target/release/$NAME.tar.gz"
echo "Install: tar xzf $NAME.tar.gz && install -Dm755 $NAME/$APP ~/.local/bin/$APP"
echo "         install -Dm644 $NAME/$APP.desktop ~/.local/share/applications/$APP.desktop"
