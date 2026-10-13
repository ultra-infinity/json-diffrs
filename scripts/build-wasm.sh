#!/usr/bin/env bash
# Builds the web app's WebAssembly module (crates/json-diff-web) and its
# JavaScript bindings into ui/wasm/, for `npm run dev:web` and
# `npm run build:web`. Needs the wasm32 target and wasm-bindgen-cli, which
# `nix develop` provides:
#
#   nix develop -c scripts/build-wasm.sh
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --locked --release -p json-diff-web --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir ui/wasm --out-name json_diff_web \
  target/wasm32-unknown-unknown/release/json_diff_web.wasm
echo "wrote ui/wasm"
