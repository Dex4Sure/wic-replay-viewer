#!/usr/bin/env bash
set -euo pipefail

# Cross-builds the portable Windows x64 executable from Linux. The supported
# release path remains `npm run build:windows` on a Windows host, which is what
# the artifact workflow uses; this script exists so a Linux developer can produce
# a testable build without one. Tauri's Windows bundling stays disabled either
# way, so the deliverable is a single unsigned executable.

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
target=x86_64-pc-windows-msvc
exe="$repo_root/target/$target/release/wic-replay-viewer.exe"

if ! command -v cargo-xwin >/dev/null 2>&1; then
    printf '%s\n' \
        'Required command not found: cargo-xwin (cargo install cargo-xwin)' >&2
    exit 1
fi

# clang-cl compiles the bundled SQLite C sources, llvm-lib archives them,
# lld-link is the MSVC-flavour linker, and llvm-rc compiles the resource script
# that tauri-build emits for the icon and version metadata. On Fedora these come
# from the `clang`, `llvm`, and `lld` packages.
missing=()
for tool in clang-cl llvm-lib lld-link llvm-rc; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if ((${#missing[@]} > 0)); then
    printf 'Required cross-compilation tools not found: %s\n' "${missing[*]}" >&2
    printf 'On Fedora: sudo dnf install -y clang llvm lld\n' >&2
    exit 1
fi

if ! rustup target list --installed | grep -qx "$target"; then
    printf 'Rust target not installed: %s\n' "$target" >&2
    printf 'Install it with: rustup target add %s\n' "$target" >&2
    exit 1
fi

npm run build

# `custom-protocol` is what makes Tauri embed the built frontend. Without it the
# executable starts against the development server URL and shows an empty window.
cargo xwin build \
    --release \
    --target "$target" \
    --package wic-replay-viewer-app \
    --bin wic-replay-viewer \
    --features custom-protocol

printf 'Created %s\n' "$exe"
