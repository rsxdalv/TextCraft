#!/usr/bin/env bash
# Package a built TextCraft binary for macOS as TextCraft.app inside a .zip.
#
#   packaging/macos/package.sh <version> <path-to-binary> <arch-label>
#
# Writes dist/release/textcraft-<version>-macos-<arch-label>.zip. The app is signed ad hoc
# (no Apple developer identity): macOS runs it after the user approves it once in
# System Settings > Privacy & Security, or with `xattr -dr com.apple.quarantine TextCraft.app`.
set -euo pipefail

version="${1:?usage: package.sh <version> <binary> <arch-label>}"
binary="${2:?usage: package.sh <version> <binary> <arch-label>}"
label="${3:?usage: package.sh <version> <binary> <arch-label>}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dist="${DIST:-$root/dist/release}"
work="$(mktemp -d)"
app="$work/TextCraft.app"
# MacOS bundle versions are numeric; a pre-release suffix is dropped.
short_version="${version%%-*}"

[ -f "$binary" ] || { echo "error: $binary does not exist" >&2; exit 1; }
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$dist"
install -m 755 "$binary" "$app/Contents/MacOS/textcraft"
install -m 644 "$root/LICENSE" "$app/Contents/Resources/LICENSE"
install -m 644 "$root/THIRD_PARTY_NOTICES.md" "$app/Contents/Resources/THIRD_PARTY_NOTICES.md"

# The icon is a nicety: if building the .icns fails, ship without it rather than fail the release.
if iconset="$work/textcraft.iconset" && mkdir -p "$iconset" \
  && for size in 16 32 64 128 256; do
       sips -z "$size" "$size" "$root/assets/textcraft.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
     done \
  && cp "$iconset/icon_32x32.png" "$iconset/icon_16x16@2x.png" \
  && cp "$iconset/icon_64x64.png" "$iconset/icon_32x32@2x.png" \
  && cp "$iconset/icon_256x256.png" "$iconset/icon_128x128@2x.png" \
  && rm "$iconset/icon_64x64.png" \
  && iconutil -c icns "$iconset" -o "$app/Contents/Resources/textcraft.icns"; then
  icon_key='<key>CFBundleIconFile</key><string>textcraft</string>'
else
  echo "warning: could not build textcraft.icns; the app will have the default icon" >&2
  icon_key=''
fi

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>TextCraft</string>
  <key>CFBundleDisplayName</key><string>TextCraft</string>
  <key>CFBundleIdentifier</key><string>io.github.textcraft.TextCraft</string>
  <key>CFBundleExecutable</key><string>textcraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$short_version</string>
  <key>CFBundleVersion</key><string>$short_version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  $icon_key
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Text document</string>
      <key>CFBundleTypeRole</key><string>Editor</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.plain-text</string><string>net.daringfireball.markdown</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist"

codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"

zip_path="$dist/textcraft-$version-macos-$label.zip"
rm -f "$zip_path"
# ditto keeps the bundle's permissions and metadata, which a plain zip would not.
ditto -c -k --keepParent "$app" "$zip_path"
echo "wrote $zip_path"
