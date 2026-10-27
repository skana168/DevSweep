#!/usr/bin/env bash
# Builds assets/fonts/MapleMono-CN-Regular.ttf: Maple Mono CN v7.9 cut down to the
# GB2312 hanzi set plus Latin and punctuation. The UI defaults to Chinese and shows
# arbitrary project paths, so the subset keeps everyday Chinese; the full face is
# 18.6 MB, this one is ~5 MB. Run after bumping the font version, and update
# assets/fonts/OFL-MapleMono.txt with it.
set -euo pipefail
cd "$(dirname "$0")/.."

version=v7.9
zip="MapleMono-CN.zip"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/$zip" "https://github.com/subframe7536/maple-font/releases/download/$version/$zip"
unzip -q -o "$work/$zip" MapleMono-CN-Regular.ttf LICENSE.txt -d "$work"
cp "$work/LICENSE.txt" assets/fonts/OFL-MapleMono.txt

# GB2312's hanzi plus ASCII, CJK punctuation and the symbols the interface draws.
python3 - "$work/MapleMono-CN-Regular.ttf" <<'PY'
import subprocess, sys

src, out = sys.argv[1], "assets/fonts/MapleMono-CN-Regular.ttf"

def encodable(ch):
    try:
        ch.encode("gb2312")
        return True
    except UnicodeEncodeError:
        return False

ranges = [(0x20, 0x7E), (0xA0, 0xFF), (0x2000, 0x206F), (0x2190, 0x21FF),
          (0x2200, 0x22FF), (0x25A0, 0x25FF), (0x3000, 0x303F), (0xFF00, 0xFFEF)]
codepoints = {c for lo, hi in ranges for c in range(lo, hi + 1)}
codepoints |= {ord(ch) for ch in map(chr, range(0x4E00, 0xA000)) if encodable(ch)}

subprocess.run([
    "pyftsubset", src,
    "--unicodes=" + ",".join(f"U+{c:04X}" for c in sorted(codepoints)),
    "--output-file=" + out,
    "--layout-features=", "--drop-tables+=GSUB,GPOS,GDEF,BASE", "--no-hinting",
], check=True)
PY
ls -l assets/fonts/MapleMono-CN-Regular.ttf
