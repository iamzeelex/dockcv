#!/usr/bin/env bash
#
# Build the browser's font subsets: assets/fonts/web/*.subset.*
#
# Fonts are 3.5 MB of the 26 MB wasm module and they do not compress — the
# largest single thing a visitor waits for. A CV needs a few hundred glyphs;
# a shipped face carries thousands.
#
# The repertoire below is *derived*, not guessed: every non-ASCII character
# `resume/template.rs` and the vendored AltaCV package can emit was collected
# before choosing it, and `fonts_for_the_browser_cover_what_a_cv_can_contain`
# in typst_engine.rs fails if a face stops covering it. Widen both together.
#
# Needs fonttools: pip3 install --user fonttools

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/assets/fonts/web"

command -v pyftsubset >/dev/null || { echo "pyftsubset not found — pip3 install --user fonttools" >&2; exit 1; }

# ASCII; Latin-1 (é ò ä û ü § © ·); Latin Extended-A (ā ł ő); Cyrillic incl.
# Ukrainian; general punctuation (– — … • ‰); ₴ € ₽; № ; arrows (→);
# mathematical operators (≈ ≤ ≥ ×).
RANGES="U+0020-007E,U+00A0-00FF,U+0100-017F,U+0180-024F,U+0400-04FF,U+2010-205E,U+20A0-20BF,U+2116,U+2190-21FF,U+2200-22FF,U+2713,U+2714,U+2605,U+FB00-FB06"

subset() {
  local src="$1" dst="$2"
  pyftsubset "$src" --output-file="$dst" \
    --unicodes="$RANGES" \
    --layout-features="kern,liga,calt,onum,tnum" \
    --no-hinting --desubroutinize
  printf '    %-34s %6.0f KB → %5.0f KB\n' "$(basename "$dst")" \
    "$(echo "$(stat -f%z "$src") / 1024" | bc)" "$(echo "$(stat -f%z "$dst") / 1024" | bc)"
}

echo "==> subsetting the browser faces"
subset "$OUT/LibertinusSerif-Regular.otf" "$OUT/LibertinusSerif-Regular.subset.otf"
subset "$OUT/LibertinusSerif-Bold.otf"    "$OUT/LibertinusSerif-Bold.subset.otf"
subset "$OUT/LibertinusSerif-Italic.otf"  "$OUT/LibertinusSerif-Italic.subset.otf"
subset "$ROOT/assets/fonts/Geist-Regular.ttf" "$OUT/Geist-Regular.subset.ttf"
subset "$ROOT/assets/fonts/Geist-Bold.ttf"    "$OUT/Geist-Bold.subset.ttf"

TOTAL=$(find "$OUT" -name "*.subset.*" -exec stat -f%z {} \; | paste -sd+ - | bc)
printf '==> %.0f KB of faces for the browser, against 3574 KB unsubsetted\n' "$(echo "$TOTAL / 1024" | bc)"
