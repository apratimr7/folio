#!/usr/bin/env bash
# Build folio.app on macOS.
#
#   ./scripts/build-macos.sh            # app linked against Homebrew's poppler (fine on your own Mac)
#   BUNDLE=1 ./scripts/build-macos.sh   # also copy the dylibs into the app (needs `brew install dylibbundler`)
#
# Requirements: Xcode command line tools, Homebrew, rustup.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null; then
  echo "error: Rust is not installed — run: curl https://sh.rustup.rs -sSf | sh" >&2
  exit 1
fi
for pkg in pkg-config poppler; do
  brew list "$pkg" >/dev/null 2>&1 || brew install "$pkg"
done
export PKG_CONFIG_PATH="$(brew --prefix)/lib/pkgconfig:$(brew --prefix)/share/pkgconfig:${PKG_CONFIG_PATH:-}"

cargo build --release

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
ARCH=$(uname -m)
APP=target/release/folio.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/folio "$APP/Contents/MacOS/folio"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

if [[ "${BUNDLE:-0}" == "1" ]]; then
  brew list dylibbundler >/dev/null 2>&1 || brew install dylibbundler
  mkdir -p "$APP/Contents/Frameworks"
  dylibbundler -od -b -x "$APP/Contents/MacOS/folio" -d "$APP/Contents/Frameworks" -p @executable_path/../Frameworks/
fi

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
