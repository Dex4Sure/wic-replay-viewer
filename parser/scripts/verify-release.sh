#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
corpus_root=${WIC_REPLAY_ROOT:-}

usage() {
    printf 'Usage: %s [--corpus-root PATH]\n' "$0"
}

while (($#)); do
    case "$1" in
        --corpus-root)
            if (($# < 2)); then
                printf '%s requires a path.\n' "$1" >&2
                usage >&2
                exit 2
            fi
            corpus_root=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

cargo_bin=${WIC_CARGO:-cargo}
python_bin=${WIC_PYTHON:-python3}
wasm_pack_cache=${WIC_WASM_PACK_CACHE:-$repo_root/state/wasm-pack}

cd "$repo_root"
./scripts/quality.sh full
"$cargo_bin" build --release --offline --locked --all-features \
    --manifest-path rust_parser/Cargo.toml

if ! command -v wasm-pack >/dev/null 2>&1; then
    printf 'wasm-pack is required for release verification.\n' >&2
    exit 1
fi
mkdir -p "$wasm_pack_cache"
WASM_PACK_CACHE=$wasm_pack_cache \
    wasm-pack build rust_parser --release -- --locked --offline

if [[ -n "$corpus_root" ]]; then
    WIC_REPLAY_ROOT=$corpus_root WIC_REQUIRE_ALL_GROUND_TRUTH=1 \
        "$python_bin" test_ground_truth.py
else
    printf '%s\n' \
        'Ground truth not run: pass --corpus-root PATH or set WIC_REPLAY_ROOT.'
fi
