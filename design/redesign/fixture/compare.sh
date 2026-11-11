#!/usr/bin/env bash
# The app and the prototype side by side, as two PNGs of the same size.
#
#   compare.sh <screen-id> [light|dark] [WxH]
#
# Writes <outdir>/<id>-<theme>-app.png (shoot.sh) and
# <outdir>/<id>-<theme>-design.png (headless Chrome, bare mode, DPR 2) and
# prints both paths. outdir: $OUTDIR, else $TMPDIR/tw-shots. The size
# defaults to the screen's own (1280x820, or the "_size" of its steps).
# CHROME overrides the Chrome binary.
set -euo pipefail

[ $# -ge 1 ] || { sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
ID="$1"
THEME="${2:-light}"
SIZE="${3:-}"
TMP="${TMPDIR:-/tmp}"
TMP="${TMP%/}"
OUT="${OUTDIR:-$TMP/tw-shots}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESIGN="$(cd "$HERE/.." && pwd)"
STEPS="$HERE/steps/$ID.json"
[ -f "$STEPS" ] || { echo "compare.sh: no steps for '$ID'" >&2; exit 2; }
if [ -z "$SIZE" ]; then
  SIZE="$(python3 -c 'import json,sys
for s in json.load(open(sys.argv[1])):
    if isinstance(s, dict) and "_size" in s: print(s["_size"]); break
else: print("1280x820")' "$STEPS")"
fi
W="${SIZE%x*}"
H="${SIZE#*x}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

CHROME="${CHROME:-}"
if [ -z "$CHROME" ]; then
  for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
           "/Applications/Chromium.app/Contents/MacOS/Chromium" google-chrome chromium chromium-browser; do
    if [ -x "$c" ] || command -v "$c" >/dev/null 2>&1; then CHROME="$c"; break; fi
  done
fi
[ -n "$CHROME" ] || { echo "compare.sh: Chrome not found (set CHROME)" >&2; exit 1; }

# The prototype, in the background while the app runs. Every run gets its
# own profile, so several can run at once.
DESIGN_PNG="$OUT/$ID-$THEME-design.png"
rm -f "$DESIGN_PNG"
PROFILE="$(mktemp -d "$TMP/tw-chrome.XXXXXX")"
URL="file://$DESIGN/index.html?bare=1&size=${W}x${H}&theme=$THEME#$ID"
"$CHROME" --headless=new --disable-gpu --hide-scrollbars --no-first-run --no-default-browser-check \
  --user-data-dir="$PROFILE" --window-size="$W,$H" --force-device-scale-factor=2 --virtual-time-budget=1500 \
  --screenshot="$DESIGN_PNG" "$URL" >"$PROFILE.log" 2>&1 &
CHROME_PID=$!

APP_PNG="$("$HERE/shoot.sh" "$ID" "$THEME" "$SIZE" "$OUT")" || { wait "$CHROME_PID" || true; rm -rf "$PROFILE" "$PROFILE.log"; exit 1; }
wait "$CHROME_PID" || true
if [ ! -s "$DESIGN_PNG" ]; then
  cat "$PROFILE.log" >&2
  echo "compare.sh: the prototype screenshot failed" >&2
  rm -rf "$PROFILE" "$PROFILE.log"
  exit 1
fi
rm -rf "$PROFILE" "$PROFILE.log"
echo "$APP_PNG"
echo "$DESIGN_PNG"
