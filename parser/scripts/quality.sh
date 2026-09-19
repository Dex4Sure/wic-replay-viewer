#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
mode=${1:-full}

case "$mode" in
    fast|full) ;;
    *)
        printf 'Usage: %s {fast|full}\n' "$0" >&2
        exit 2
        ;;
esac

ruff_bin=${WIC_RUFF:-}
if [[ -z "$ruff_bin" ]]; then
    if command -v ruff >/dev/null 2>&1; then
        ruff_bin=$(command -v ruff)
    elif [[ -x $HOME/.venvs/wic-analysis/bin/ruff ]]; then
        ruff_bin=$HOME/.venvs/wic-analysis/bin/ruff
    else
        printf 'Ruff is required. Install requirements-dev.txt in a Python environment.\n' >&2
        exit 1
    fi
fi

cargo_bin=${WIC_CARGO:-cargo}

cd "$repo_root"
"$ruff_bin" check .
"$ruff_bin" format --check .
"$cargo_bin" fmt --manifest-path rust_parser/Cargo.toml --check

if [[ "$mode" == full ]]; then
    "$cargo_bin" test --offline --locked --all-features \
        --manifest-path rust_parser/Cargo.toml
    "$cargo_bin" clippy --offline --locked --all-targets --all-features \
        --manifest-path rust_parser/Cargo.toml -- -D warnings
fi
