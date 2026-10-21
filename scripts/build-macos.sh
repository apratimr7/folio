#!/usr/bin/env bash
# Build folio.app on macOS.
#
#   ./scripts/build-macos.sh
#
# MuPDF is compiled in statically, so the app has no dependencies beyond macOS itself.
# Requirements: Xcode command line tools (C compiler, make, libclang) and rustup.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null; then
  echo "error: Rust is not installed — run: curl https://sh.rustup.rs -sSf | sh" >&2
  exit 1
fi

cargo build --release

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=$(uname -m)
APP=target/release/folio.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/folio "$APP/Contents/MacOS/folio"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# Ad-hoc signature: required for arm64 binaries to launch at all.
codesign --force --deep --sign - "$APP"

ZIP="target/release/folio-$VERSION-macos-$ARCH.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo
echo "built $APP"
echo "      $ZIP"
echo
echo "Run it:  open $APP --args ~/paper.pdf      (or: $APP/Contents/MacOS/folio ~/paper.pdf)"
echo "Install: cp -R $APP /Applications && ln -sf /Applications/folio.app/Contents/MacOS/folio /usr/local/bin/folio"
