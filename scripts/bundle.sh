#!/usr/bin/env bash
#
# Build DockCV.app (and optionally a DMG) from a release binary.
#
# Usage:
#   scripts/bundle.sh                     # unsigned .app — local use only
#   scripts/bundle.sh --sign "Developer ID Application: Name (TEAMID)"
#   scripts/bundle.sh --sign "..." --notarize KEYCHAIN_PROFILE
#   scripts/bundle.sh --sign "..." --notarize PROFILE --dmg
#
# Signing and notarisation need an Apple Developer ID certificate in the login
# keychain and, for notarisation, credentials stored with
#
#   xcrun notarytool store-credentials KEYCHAIN_PROFILE \
#       --apple-id you@example.com --team-id TEAMID --password <app-specific>
#
# Neither can be done for you: they are tied to an Apple Developer account.
# Without them the .app still runs on the machine that built it, and is blocked
# by Gatekeeper on every other machine.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/DockCV.app"

SIGN_IDENTITY=""
NOTARY_PROFILE=""
MAKE_DMG=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sign)     SIGN_IDENTITY="${2:?--sign needs an identity}"; shift 2 ;;
    --notarize) NOTARY_PROFILE="${2:?--notarize needs a keychain profile}"; shift 2 ;;
    --dmg)      MAKE_DMG=1; shift ;;
    -h|--help)  sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)          echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "bundle.sh builds a macOS .app and only runs on macOS." >&2
  exit 1
fi

# The single source of version truth — asked of cargo rather than read out of
# the manifest by eye. The hand-rolled parser this replaces looked for
# `version = ` inside `[package]`, and stopped finding it the day the version
# moved to `[workspace.package]` and the package started inheriting it: the
# build failed with "could not read version from Cargo.toml" for a manifest
# that was perfectly valid.
VERSION="$(cargo metadata --format-version 1 --no-deps --manifest-path "$ROOT/Cargo.toml" \
  | python3 -c 'import json,sys; m=json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"]=="dockcv"))')"
[[ -n "$VERSION" ]] || { echo "could not read version from Cargo.toml" >&2; exit 1; }
echo "==> DockCV $VERSION"

# Panic locations and dependency paths otherwise embed the absolute path of the
# machine that built this — `/Users/<name>/.cargo/registry/...` — so every copy
# handed to anyone carries one person's home directory and directory layout to
# whoever runs `strings` on it. Cargo's own `trim-paths` would be the place for
# this, but it is not stable on the pinned toolchain; the flags below are what
# it does. Set here rather than in `.cargo/config.toml` on purpose: a global
# RUSTFLAGS change invalidates every cached build, and this only ever needs to
# be true of what ships. Panics still name a file and a line, which is all a
# report from a user can use.
REMAP="--remap-path-prefix=$HOME/.cargo/registry=/cargo/registry"
REMAP="$REMAP --remap-path-prefix=$HOME/.cargo/git=/cargo/git"
REMAP="$REMAP --remap-path-prefix=$HOME/.rustup=/rustup"
REMAP="$REMAP --remap-path-prefix=$ROOT=/dockcv"

# `-p dockcv`, not the workspace: the bundle ships exactly one binary, and
# building the rest — the browser crate above all — is minutes and gigabytes
# spent on artefacts nothing here will open. The macOS release runner filled
# its disk doing it.
echo "==> cargo build --release -p dockcv"
(cd "$ROOT" && RUSTFLAGS="${RUSTFLAGS:-} $REMAP" cargo build --release --locked -p dockcv)

BIN="$ROOT/target/release/dockcv"
[[ -x "$BIN" ]] || { echo "no release binary at $BIN" >&2; exit 1; }

# A build handed to someone else must not describe the machine that made it.
# The remapping above removes the compile-time paths rustc embeds; this checks
# it actually held, because the failure is silent and the next person to add an
# `env!("CARGO_MANIFEST_DIR")` would not notice.
#
# The exception is GPUI's Metal shaders: `xcrun metal` records its own source
# locations in the .metallib and takes no flag to stop, so those paths are the
# dependency's to fix, not ours. They are named here rather than waved away so
# the day one of them changes, this fires and someone looks.
echo "==> checking the binary describes nothing local"
LOCAL="$(strings -a "$BIN" | grep -F "$HOME" | grep -vE 'shaders\.(metal|air|metallib)' || true)"
if [[ -n "$LOCAL" ]]; then
  echo "    ! the binary carries paths from this machine:" >&2
  # The offending strings themselves, clipped: extracting "just the path" needs
  # a regex over $HOME, and $HOME is not a safe regex.
  echo "$LOCAL" | sort -u | head -5 | cut -c1-100 | sed 's/^/    ! /' >&2
  echo "    ! fix the source of these before shipping — see REMAP above." >&2
  exit 1
fi
SHADER_PATHS="$(strings -a "$BIN" | grep -Fc "$HOME" || true)"
echo "    clean, apart from $SHADER_PATHS Metal shader paths from gpui's build script"

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/dockcv"
sed "s/@VERSION@/$VERSION/g" "$ROOT/packaging/Info.plist" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# The licences are not documentation here — THIRD-PARTY-NOTICES.md says they
# ship with the binary, and the OFL says the same about the font notices. The
# bundle is the binary, so this is where they go.
cp "$ROOT/LICENSE" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" \
   "$ROOT/THIRD-PARTY-NOTICES.md" "$APP/Contents/Resources/"
mkdir -p "$APP/Contents/Resources/fonts"
cp "$ROOT/assets/fonts/LICENSE-OFL.txt" \
   "$ROOT/assets/fonts/NOTICE-typst-assets.txt" \
   "$APP/Contents/Resources/fonts/"

echo "==> icon"
if command -v rsvg-convert >/dev/null 2>&1; then
  ICONSET="$DIST/AppIcon.iconset"
  rm -rf "$ICONSET"; mkdir -p "$ICONSET"
  # The set macOS actually asks for; anything missing falls back to a blurry
  # upscale of whatever is nearest.
  for spec in "16 icon_16x16" "32 icon_16x16@2x" "32 icon_32x32" "64 icon_32x32@2x" \
              "128 icon_128x128" "256 icon_128x128@2x" "256 icon_256x256" \
              "512 icon_256x256@2x" "512 icon_512x512" "1024 icon_512x512@2x"; do
    set -- $spec
    rsvg-convert -w "$1" -h "$1" "$ROOT/assets/icon.svg" -o "$ICONSET/$2.png"
  done
  iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
  rm -rf "$ICONSET"
else
  echo "    ! rsvg-convert not found (brew install librsvg) — shipping without an icon."
  echo "    ! macOS will draw the generic application icon."
fi

if [[ -n "$SIGN_IDENTITY" ]]; then
  echo "==> signing"
  # --options runtime is the hardened runtime, which notarisation requires.
  # --timestamp too: an unstamped signature stops validating when the
  # certificate expires.
  codesign --force --deep --options runtime --timestamp \
           --sign "$SIGN_IDENTITY" "$APP"
  codesign --verify --strict --verbose=2 "$APP"
else
  # Ad-hoc: a real, structurally valid signature with no identity behind it.
  # It buys exactly one thing, and it is the thing that matters for handing a
  # build to someone: without any signature macOS reports the app as *damaged*
  # and offers no way past, while an ad-hoc signed app gets the ordinary
  # "unidentified developer" dialog the recipient can approve. It is not
  # notarisation and does not pretend to be.
  echo "==> ad-hoc signing (no Developer ID available)"
  codesign --force --sign - "$APP"
  codesign --verify --strict "$APP"
  echo "==> NOT NOTARISED"
  echo "    On another Mac the first launch is blocked until the recipient"
  echo "    approves it: System Settings ▸ Privacy & Security ▸ Open Anyway."
  echo "    HOW-TO-OPEN.txt is written beside the .app as the text for the"
  echo "    download page — the .dmg itself carries only the app."
  echo "    Re-run with --sign \"Developer ID Application: … (TEAMID)\" to"
  echo "    remove that step for everyone."
fi

if [[ -n "$NOTARY_PROFILE" ]]; then
  [[ -n "$SIGN_IDENTITY" ]] || { echo "notarisation requires --sign" >&2; exit 1; }
  echo "==> notarising (this waits on Apple, typically a few minutes)"
  ZIP="$DIST/DockCV-$VERSION.zip"
  ditto -c -k --keepParent "$APP" "$ZIP"
  xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
  # Staple, so the app validates offline. Without this a user with no network
  # on first launch still sees the Gatekeeper warning.
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
  rm -f "$ZIP"
fi

if [[ -z "$NOTARY_PROFILE" ]]; then
  # Read from the binary, never hard-coded: a note that promises Intel
  # support an arm64-only build does not have is worse than no note.
  ARCHS="$(lipo -archs "$APP/Contents/MacOS/dockcv")"
  case "$ARCHS" in
    *arm64*x86_64*|*x86_64*arm64*) ARCH_LINE="Apple Silicon and Intel Macs." ;;
    *arm64*)                       ARCH_LINE="Apple Silicon Macs (M1 or newer). Not Intel." ;;
    *)                             ARCH_LINE="Intel Macs. Not Apple Silicon." ;;
  esac
  # Written next to the .app, not into the .dmg: distribution is through the
  # website, and the place to explain a first launch is the page someone is
  # already reading, not a file inside a disk image they have to open first.
  # This is that text, kept in sync with the build it describes.
  cat > "$DIST/HOW-TO-OPEN.txt" <<NOTE
Opening DockCV the first time
=============================

Built for: $ARCH_LINE

This build carries an ad-hoc signature: a valid signature with no Apple
Developer account behind it, because notarisation needs a paid one. macOS
therefore cannot name the developer, and asks you once before running it.

  1. Drag DockCV to your Applications folder.
  2. Double-click it. macOS will refuse and say it cannot verify the
     developer. Click Done.
  3. Open System Settings ▸ Privacy & Security, scroll to Security, and click
     "Open Anyway" next to DockCV.
  4. Double-click DockCV again and confirm. That is the last time you'll see
     this.

What it needs
-------------

macOS 11 or newer, on the architecture named above. Nothing else: everything
DockCV stores is plain TOML in a folder you choose, and it needs no network to
do any of its work. The one request it can make is an update check, which is
off until you turn it on in Settings and never downloads anything by itself.

If something is wrong
---------------------

DockCV keeps a log of what it did, at ~/Library/Logs/DockCV/dockcv.log —
Help ▸ Reveal Log in Finder opens the folder. It records what happened, never
what you wrote: no CV text, no diary entries, and your home folder is written
as ~ rather than by your account name. Send it with a bug report and it will
usually say what went wrong.

"DockCV is damaged and can't be opened" means the download was corrupted or
altered in transit — get a fresh copy rather than working around it.
NOTE
fi

if [[ "$MAKE_DMG" == "1" ]]; then
  echo "==> dmg"
  DMG="$DIST/DockCV-$VERSION.dmg"
  STAGE="$DIST/dmg-stage"
  rm -rf "$STAGE" "$DMG"; mkdir -p "$STAGE"
  cp -R "$APP" "$STAGE/"
  ln -s /Applications "$STAGE/Applications"
  hdiutil create -volname "DockCV $VERSION" -srcfolder "$STAGE" \
                 -ov -format UDZO "$DMG" >/dev/null
  rm -rf "$STAGE"
  [[ -n "$SIGN_IDENTITY" ]] && codesign --force --sign "$SIGN_IDENTITY" "$DMG"
  echo "    $DMG"
fi

echo "==> done: $APP"
du -sh "$APP" | sed 's/^/    /'
