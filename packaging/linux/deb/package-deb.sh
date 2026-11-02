#!/usr/bin/env bash
# Package a built TextCraft binary for Linux as a .deb package.
#
#   packaging/linux/deb/package-deb.sh <version> <path-to-textcraft-binary> [arch]
#
# Writes dist/release/textcraft_<version>_<arch>.deb
set -euo pipefail

version="${1:?usage: package-deb.sh <version> <binary> [arch]}"
binary="${2:?usage: package-deb.sh <version> <binary> [arch]}"
raw_arch="${3:-amd64}"

# Normalize architecture to Debian naming convention
case "$raw_arch" in
  x86_64) deb_arch="amd64" ;;
  aarch64) deb_arch="arm64" ;;
  *) deb_arch="$raw_arch" ;;
esac

command -v dpkg-deb >/dev/null 2>&1 || {
  echo "error: dpkg-deb is required to build .deb packages" >&2
  exit 1
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
dist="${DIST:-$root/dist/release}"
stage="$(mktemp -d)/textcraft-$version"
trap 'rm -rf "$(dirname "$stage")"' EXIT

[ -f "$binary" ] || { echo "error: $binary does not exist" >&2; exit 1; }

mkdir -p "$stage/DEBIAN"
mkdir -p "$stage/usr/bin"
mkdir -p "$stage/usr/share/applications"
mkdir -p "$stage/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$stage/usr/share/metainfo"
mkdir -p "$stage/usr/share/doc/textcraft"
mkdir -p "$dist"

# Install executable
install -m 755 "$binary" "$stage/usr/bin/textcraft"

# Install desktop entries
install -m 644 "$root/packaging/linux/textcraft.desktop" "$stage/usr/share/applications/"
install -m 644 "$root/packaging/linux/com.github.rsxdalv.TextCraft.desktop" "$stage/usr/share/applications/"

# Install icons (both textcraft and app-id names for full compatibility)
install -m 644 "$root/assets/textcraft.png" "$stage/usr/share/icons/hicolor/256x256/apps/textcraft.png"
install -m 644 "$root/assets/textcraft.png" "$stage/usr/share/icons/hicolor/256x256/apps/com.github.rsxdalv.TextCraft.png"

# Install AppStream metainfo
install -m 644 "$root/packaging/linux/com.github.rsxdalv.TextCraft.metainfo.xml" "$stage/usr/share/metainfo/"

# Install documentation & copyright
install -m 644 "$root/packaging/linux/deb/debian/copyright" "$stage/usr/share/doc/textcraft/copyright"
install -m 644 "$root/README.md" "$stage/usr/share/doc/textcraft/README.md"

# Calculate installed size in kilobytes
installed_size="$(du -ks "$stage/usr" | cut -f1)"

# Generate control file
cat <<EOF > "$stage/DEBIAN/control"
Package: textcraft
Version: ${version}
Section: editors
Priority: optional
Architecture: ${deb_arch}
Maintainer: TextCraft contributors <https://github.com/rsxdalv/TextCraft>
Installed-Size: ${installed_size}
Depends: libc6 (>= 2.34), libgcc-s1
Recommends: libvulkan1 | libgl1, libwayland-client0, libx11-6, libxcursor1, libxkbcommon0, xdg-desktop-portal
Homepage: https://github.com/rsxdalv/TextCraft
Description: Fast, modern text and Markdown editor
 TextCraft is a lightweight text and Markdown editor written in Rust with
 egui and wgpu, supporting atomic file saves, Markdown formatting, tabs,
 and session restore.
EOF

deb_file="$dist/textcraft_${version}_${deb_arch}.deb"
dpkg-deb --build --root-owner-group "$stage" "$deb_file"
echo "wrote $deb_file"
