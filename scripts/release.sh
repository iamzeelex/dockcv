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

# The newest *released* entry in the changelog — the first `## [x.y.z]`, which
# skips `## [Unreleased]` because that heading carries no digits.
#
# Asked for as a value rather than grepped for as a presence: a check that only
# asks "does an entry for the manifest version exist?" is blind in the
# direction the project actually drifted. The manifest sat at 0.1.0 while the
# changelog announced 0.2.0, and the guard stayed green the whole time,
# because 0.1.0's own entry was still there further down the file.
latest_changelog_version() {
  grep -m1 -E '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' "$CHANGELOG" \
    | sed -E 's/^## \[([^]]+)\].*/\1/'
}

CURRENT_VERSION="$(get_current_version)"

if [[ $# -eq 0 ]]; then
  echo "Usage: scripts/release.sh [--check | <new_version>]" >&2
  echo "Current workspace version: $CURRENT_VERSION" >&2
  exit 1
fi

if [[ "$1" == "--check" ]]; then
  echo "==> Verifying version consistency for DockCV ($CURRENT_VERSION)..."

  CHANGELOG_VERSION="$(latest_changelog_version)"

  if [[ -z "$CHANGELOG_VERSION" ]]; then
    echo "ERROR: CHANGELOG.md has no released version entry at all." >&2
    echo "       Expected a heading of the form '## [$CURRENT_VERSION] - <date>'." >&2
    exit 1
  fi

  # Equality in both directions. A manifest ahead of the changelog ships a
  # build nobody can read the notes for; a changelog ahead of the manifest
  # ships a build that misreports itself in Settings > About and to anything
  # comparing versions.
  if [[ "$CHANGELOG_VERSION" != "$CURRENT_VERSION" ]]; then
    echo "ERROR: version mismatch." >&2
    echo "       Cargo.toml says   $CURRENT_VERSION" >&2
    echo "       CHANGELOG.md says $CHANGELOG_VERSION (newest released entry)" >&2
    echo "       Bump the manifest with scripts/release.sh <version>, or move" >&2
    echo "       the unreleased notes back under '## [Unreleased]'." >&2
    exit 1
  fi

  echo "SUCCESS: Cargo.toml and CHANGELOG.md both say $CURRENT_VERSION."
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

# 2. The changelog must already lead with the version being released — not
#    merely mention it somewhere, or the notes for an older release would
#    satisfy the gate.
CHANGELOG_VERSION="$(latest_changelog_version)"
if [[ "$CHANGELOG_VERSION" != "$NEW_VERSION" ]]; then
  echo "ERROR: CHANGELOG.md's newest released entry is '${CHANGELOG_VERSION:-none}', not '$NEW_VERSION'." >&2
  echo "Please add release notes under '## [$NEW_VERSION] - $(date +%Y-%m-%d)' before releasing." >&2
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
