#!/usr/bin/env bash
# Builds target/DevSweep.app and target/DevSweep_<version>_macos.dmg.
# Unsigned on purpose: see the Gatekeeper notes in README.md.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -1)
app=target/DevSweep.app
stage=target/dmg
dmg="target/DevSweep_${version}_macos.dmg"

cargo build --release

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/devsweep "$app/Contents/MacOS/DevSweep"
cp assets/icon.icns "$app/Contents/Resources/DevSweep.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>DevSweep</string>
    <key>CFBundleDisplayName</key><string>DevSweep</string>
    <key>CFBundleIdentifier</key><string>dev.devsweep.app</string>
    <key>CFBundleExecutable</key><string>DevSweep</string>
    <key>CFBundleIconFile</key><string>DevSweep</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${version}</string>
    <key>CFBundleVersion</key><string>${version}</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

rm -rf "$stage" "$dmg"
mkdir -p "$stage"
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -quiet -volname DevSweep -srcfolder "$stage" -ov -format UDZO "$dmg"

echo "$dmg"
