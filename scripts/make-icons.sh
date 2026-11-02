#!/usr/bin/env bash
# Renders every committed icon file from packaging/icon.svg, the one source of the app
# icon. Run it after changing the SVG (macOS: it draws with AppKit). Each size is drawn
# from the SVG, so small sizes stay sharp.
set -euo pipefail
cd "$(dirname "$0")/.."
SVG=packaging/icon.svg
render() { swift scripts/render-icon.swift "$SVG" "$2" "$1"; }
render 1024 packaging/macos/icon-1024.png
render 256 crates/ternarywire-app/assets/icon-256.png
for s in 16 24 32 48 64 128 256 512; do
  render "$s" "packaging/linux/icons/hicolor/${s}x${s}/apps/ternarywire.png"
done
python3 scripts/make-windows-icon.py "$SVG" packaging/windows/ternarywire.ico
