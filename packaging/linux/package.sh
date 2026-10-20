#!/usr/bin/env bash
# Package a built TextCraft binary for Linux as a .tar.gz.
#
#   packaging/linux/package.sh <version> <path-to-textcraft-binary> [arch]
#
# Writes dist/release/textcraft-<version>-linux-<arch>.tar.gz, holding one folder with the
# binary, the licence, the README, a .desktop launcher and the icon.
set -euo pipefail

version="${1:?usage: package.sh <version> <binary> [arch]}"
binary="${2:?usage: package.sh <version> <binary> [arch]}"
arch="${3:-x86_64}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dist="${DIST:-$root/dist/release}"
name="textcraft-$version-linux-$arch"
stage="$(mktemp -d)/$name"

[ -f "$binary" ] || { echo "error: $binary does not exist" >&2; exit 1; }
mkdir -p "$stage" "$dist"
install -m 755 "$binary" "$stage/textcraft"
install -m 644 "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$root/README.md" "$stage/"
install -m 644 "$root/packaging/linux/textcraft.desktop" "$stage/"
install -m 644 "$root/assets/textcraft.png" "$stage/"

tar -C "$(dirname "$stage")" -czf "$dist/$name.tar.gz" "$name"
echo "wrote $dist/$name.tar.gz"
