#!/usr/bin/env bash
# Screenshot the real app in the state of one prototype screen.
#
#   shoot.sh <screen-id> [light|dark] [WxH] [outdir]
#
# Defaults: light, the screen's size (1280x820, or the "_size" of its steps),
# outdir $TMPDIR/tw-shots. Prints the PNG path (<outdir>/<id>-<theme>-app.png,
# 2x pixels). Uses the app of the worktree you run it from
# (`git rev-parse --show-toplevel` of the current directory; outside a
# repository, the worktree this script is in).
#
# Steps: steps/<screen-id>.json, an automation script for
# crates/ternarywire-app/src/automation.rs with these placeholders:
#   "@W", "@H"  window size (numbers)   "@THEME"  light|dark   "@OUT"  PNG path
# and optional fixture steps that this script handles and removes:
#   {"_size": "900x640"}                 default size of the screen
#   {"_settings": {...}}                 merged into settings.json
#   {"_current": {"<owner id>": {"key": "value"}}}  current values (state.json)
#   {"_session": false}                  start without the restored tabs
#   {"_env": {"NAME": "value"}}          extra environment for the app
#   {"_poke": "webhook" | "capture"}     server.py sends sample traffic once the
#                                        listener / proxy is up
#
# Env: KEEP=1 keeps the scratch folder; TW_FIXTURE_CACHE overrides the cache
# folder; CARGO_BUILD_JOBS (default 4).
set -euo pipefail

usage() { sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
[ $# -ge 1 ] || usage
ID="$1"
THEME="${2:-light}"
SIZE="${3:-}"
TMP="${TMPDIR:-/tmp}"
TMP="${TMP%/}"
OUTDIR="${4:-$TMP/tw-shots}"
case "$THEME" in light|dark) ;; *) echo "shoot.sh: theme must be light or dark" >&2; exit 2 ;; esac

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STEPS="$HERE/steps/$ID.json"
[ -f "$STEPS" ] || { echo "shoot.sh: no steps for '$ID' ($STEPS)" >&2; exit 2; }
ROOT="$(git rev-parse --show-toplevel 2>/dev/null || git -C "$HERE" rev-parse --show-toplevel)"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"

if [ -z "$SIZE" ]; then
  SIZE="$(python3 -c 'import json,sys
for s in json.load(open(sys.argv[1])):
    if isinstance(s, dict) and "_size" in s: print(s["_size"]); break
else: print("1280x820")' "$STEPS")"
fi
case "$SIZE" in *x*) ;; *) echo "shoot.sh: size must be WxH" >&2; exit 2 ;; esac

# --- 1. The fixture workspace, generated once per design_fixture.rs ---------
SRC="$ROOT/crates/ternarywire-app/tests/design_fixture.rs"
GEN_ROOT="$ROOT"
if [ ! -f "$SRC" ]; then
  # This worktree has no fixture test yet: use the one next to this script.
  GEN_ROOT="$(git -C "$HERE" rev-parse --show-toplevel)"
  SRC="$GEN_ROOT/crates/ternarywire-app/tests/design_fixture.rs"
fi
KEY="$(printf '%s' "$SRC" | shasum | cut -c1-12)"
CACHE="${TW_FIXTURE_CACHE:-$TMP/tw-fixture-cache/$KEY}"
mkdir -p "$(dirname "$CACHE")"

# Locks shared by every shoot.sh run: a folder holding the owner's pid; one
# whose owner is gone is taken over. Released on exit, whatever happens.
LOCKS=()
POKE_PID=""
APP_PID=""
cleanup() {
  if [ -n "$POKE_PID" ]; then kill "$POKE_PID" 2>/dev/null || true; fi
  if [ -n "$APP_PID" ]; then kill "$APP_PID" 2>/dev/null || true; fi
  for l in ${LOCKS[@]+"${LOCKS[@]}"}; do rm -rf "$l"; done
  return 0
}
trap cleanup EXIT
lock() { # name
  local dir="$TMP/tw-fixture-cache/$1.lock" waited=0 owner
  until mkdir "$dir" 2>/dev/null; do
    owner="$(cat "$dir/pid" 2>/dev/null || true)"
    if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null; then rm -rf "$dir"; continue; fi
    sleep 1; waited=$((waited + 1))
    [ $((waited % 30)) -eq 0 ] && echo "shoot.sh: waiting for lock ${1}…" >&2
  done
  echo $$ >"$dir/pid"
  LOCKS+=("$dir")
}
unlock() { # name
  local dir="$TMP/tw-fixture-cache/$1.lock" keep=() l
  for l in ${LOCKS[@]+"${LOCKS[@]}"}; do [ "$l" = "$dir" ] || keep+=("$l"); done
  LOCKS=(${keep[@]+"${keep[@]}"})
  rm -rf "$dir"
}

# The lock also covers the copy (step 3), so no run copies a half-written cache.
lock "gen-$KEY"
if [ ! -f "$CACHE/fixture.json" ] || [ "$SRC" -nt "$CACHE/fixture.json" ]; then
  echo "shoot.sh: generating the fixture into ${CACHE}…" >&2
  if ! (cd "$GEN_ROOT" && TW_FIXTURE_OUT="$CACHE" cargo test -q -p ternarywire-app --test design_fixture -- --ignored >"$TMP/tw-fixture-cache/gen-$KEY.log" 2>&1); then
    tail -30 "$TMP/tw-fixture-cache/gen-$KEY.log" >&2; echo "shoot.sh: fixture generation failed" >&2; exit 1
  fi
fi

# --- 3. A scratch copy of the workspace and config -------------------------
RUN="$(mktemp -d "$TMP/tw-shoot.XXXXXX")"
mkdir -p "$RUN/Developer" && cp -R "$CACHE/workspace" "$RUN/Developer/bookstore"
cp -R "$CACHE/config" "$RUN/config"
unlock "gen-$KEY"

# --- 2. Servers (shared; started only when not running) ---------------------
python3 "$HERE/server.py" start >/dev/null

mkdir -p "$OUTDIR"
OUTDIR="$(cd "$OUTDIR" && pwd)"
PNG="$OUTDIR/$ID-$THEME-app.png"
rm -f "$PNG"
W="${SIZE%x*}"
H="${SIZE#*x}"

# --- 4. Steps: placeholders and fixture steps ---------------------------------
python3 - "$STEPS" "$RUN" "$CACHE/fixture.json" "$W" "$H" "$THEME" "$PNG" >"$RUN/prepare.env" <<'PY'
import datetime, json, os, shlex, sys, time
steps_path, run, meta_path, w, h, theme, png = sys.argv[1:8]
meta = json.load(open(meta_path))
ws, cfg = os.path.join(run, "Developer", "bookstore"), os.path.join(run, "config", "TernaryWire")
local = os.path.join(ws, ".ternarywire")

def load(p, default):
    try:
        return json.load(open(p))
    except (OSError, ValueError):
        return default

def save(p, v):
    with open(p, "w") as f:
        json.dump(v, f, indent=2)

def merge(a, b):
    for k, v in b.items():
        if isinstance(v, dict) and isinstance(a.get(k), dict):
            merge(a[k], v)
        else:
            a[k] = v

def subst(v):
    if isinstance(v, list):
        return [subst(x) for x in v]
    if isinstance(v, dict):
        return {k: subst(x) for k, x in v.items()}
    return {"@W": float(w), "@H": float(h), "@THEME": theme, "@OUT": png, "@WORKSPACE": ws}.get(v, v) if isinstance(v, str) else v

settings = load(os.path.join(cfg, "settings.json"), {})
settings["recent_workspaces"] = [ws]
settings.setdefault("ui", {})["theme"] = theme
state = load(os.path.join(local, "state.json"), {})
env, poke, out = {}, "", []
for s in load(steps_path, []):
    if isinstance(s, dict) and len(s) == 1 and next(iter(s)).startswith("_"):
        k, v = next(iter(s.items()))
        if k == "_settings":
            merge(settings, v)
        elif k == "_current":
            for owner, vals in v.items():
                state.setdefault("current_values", {}).setdefault(owner, {}).update(vals)
        elif k == "_session" and not v:
            try:
                os.remove(os.path.join(local, "session.json"))
            except OSError:
                pass
        elif k == "_env":
            env.update(v)
        elif k == "_poke":
            poke = v
        elif k != "_size":
            sys.exit("shoot.sh: unknown fixture step %s" % k)
        continue
    out.append(subst(s))
save(os.path.join(cfg, "settings.json"), settings)
save(os.path.join(local, "state.json"), state)
save(os.path.join(run, "steps.json"), out)

# OAuth tokens: "Token active · 52 min" on every run.
sec_path = os.path.join(local, "secrets.json")
secrets = load(sec_path, {})
now = int(time.time())
for owner in meta.get("oauth_owners", []):
    key = "oauth2:" + owner
    if key in secrets:
        t = json.loads(secrets[key])
        t["expires_at"], t["obtained_at"] = now + 52 * 60 + 30, now - 8 * 60
        secrets[key] = json.dumps(t)
save(sec_path, secrets)

# History: written for the prototype's day, moved to today (same clock times).
ref = datetime.date.fromisoformat(meta.get("reference_day", "2026-11-11"))
delta = (datetime.date.today() - ref).days
hist = os.path.join(local, "history.jsonl")
if delta and os.path.exists(hist):
    lines = []
    for line in open(hist):
        if line.strip():
            e = json.loads(line)
            t = time.localtime(e["timestamp"] / 1000)
            e["timestamp"] = int(time.mktime((t.tm_year, t.tm_mon, t.tm_mday + delta, t.tm_hour, t.tm_min, t.tm_sec, 0, 0, -1)) * 1000)
            lines.append(json.dumps(e))
    open(hist, "w").write("\n".join(lines) + "\n")

for k, v in env.items():
    print("export %s=%s" % (k, shlex.quote(str(v))))
print("POKE=%s" % shlex.quote(poke))
PY
POKE=""
# shellcheck disable=SC1091
. "$RUN/prepare.env"

# Listeners on fixed ports (webhook 8787, capture 8888): one run at a time.
if grep -q '"new_webhook"' "$RUN/steps.json"; then lock port-webhook; fi
if grep -q '"new_capture"' "$RUN/steps.json"; then lock port-capture; fi
if [ -n "$POKE" ]; then
  python3 "$HERE/server.py" poke "$POKE" >"$RUN/poke.log" 2>&1 &
  POKE_PID=$!
fi

# --- 5. The off-screen test at 2 pixels per point ------------------------------
# Script jobs run in worker processes started from the current executable,
# which libtest would reject. The test binary is started through a symlink
# (macOS reports the path it was started by), and once it runs the link is
# pointed at the app binary of the same build, whose main() serves as worker.
artifact() { # test|bin cargo-args… : the app executable cargo reports
  local want="$1"; shift
  (cd "$ROOT" && cargo "$@" --message-format=json 2>>"$RUN/build.log") | python3 -c 'import json, sys
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "ternarywire-app" \
            and m["profile"]["test"] == (sys.argv[1] == "test"):
        print(m["executable"])' "$want"
}
APP_BIN="$(artifact bin build -p ternarywire-app --bin ternarywire-app | tail -1)"
TEST_BIN="$(artifact test test -p ternarywire-app --bin ternarywire-app --no-run | tail -1)"
if [ ! -x "$TEST_BIN" ] || [ ! -x "$APP_BIN" ]; then
  tail -20 "$RUN/build.log" >&2; echo "shoot.sh: build failed; log: $RUN/build.log" >&2; exit 1
fi
LINK="$RUN/ternarywire-offscreen"
ln -s "$TEST_BIN" "$LINK"
(
  cd "$ROOT/crates/ternarywire-app" &&
  # HOME is the run folder, so the workspace shows as ~/Developer/bookstore, as in the design.
  exec env HOME="$RUN" TERNARYWIRE_CONFIG_DIR="$RUN/config" TERNARYWIRE_SECRETS=file TERNARYWIRE_AUTOMATION="$RUN/steps.json" \
    TERNARYWIRE_SIZE="$SIZE" TERNARYWIRE_PPP="${TW_PPP:-2}" \
    "$LINK" offscreen --ignored --nocapture --exact automation::tests::offscreen
) >"$RUN/app.log" 2>&1 &
APP_PID=$!
for _ in $(seq 1 600); do
  grep -q "running 1 test" "$RUN/app.log" 2>/dev/null && break
  kill -0 "$APP_PID" 2>/dev/null || break
  sleep 0.05
done
ln -s "$APP_BIN" "$RUN/link.tmp" && mv -f "$RUN/link.tmp" "$LINK"
set +e
wait "$APP_PID"
STATUS=$?
set -e
APP_PID=""

# --- 6. Result -----------------------------------------------------------------
if [ $STATUS -ne 0 ] || [ ! -s "$PNG" ]; then
  grep -E "^automation: |panicked|error" "$RUN/app.log" | grep -v "no accessible name" | tail -20 >&2 || true
  echo "shoot.sh: $ID failed (exit $STATUS); log and scratch: $RUN" >&2
  exit 1
fi
grep "^automation: " "$RUN/app.log" | grep -v -e "no accessible name" -e "automation: saved" >&2 || true
[ "${KEEP:-0}" = 1 ] && echo "shoot.sh: scratch kept in $RUN" >&2 || rm -rf "$RUN"
echo "$PNG"
