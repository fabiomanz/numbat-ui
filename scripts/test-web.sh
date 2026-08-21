#!/usr/bin/env bash
#
# Runs the JavaScript-side smoke test against a node-targeted build of the
# wasm module. The Rust logic is covered by `cargo test`; this covers the
# boundary.
set -euo pipefail

cd "$(dirname "$0")/.."

WB_VERSION=$(sed -n 's/^wasm-bindgen = "=\([0-9.]*\)".*/\1/p' crates/web/Cargo.toml)
if ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
  rustup target add wasm32-unknown-unknown
fi
if [ "$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')" != "$WB_VERSION" ]; then
  cargo install wasm-bindgen-cli --version "$WB_VERSION"
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cargo build --profile wasm-release -p numbat-ui-web --target wasm32-unknown-unknown
wasm-bindgen --target nodejs --no-typescript --out-dir "$TMP" \
  target/wasm32-unknown-unknown/wasm-release/numbat_ui_web.wasm

# Currency assertions are skipped if the rates cannot be fetched.
curl -fsS --max-time 20 \
  https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml \
  -o "$TMP/rates.xml" || true

node crates/web/tests/smoke.js "$TMP" "$TMP/rates.xml"
