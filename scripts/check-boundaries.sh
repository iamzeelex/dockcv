#!/usr/bin/env bash
#
# Fail when a crate depends on something its role says it must not.
#
# The crates in this workspace are layers, and a layer is only a layer while
# nothing reaches past it. These are the rules that were written down in prose
# (CLAUDE.md, each crate's manifest) and then had nothing checking them:
#
#   dockcv-core           no GPUI, no screen widgets, and no knowledge of the
#                         filesystem crate — it builds for the browser.
#   dockcv-vault          no GPUI and no screen widgets — files, not windows.
#   dockcv-ui-components  nothing about résumés, vaults or Typst.
#
# Only *normal* dependencies are counted: a dev-dependency does not ship, and
# the fixtures a test builds on are allowed to be anywhere.
#
# Usage: scripts/check-boundaries.sh

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

failed=0

# check <crate> <banned crate>...
check() {
  local crate="$1"
  shift
  local tree
  tree="$(cargo tree --locked -p "$crate" -e normal --prefix none | awk '{print $1}' | sort -u)"
  for banned in "$@"; do
    if grep -qx -- "$banned" <<<"$tree"; then
      echo "boundary broken: $crate depends on $banned" >&2
      failed=1
    fi
  done
}

GPUI=(gpui gpui_platform gpui-component dockcv-ui-components)

check dockcv-core "${GPUI[@]}" dockcv-vault
check dockcv-vault "${GPUI[@]}"
check dockcv-ui-components dockcv-core dockcv-vault typst

if [[ $failed -ne 0 ]]; then
  exit 1
fi
echo "boundaries hold: core, vault and ui-components depend only on what their role allows"
