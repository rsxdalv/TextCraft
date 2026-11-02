#!/usr/bin/env bash
# Build TextCraft Flatpak locally or export as a standalone .flatpak bundle.
#
#   packaging/linux/flatpak/build-flatpak.sh [--bundle] [arch]
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
manifest="$root/packaging/linux/flatpak/com.github.rsxdalv.TextCraft.yaml"
build_dir="$root/target/flatpak-build"
repo_dir="$root/target/flatpak-repo"
dist="${DIST:-$root/dist/release}"
app_id="com.github.rsxdalv.TextCraft"

bundle=false
arch="$(uname -m)"

for arg in "$@"; do
  case "$arg" in
    --bundle) bundle=true ;;
    *) arch="$arg" ;;
  esac
done

command -v flatpak-builder >/dev/null 2>&1 || {
  echo "error: flatpak-builder is required" >&2
  exit 1
}

echo "Building Flatpak for $app_id ($arch)..."
flatpak-builder --force-clean --arch="$arch" --repo="$repo_dir" "$build_dir" "$manifest"

if [ "$bundle" = true ]; then
  mkdir -p "$dist"
  bundle_file="$dist/$app_id-$arch.flatpak"
  echo "Creating bundle $bundle_file..."
  flatpak build-bundle "$repo_dir" "$bundle_file" "$app_id"
  echo "Wrote $bundle_file"
fi
