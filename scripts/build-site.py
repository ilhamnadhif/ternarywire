#!/usr/bin/env python3
"""Builds the generated parts of the website (site/, published by .github/workflows/pages.yml).

    python3 -m venv /tmp/ft && /tmp/ft/bin/pip install fonttools brotli pillow
    /tmp/ft/bin/python scripts/build-site.py [--phosphor DIR] [--shots DIR]

It writes
  site/demo/          the redesign prototype (design/redesign) for the live window in the hero:
                      its scripts and styles as they are, except app.js (site/demo/runtime.js
                      takes its place) and the icon path; plus fonts.css
  site/assets/fonts/  Inter and JetBrains Mono (from the app's fonts) cut down to Latin and the
                      symbols the page uses, the page's Phosphor icons, and the app's icon font
                      for the demo; all WOFF2
  site/assets/icons.js  the page's icon names -> code points (every data-i="name" in site/)
  site/assets/img/shots/<id>.webp   with --shots: the app's screenshots, from the PNGs that
                      design/redesign/fixture/shoot.sh <id> dark 1440x900 DIR writes
                      (<id>-dark-app.png), with the macOS window buttons drawn in

--phosphor  folder with Phosphor.ttf, Phosphor-Fill.ttf and codepoints.rs (the egui-phosphor
            crate's res/ and src/variants/); found in ~/.cargo/registry when left out.
"""

import argparse
import glob
import os
import re
import shutil
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = os.path.join(ROOT, "site")
PROTO = os.path.join(ROOT, "design", "redesign")
APP_FONTS = os.path.join(ROOT, "crates", "ternarywire-app", "assets", "fonts")
FONTS_OUT = os.path.join(SITE, "assets", "fonts")

# The prototype's files the demo loads (index.html lists them); app.js is replaced by runtime.js.
DEMO_FILES = ["icons.js", "data.js", "kit.js", "shell.js", "screens-http.js", "screens-proto.js",
              "screens-work.js", "screens-flow.js", "screens-tools.js", "dialogs.js", "styles.css"]
APP_ICON = "../../crates/ternarywire-app/assets/icon-256.png"
# Changes to the prototype for the website: (file, text, replacement). The status bar's "Update"
# pill names a version older than the one the page offers, so the demo leaves it out.
DEMO_EDITS = [
    ("shell.js", '<button class="sbtn" data-dlg="update" style="color:var(--accent-text)">${I("ARROW_LINE_DOWN")}Update 0.3.0</button>', ""),
]

TEXT = ",".join([
    "U+0000-017F",  # Basic Latin, Latin-1, Latin Extended-A
    "U+2000-206F",  # General punctuation
    "U+20AC", "U+2122",
    "U+2190-21FF",  # Arrows
    "U+2200-22FF",  # Math operators
    "U+2300-23FF",  # Misc technical (⌘ ⌥ ⏎ ...)
    "U+2500-257F",  # Box drawing (the workspace tree)
    "U+25A0-25FF",  # Geometric shapes
    "U+2700-27BF",  # Dingbats (✓ ✗)
    "U+FFFD",
])

# The app screenshots the page shows (site/assets/img/shots/<id>.webp).
SHOTS = ["http", "http-filter", "http-compare", "dlg-set-var", "dlg-gen-tests", "grpc", "ws", "sio",
         "mqtt", "mcp", "ai", "http-graphql", "runner", "flow"]


def woff2(src, dst, unicodes=None):
    """`src` as WOFF2, cut down to `unicodes` (None: every glyph)."""
    font = TTFont(src)
    if unicodes is not None:
        opts = subset.Options()
        opts.flavor = "woff2"
        opts.layout_features = ["*"]
        opts.name_IDs = ["*"]
        opts.notdef_outline = True
        sub = subset.Subsetter(opts)
        sub.populate(unicodes=unicodes)
        sub.subset(font)
    font.flavor = "woff2"
    font.save(dst)
    return os.path.getsize(dst)


def ranges(spec):
    out = []
    for part in spec.split(","):
        part = part.strip().replace("U+", "")
        if "-" in part:
            a, b = part.split("-")
            out.extend(range(int(a, 16), int(b, 16) + 1))
        else:
            out.append(int(part, 16))
    return out


def find_phosphor(arg):
    if arg:
        return arg, arg
    hits = sorted(glob.glob(os.path.expanduser("~/.cargo/registry/src/*/egui-phosphor-*/")))
    if not hits:
        sys.exit("build-site.py: egui-phosphor not found in ~/.cargo/registry; pass --phosphor")
    base = hits[-1]
    return os.path.join(base, "res"), os.path.join(base, "src", "variants")


def site_icon_names():
    names = set()
    for path in glob.glob(os.path.join(SITE, "*.html")) + glob.glob(os.path.join(SITE, "assets", "*.js")):
        if path.endswith("icons.js"):
            continue
        with open(path, encoding="utf-8") as f:
            names.update(re.findall(r"""data-i=\\?["']([a-z0-9-]+)""", f.read()))
    return names


def build_demo():
    out = os.path.join(SITE, "demo")
    os.makedirs(out, exist_ok=True)
    for name in DEMO_FILES:
        with open(os.path.join(PROTO, name), encoding="utf-8") as f:
            text = f.read()
        text = text.replace(APP_ICON, "../assets/img/app-icon-128.png")
        for f_name, old, new in DEMO_EDITS:
            if f_name == name:
                if old not in text:
                    sys.exit(f"build-site.py: the prototype changed; update DEMO_EDITS for {name}")
                text = text.replace(old, new)
        with open(os.path.join(out, name), "w", encoding="utf-8") as f:
            f.write(text)
    with open(os.path.join(out, "fonts.css"), "w", encoding="utf-8") as f:
        f.write("/* Generated by scripts/build-site.py. The app's fonts, shared with the page. */\n")
        for w in (400, 500, 600):
            f.write(f'@font-face {{ font-family: "Inter"; font-weight: {w}; font-display: block; src: url(../assets/fonts/inter-{w}.woff2) format("woff2"); }}\n')
        f.write('@font-face { font-family: "JBMono"; font-display: block; src: url(../assets/fonts/jbm-400.woff2) format("woff2"); }\n')
        f.write('@font-face { font-family: "Phosphor"; font-display: block; src: url(../assets/fonts/app-icons.woff2) format("woff2"); }\n')
    print("demo:", len(DEMO_FILES), "files")


def build_fonts(phosphor_res, phosphor_cp):
    os.makedirs(FONTS_OUT, exist_ok=True)
    text = ranges(TEXT)
    for w, name in ((400, "Regular"), (500, "Medium"), (600, "SemiBold")):
        n = woff2(os.path.join(APP_FONTS, f"Inter-{name}.ttf"), os.path.join(FONTS_OUT, f"inter-{w}.woff2"), text)
        print(f"inter-{w}.woff2", n)
    n = woff2(os.path.join(APP_FONTS, "JetBrainsMono-Regular.ttf"), os.path.join(FONTS_OUT, "jbm-400.woff2"), text)
    print("jbm-400.woff2", n)
    # the app's own icon font, whole (it is already cut down to the app's icons)
    n = woff2(os.path.join(APP_FONTS, "Phosphor.ttf"), os.path.join(FONTS_OUT, "app-icons.woff2"))
    print("app-icons.woff2", n)

    cps = {}
    with open(os.path.join(phosphor_cp, "codepoints.rs"), encoding="utf-8") as f:
        for m in re.finditer(r'pub const ([A-Z0-9_]+): &str = "\\u\{([0-9A-Fa-f]+)\}";', f.read()):
            cps[m.group(1).lower().replace("_", "-")] = int(m.group(2), 16)
    names = sorted(site_icon_names())
    missing = [n for n in names if n not in cps]
    if missing:
        sys.exit(f"build-site.py: unknown Phosphor icons: {', '.join(missing)}")
    used = {n: cps[n] for n in names}
    for src, dst in (("Phosphor.ttf", "phosphor.woff2"), ("Phosphor-Fill.ttf", "phosphor-fill.woff2")):
        n = woff2(os.path.join(phosphor_res, src), os.path.join(FONTS_OUT, dst), sorted(used.values()))
        print(dst, n)
    with open(os.path.join(SITE, "assets", "icons.js"), "w", encoding="utf-8") as f:
        f.write("/* Generated by scripts/build-site.py: the Phosphor icons this page uses (MIT, phosphoricons.com). */\n")
        f.write("window.PH={" + ",".join(f'"{k}":{v}' for k, v in used.items()) + "};\n")
    print("icons:", len(used))
    for lic in ("LICENSE-Inter.txt", "LICENSE-JetBrainsMono.txt", "LICENSE-Phosphor.txt"):
        shutil.copy(os.path.join(APP_FONTS, lic), os.path.join(FONTS_OUT, lic))


def build_shots(src_dir):
    from PIL import Image, ImageDraw
    out = os.path.join(SITE, "assets", "img", "shots")
    os.makedirs(out, exist_ok=True)
    for sid in SHOTS:
        src = os.path.join(src_dir, f"{sid}-dark-app.png")
        if not os.path.exists(src):
            print("shot missing:", sid)
            continue
        im = Image.open(src).convert("RGB")
        k = im.width / 1440  # 2x pixels of a 1440x900 window
        # the window buttons macOS draws over the title bar (the app leaves room for them)
        d = ImageDraw.Draw(im)
        for i, (fill, ring) in enumerate((("#ff5f57", "#e0443e"), ("#febc2e", "#dea123"), ("#28c840", "#1aab29"))):
            cx, cy, r = (20 + 6 + i * 20) * k, 22 * k, 6 * k
            d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=fill, outline=ring, width=max(1, round(.5 * k)))
        im = im.resize((1800, round(1800 * im.height / im.width)), Image.LANCZOS)
        dst = os.path.join(out, f"{sid}.webp")
        im.save(dst, "WEBP", quality=82, method=6)
        print(f"shots/{sid}.webp", os.path.getsize(dst))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--phosphor")
    ap.add_argument("--shots")
    a = ap.parse_args()
    build_demo()
    build_fonts(*find_phosphor(a.phosphor))
    if a.shots:
        build_shots(a.shots)


if __name__ == "__main__":
    main()
