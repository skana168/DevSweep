#!/usr/bin/env bash
# Renders assets/icon.svg with platform-specific margins into PNG, ICNS and ICO.
# Needs rsvg-convert (brew install librsvg) and Pillow. The rendered icons are
# committed, so builds and CI never have to render the SVG themselves.
set -euo pipefail
cd "$(dirname "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

python3 - "$work" <<'PY'
from pathlib import Path
import subprocess
import sys
import xml.etree.ElementTree as ET
from PIL import Image

work = Path(sys.argv[1])
ET.register_namespace("", "http://www.w3.org/2000/svg")
source = Path("assets/icon.svg").read_text()
# Match referto's generic mark extent and its separate macOS 824/1024 grid.
generic_ratio = 116.34 / 136.34
mac_ratio = 824 / 1024
for name, ratio in [("generic", generic_ratio), ("mac", mac_ratio)]:
    svg = ET.fromstring(source)
    tile = svg.find(".//*[@id='background']")
    width = float(tile.attrib["width"]) / ratio
    center_x = float(tile.attrib["x"]) + float(tile.attrib["width"]) / 2
    center_y = float(tile.attrib["y"]) + float(tile.attrib["height"]) / 2
    svg.set("viewBox", f"{center_x - width / 2} {center_y - width / 2} {width} {width}")
    ET.ElementTree(svg).write(work / f"{name}.svg", encoding="unicode")

def render(name, size, destination, ratio):
    subprocess.run([
        "rsvg-convert", "-w", str(size), "-h", str(size),
        str(work / f"{name}.svg"), "-o", str(destination),
    ], check=True)
    image = Image.open(destination).convert("RGBA")
    assert image.size == (size, size), destination
    alpha = image.getchannel("A")
    left, top, right, bottom = alpha.point(lambda a: 255 if a >= 128 else 0).getbbox()
    assert 0 < left < right < size and 0 < top < bottom < size, destination
    assert abs((right - left) - size * ratio) <= 1, destination
    assert abs((bottom - top) - size * ratio) <= 1, destination
    assert abs((left + right) / 2 - size / 2) <= 0.5, destination
    assert abs((top + bottom) / 2 - size / 2) <= 0.5, destination
    assert all(alpha.getpixel(p) == 0 for p in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)]), destination
    return image

render("generic", 256, work / "icon.png", generic_ratio)
mac = render("mac", 1024, work / "icon-macos.png", mac_ratio)
assert mac.getchannel("A").point(lambda a: 255 if a >= 128 else 0).getbbox() == (100, 100, 924, 924)

sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
frames = [render("generic", size, work / f"ico-{size}.png", generic_ratio) for size in sizes]
frames[-1].save(
    work / "icon.ico", sizes=[(s, s) for s in sizes], append_images=frames[:-1],
)
ico = Image.open(work / "icon.ico")
assert ico.ico.sizes() == {(s, s) for s in sizes}
for frame in frames:
    assert ico.ico.getimage(frame.size).convert("RGBA").tobytes() == frame.tobytes()

iconset = work / "DevSweep.iconset"
iconset.mkdir()
for size in [16, 32, 128, 256, 512]:
    render("mac", size, iconset / f"icon_{size}x{size}.png", mac_ratio)
    render("mac", size * 2, iconset / f"icon_{size}x{size}@2x.png", mac_ratio)
PY

iconutil -c icns "$work/DevSweep.iconset" -o "$work/icon.icns"
cp "$work/icon.png" assets/icon.png
cp "$work/icon-macos.png" assets/icon-macos.png
cp "$work/icon.ico" assets/icon.ico
cp "$work/icon.icns" assets/icon.icns
