#!/usr/bin/env bash
# Builds TernaryWire.app, a DMG, the app archive the in-app updater installs
# (TernaryWire-<version>-macos-<arch>.zip), a CLI tarball and a Homebrew cask
# (macOS).
#
#   scripts/package-macos.sh              # universal (arm64 + x86_64), ad-hoc signed
#   scripts/package-macos.sh --native     # host architecture only (faster)
#
# Signing and notarization (optional, needs an Apple Developer account):
#   TERNARYWIRE_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"
#   TERNARYWIRE_NOTARY_PROFILE=profile   # created once with `xcrun notarytool store-credentials`
# Without TERNARYWIRE_SIGN_IDENTITY the app is signed ad hoc: it runs on this Mac,
# but Gatekeeper on other Macs asks the user to allow it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${TERNARYWIRE_DIST_DIR:-$ROOT/target/dist-pkg}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(date +%Y%m%d%H%M)"
NATIVE=0
[[ "${1:-}" == "--native" ]] && NATIVE=1

cd "$ROOT"
mkdir -p "$OUT"
if [[ $NATIVE == 1 ]]; then
  echo "== building (dist profile, host architecture)"
  cargo build --profile dist -p ternarywire-app -p ternarywire-cli
  APP_BIN="$ROOT/target/dist/ternarywire-app"
  CLI_BIN="$ROOT/target/dist/ternarywire"
  ARCH="$(uname -m)"
else
  echo "== building (dist profile, arm64 + x86_64)"
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --profile dist --target "$t" -p ternarywire-app -p ternarywire-cli
  done
  APP_BIN="$OUT/ternarywire-app"
  CLI_BIN="$OUT/ternarywire"
  lipo -create -output "$APP_BIN" target/aarch64-apple-darwin/dist/ternarywire-app target/x86_64-apple-darwin/dist/ternarywire-app
  lipo -create -output "$CLI_BIN" target/aarch64-apple-darwin/dist/ternarywire target/x86_64-apple-darwin/dist/ternarywire
  ARCH=universal
fi

echo "== TernaryWire.app"
APP="$OUT/TernaryWire.app"
rm -rf "$OUT/TernaryWire.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$APP_BIN" "$APP/Contents/MacOS/ternarywire-app"
sed -e "s/__VERSION__/$VERSION/" -e "s/__BUILD__/$BUILD/" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
ICONSET="$OUT/TernaryWire.iconset"
rm -rf "$OUT/TernaryWire.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s packaging/macos/icon-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) packaging/macos/icon-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/TernaryWire.icns"
rm -rf "$OUT/TernaryWire.iconset"

echo "== signing"
if [[ -n "${TERNARYWIRE_SIGN_IDENTITY:-}" ]]; then
  SIGN=(--sign "$TERNARYWIRE_SIGN_IDENTITY" --options runtime --timestamp --entitlements packaging/macos/entitlements.plist)
else
  SIGN=(--sign -)
fi
codesign --force "${SIGN[@]}" "$CLI_BIN"
codesign --force "${SIGN[@]}" "$APP"
codesign --verify --strict --verbose=1 "$APP"

echo "== DMG"
DMG="$OUT/TernaryWire-$VERSION-$ARCH.dmg"
STAGE="$OUT/dmg-stage"
rm -rf "$OUT/dmg-stage"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
# An explicit size: hdiutil's own estimate is sometimes too small ("No space
# left on device" on CI runners). It also fails now and then with "Resource
# busy", so it gets a few tries.
STAGE_MB=$(du -sm "$STAGE" | cut -f1)
for try in 1 2 3; do
  hdiutil create -volname "TernaryWire $VERSION" -srcfolder "$STAGE" -size "$((STAGE_MB + 64))m" \
    -ov -format UDZO "$DMG" >/dev/null && break
  [[ $try == 3 ]] && exit 1
  echo "hdiutil failed, retrying ($try)"
  sleep 5
done
rm -rf "$OUT/dmg-stage"
[[ -n "${TERNARYWIRE_SIGN_IDENTITY:-}" ]] && codesign --force --sign "$TERNARYWIRE_SIGN_IDENTITY" --timestamp "$DMG"

if [[ -n "${TERNARYWIRE_NOTARY_PROFILE:-}" ]]; then
  echo "== notarizing"
  xcrun notarytool submit "$DMG" --keychain-profile "$TERNARYWIRE_NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi

echo "== app archive (in-app updates)"
# The signed app, zipped the way macOS expects: "Install and restart" in the
# app downloads it, checks it and puts it in place of the running copy.
ZIP="$OUT/TernaryWire-$VERSION-macos-$ARCH.zip"
rm -f "$ZIP"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"

echo "== CLI tarball"
CLI_TGZ="$OUT/ternarywire-cli-$VERSION-macos-$ARCH.tar.gz"
tar -C "$(dirname "$CLI_BIN")" -czf "$CLI_TGZ" "$(basename "$CLI_BIN")"

echo "== Homebrew cask"
SHA="$(shasum -a 256 "$DMG" | cut -d' ' -f1)"
sed -e "s/__VERSION__/$VERSION/g" -e "s/__SHA256__/$SHA/" -e "s/__ARCH__/$ARCH/g" \
  packaging/homebrew/ternarywire.rb.in > "$OUT/ternarywire.rb"

printf "\napp  %s (%.1f MB binary)\ndmg  %s (%.1f MB)\nzip  %s (%.1f MB)\ncli  %s\ncask %s\n" \
  "$APP" "$(echo "$(stat -f%z "$APP/Contents/MacOS/ternarywire-app") / 1048576" | bc -l)" \
  "$DMG" "$(echo "$(stat -f%z "$DMG") / 1048576" | bc -l)" \
  "$ZIP" "$(echo "$(stat -f%z "$ZIP") / 1048576" | bc -l)" "$CLI_TGZ" "$OUT/ternarywire.rb"
