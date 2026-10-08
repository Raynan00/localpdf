#!/bin/bash
# Put the bundled LibreOffice inside LocalPDF.app, re-seal the app, and build the DMG.
#   scripts/macos-package.sh <path/to/LocalPDF.app> <arch-label: aarch64|x64>
set -euo pipefail
APP="$1"; ARCH="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LO="$ROOT/src-tauri/engines-mac/LibreOffice.app"
VERSION="$(node -p "require('$ROOT/src-tauri/tauri.conf.json').version")"
OUT="$ROOT/target/dmg"

[ -d "$LO" ] || { echo "Missing $LO; run scripts/fetch-engines.mjs first" >&2; exit 1; }
rm -rf "$APP/Contents/Resources/LibreOffice.app"
ditto "$LO" "$APP/Contents/Resources/LibreOffice.app"
xattr -cr "$APP"
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

mkdir -p "$OUT"
STAGE="$(mktemp -d)"
ditto "$APP" "$STAGE/LocalPDF.app"
ln -s /Applications "$STAGE/Applications"
DMG="$OUT/LocalPDF_${VERSION}_${ARCH}.dmg"
rm -f "$DMG"
# hdiutil sometimes fails with "Resource busy" while Spotlight or XProtect is
# still looking at freshly written files; give it a few tries.
for attempt in 1 2 3 4 5; do
  if hdiutil create -volname "LocalPDF" -srcfolder "$STAGE" -ov -format UDZO -imagekey zlib-level=9 "$DMG"; then
    break
  fi
  if [ "$attempt" = 5 ]; then exit 1; fi
  echo "hdiutil failed (attempt $attempt), retrying"
  rm -f "$DMG"
  sleep $((attempt * 10))
done
rm -rf "$STAGE"
du -sh "$APP" "$DMG"
