#!/usr/bin/env bash
#
# Build the browser module: dist/web/{dockcv_wasm.js, dockcv_wasm_bg.wasm}.
#
#   scripts/wasm.sh              # the module a page loads
#   scripts/wasm.sh --node       # the same, callable from node (for the smoke test)
#
# Needs `rustup target add wasm32-unknown-unknown` and `cargo install
# wasm-bindgen-cli`. Brotli is optional and only used to report the number that
# matters — what a visitor actually downloads.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/dist/web"
TARGET="web"
[[ "${1:-}" == "--node" ]] && TARGET="nodejs"

echo "==> cargo build -p dockcv-wasm --profile wasm-release"
# `wasm-release` lives in the *workspace root* manifest: cargo ignores
# `[profile.*]` in a member, quietly enough that this shipped 31 MB before
# anyone noticed the settings were doing nothing.
(cd "$ROOT" && cargo build -p dockcv-wasm --profile wasm-release --locked \
    --target wasm32-unknown-unknown)

WASM="$ROOT/target/wasm32-unknown-unknown/wasm-release/dockcv_wasm.wasm"

echo "==> wasm-bindgen --target $TARGET"
rm -rf "$OUT"; mkdir -p "$OUT"
wasm-bindgen "$WASM" --out-dir "$OUT" --target "$TARGET" --no-typescript

# Deliberately no `wasm-opt`. Measured on this module: -Oz takes the raw file
# from 22.8 MB to 20.2 MB and the *compressed* size from 6.10 MB to 6.28 MB —
# it restructures code in ways that compress worse, and compressed is what the
# visitor waits for. Re-measure before adding it back.

BG="$OUT/dockcv_wasm_bg.wasm"
printf '==> %.1f MB raw\n' "$(echo "$(stat -f%z "$BG") / 1048576" | bc -l)"
if command -v brotli >/dev/null 2>&1; then
  brotli -q 11 -f -o "$BG.br" "$BG"
  printf '==> %.2f MB brotli — this is the number that matters\n' \
    "$(echo "$(stat -f%z "$BG.br") / 1048576" | bc -l)"
  echo "    Serve the .br with Content-Encoding: br and a long immutable cache."
fi
echo "    Load it on click, never on page load."
