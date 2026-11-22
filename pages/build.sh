#!/usr/bin/env bash
# Assemble the static GitHub Pages site into $OUT (default: <repo>/_site).
#   needs: cargo + rustup target wasm32-unknown-unknown, wasm-bindgen CLI =0.2.130 (must match the
#   crate pin in rust/aeoi-wasm/Cargo.toml), optional wasm-opt (binaryen) for the -Oz pass.
#   env:   WASM_BINDGEN, WASM_OPT (paths), CARGO_TARGET_DIR, OUT
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
OUT="${OUT:-$ROOT/_site}"
WASM_BINDGEN="${WASM_BINDGEN:-wasm-bindgen}"
WASM_OPT="${WASM_OPT:-wasm-opt}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/rust/target}"

want="0.2.130"
have="$("$WASM_BINDGEN" --version | awk '{print $2}')"
[ "$have" = "$want" ] || { echo "wasm-bindgen CLI $have != crate pin $want" >&2; exit 1; }

(cd "$ROOT/rust" && CARGO_TARGET_DIR="$TARGET_DIR" cargo build --profile wasm-release -p aeoi-wasm --lib --target wasm32-unknown-unknown)
WASM="$TARGET_DIR/wasm32-unknown-unknown/wasm-release/aeoi_wasm.wasm"

rm -rf "$OUT"; mkdir -p "$OUT/pkg" "$OUT/vendor"
"$WASM_BINDGEN" --target web --no-typescript --out-dir "$OUT/pkg" "$WASM"
if command -v "$WASM_OPT" >/dev/null 2>&1; then
  "$WASM_OPT" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
    "$OUT/pkg/aeoi_wasm_bg.wasm" -o "$OUT/pkg/aeoi_wasm_bg.wasm.opt"
  mv "$OUT/pkg/aeoi_wasm_bg.wasm.opt" "$OUT/pkg/aeoi_wasm_bg.wasm"
else
  echo "note: wasm-opt not found, shipping the un-optimised wasm" >&2
fi

STATIC="$ROOT/rust/aeoi-web/static"
cp "$HERE/index.html" "$HERE/browser.js" "$HERE/pages.css" "$OUT/"
cp "$STATIC/app.js" "$STATIC/style.css" "$OUT/"
cp "$STATIC/vendor/plotly-basic-2.35.2.min.js" "$STATIC/vendor/plotly-LICENSE.txt" "$OUT/vendor/"
touch "$OUT/.nojekyll"
ls -l "$OUT" "$OUT/pkg" "$OUT/vendor"
