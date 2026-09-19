#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
    printf 'The release AppImage must be built on x86_64 Linux.\n' >&2
    exit 1
fi

for command in node objcopy; do
    command -v "$command" >/dev/null || { printf 'Required command not found: %s\n' "$command" >&2; exit 1; }
done

# Keep the artifact paths deterministic even when the caller uses a Cargo override.
export CARGO_TARGET_DIR="$repo_root/target"
# Allow linuxdeploy and its plugins to run on CI without a FUSE mount.
export APPIMAGE_EXTRACT_AND_RUN=1
bundle="$repo_root/artifacts/WiCReplayViewer-linux-x86_64.AppImage"
mkdir -p "$repo_root/artifacts/diagnostic-symbols/linux"
rm -f -- "$bundle"

node scripts/tauri.mjs build --ci --no-bundle -- --locked
objcopy --only-keep-debug target/release/wic-replay-viewer \
    artifacts/diagnostic-symbols/linux/wic-replay-viewer.debug

# A failed bundle must never leave an older AppImage looking like the new output.
shopt -s nullglob
old_images=(target/release/bundle/appimage/*.AppImage)
if ((${#old_images[@]})); then
    rm -f -- "${old_images[@]}"
fi
node scripts/tauri.mjs bundle --ci --bundles appimage
images=(target/release/bundle/appimage/*.AppImage)
if ((${#images[@]} != 1)); then
    printf 'Expected exactly one AppImage; found %s.\n' "${#images[@]}" >&2
    exit 1
fi
app_dirs=(target/release/bundle/appimage/*.AppDir)
if ((${#app_dirs[@]} != 1)); then
    printf 'Expected exactly one AppDir; found %s.\n' "${#app_dirs[@]}" >&2
    exit 1
fi
./scripts/package-appimage.sh "${app_dirs[0]}" "$bundle"
printf 'Created %s\n' "$bundle"
