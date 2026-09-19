#!/usr/bin/env bash
set -euo pipefail

if (($# != 2)); then
    printf 'Usage: %s APPDIR OUTPUT\n' "$0" >&2
    exit 1
fi
app_dir=$(realpath -- "$1")
output=$(realpath -m -- "$2")
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
packager="${XDG_CACHE_HOME:-$HOME/.cache}/tauri/linuxdeploy-plugin-appimage.AppImage"
if [[ ! -x "$app_dir/AppRun" || ! -x "$packager" ]]; then
    printf 'AppDir or Tauri AppImage output tool is missing; run build:appimage first.\n' >&2
    exit 1
fi

# Mesa must use the host Wayland client library. Bundling Ubuntu's older copy
# causes WebKit to abort with EGL_BAD_PARAMETER on Fedora. This library also
# appears on AppImage's upstream exclusion list:
# https://github.com/AppImage/pkg2appimage/blob/master/excludelist
rm -f -- "$app_dir"/usr/lib/libwayland-client.so*
# Wrap the generated launcher once; repackaging an AppDir is idempotent.
if [[ ! -e "$app_dir/AppRun.tauri" ]]; then
    mv -- "$app_dir/AppRun" "$app_dir/AppRun.tauri"
fi
install -m755 -- "$repo_root/packaging/appimage/AppRun" "$app_dir/AppRun"
rm -f -- "$output"
temporary="${output}.tmp.AppImage"
trap 'rm -f -- "$temporary"' EXIT
ARCH=x86_64 OUTPUT="$temporary" APPIMAGE_EXTRACT_AND_RUN=1 \
    "$packager" --appdir "$app_dir"
test -s "$temporary"
chmod +x "$temporary"
mv -f -- "$temporary" "$output"
