#!/usr/bin/env bash
#
# Release helper script for DockCV.
#
# Usage:
#   scripts/release.sh --check        # Verify current version & CHANGELOG consistency
#   scripts/release.sh <new_version>  # Bump version, check changelog, test, commit & tag release
#
# Example:
#   scripts/release.sh 0.2.0
#

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/Cargo.toml"
CHANGELOG="$ROOT/CHANGELOG.md"

get_current_version() {
  cargo metadata --format-version 1 --no-deps --manifest-path "$MANIFEST" \
    | python3 -c 'import json,sys; m=json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"]=="dockcv"))'
}

CURRENT_VERSION="$(get_current_version)"

if [[ $# -eq 0 ]]; then
  echo "Usage: scripts/release.sh [--check | <new_version>]" >&2
  echo "Current workspace version: $CURRENT_VERSION" >&2
  exit 1
fi

if [[ "$1" == "--check" ]]; then
  echo "==> Verifying version consistency for DockCV ($CURRENT_VERSION)..."
  
  if ! grep -q "## \[$CURRENT_VERSION\]" "$CHANGELOG"; then
    echo "ERROR: CHANGELOG.md is missing an entry for current version [$CURRENT_VERSION]." >&2
    exit 1
  fi

  echo "==> Running cargo check & test..."
  cargo check --workspace
  cargo test --workspace

  echo "SUCCESS: Version $CURRENT_VERSION is consistent and all tests pass."
  exit 0
fi

NEW_VERSION="$1"
# Strip leading 'v' if user accidentally passed 'v0.2.0'
NEW_VERSION="${NEW_VERSION#v}"

echo "==> Preparing release v$NEW_VERSION (current: v$CURRENT_VERSION)..."

# 1. Ensure working directory is clean
if [[ -n "$(git status --porcelain)" ]]; then
  echo "ERROR: Working directory is dirty. Please commit or stash changes before creating a release." >&2
  exit 1
fi

# 2. Check if CHANGELOG.md has an entry for NEW_VERSION
if ! grep -q "## \[$NEW_VERSION\]" "$CHANGELOG"; then
  echo "ERROR: CHANGELOG.md does not contain an entry for '## [$NEW_VERSION]'." >&2
  echo "Please update CHANGELOG.md with release notes before releasing." >&2
  exit 1
fi

# 3. Update workspace version in Cargo.toml
echo "==> Bumping version in Cargo.toml to $NEW_VERSION..."
python3 -c "
import re, sys

with open('$MANIFEST', 'r') as f:
    content = f.read()

# Replace version under [workspace.package]
new_content = re.sub(
    r'(\[workspace\.package\]\s*version\s*=\s*\")[^\"]+(\")',
    r'\g<1>$NEW_VERSION\g<2>',
    content
)

with open('$MANIFEST', 'w') as f:
    f.write(new_content)
"

# 4. Update Cargo.lock
cargo check --workspace >/dev/null 2>&1 || true

UPDATED_VERSION="$(get_current_version)"
if [[ "$UPDATED_VERSION" != "$NEW_VERSION" ]]; then
  echo "ERROR: Version bump failed. Expected $NEW_VERSION, found $UPDATED_VERSION." >&2
  exit 1
fi

# 5. Run tests
echo "==> Running workspace tests..."
cargo test --workspace

# 6. Commit and Tag
echo "==> Creating release commit and Git tag v$NEW_VERSION..."
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): bump version to v$NEW_VERSION"
git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"

echo ""
echo "========================================================"
echo " SUCCESS: Release v$NEW_VERSION created!"
echo "========================================================"
echo "To publish this release to GitHub:"
echo "  git push origin main"
echo "  git push origin v$NEW_VERSION"
echo ""
