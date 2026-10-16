#!/usr/bin/env bash
#
# Builds the web app (the iOS/Android PWA) into ./dist.
#
#   ./scripts/build-web.sh            # build
#   ./scripts/build-web.sh --serve    # build, then serve on http://localhost:8080
#
# The wasm target and the matching wasm-bindgen CLI are installed on demand.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT=dist
SERVE=${1:-}

# The CLI that writes the JS glue has to be the exact version of the crate that
# generated the wasm, so both come from one place: crates/web/Cargo.toml.
WB_VERSION=$(sed -n 's/^wasm-bindgen = "=\([0-9.]*\)".*/\1/p' crates/web/Cargo.toml)
if [ -z "$WB_VERSION" ]; then
  echo "Could not read the wasm-bindgen version from crates/web/Cargo.toml" >&2
  exit 1
fi

if ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
  echo "==> Adding the wasm32-unknown-unknown target"
  rustup target add wasm32-unknown-unknown
fi

if [ "$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')" != "$WB_VERSION" ]; then
  echo "==> Installing wasm-bindgen-cli $WB_VERSION"
  cargo install wasm-bindgen-cli --version "$WB_VERSION"
fi

echo "==> Compiling the core to WebAssembly"
cargo build --profile wasm-release -p numbat-ui-web --target wasm32-unknown-unknown

echo "==> Generating JavaScript bindings"
rm -rf "$OUT"
mkdir -p "$OUT"
cp -R web/. "$OUT/"
wasm-bindgen \
  --target web \
  --no-typescript \
  --out-dir "$OUT/pkg" \
  target/wasm32-unknown-unknown/wasm-release/numbat_ui_web.wasm

# Optional last squeeze (brew install binaryen). Worth ~20% of the download.
if command -v wasm-opt >/dev/null 2>&1; then
  echo "==> wasm-opt"
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    "$OUT/pkg/numbat_ui_web_bg.wasm" -o "$OUT/pkg/numbat_ui_web_bg.wasm"
fi

# Stamp the service worker so a deploy invalidates the previous cache. Without
# this the browser would keep serving the old wasm forever.
VERSION="$(git rev-parse --short HEAD 2>/dev/null || date +%s)"
if [ -n "$(git status --porcelain 2>/dev/null || true)" ]; then
  VERSION="$VERSION-dev-$(date +%s)"
fi
sed -i.bak "s/__CACHE_VERSION__/$VERSION/" "$OUT/sw.js" && rm -f "$OUT/sw.js.bak"
echo "==> Cache version: $VERSION"

echo "==> Built $OUT ($(du -sh "$OUT" | cut -f1))"

if [ "$SERVE" = "--serve" ]; then
  echo "==> http://localhost:8080  (Ctrl+C to stop)"
  python3 -m http.server 8080 --directory "$OUT"
fi
