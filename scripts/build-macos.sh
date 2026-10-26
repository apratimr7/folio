#!/usr/bin/env bash
# Build the macOS app bundle (named after the package in Cargo.toml).
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

NAME=$(grep -m1 '^name' Cargo.toml | cut -d'"' -f2)
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=$(uname -m)
APP="target/release/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/release/$NAME" "$APP/Contents/MacOS/$NAME"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@NAME@/$NAME/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp packaging/icons/app.icns "$APP/Contents/Resources/$NAME.icns"

# Ad-hoc signature: required for arm64 binaries to launch at all.
codesign --force --deep --sign - "$APP"

ZIP="target/release/$NAME-$VERSION-macos-$ARCH.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo
echo "built $APP"
echo "      $ZIP"
echo
echo "Run it:  open $APP --args ~/paper.pdf      (or: $APP/Contents/MacOS/$NAME ~/paper.pdf)"
echo "Install: cp -R $APP /Applications && ln -sf /Applications/$NAME.app/Contents/MacOS/$NAME /usr/local/bin/$NAME"
