#!/usr/bin/env bash
# Renders assets/icon.svg into assets/icon.icns and assets/icon.ico.
# Needs rsvg-convert (brew install librsvg) and Pillow. The rendered icons are
# committed, so builds and CI never have to render the SVG themselves.
set -euo pipefail
cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rsvg-convert -w 1024 -h 1024 assets/icon.svg -o "$work/icon.png"

mkdir "$work/DevSweep.iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$work/icon.png" --out "$work/DevSweep.iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$((size * 2))" "$((size * 2))" "$work/icon.png" --out "$work/DevSweep.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$work/DevSweep.iconset" -o assets/icon.icns

python3 - "$work/icon.png" <<'PY'
import sys
from PIL import Image

Image.open(sys.argv[1]).save(
    "assets/icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
)
PY
