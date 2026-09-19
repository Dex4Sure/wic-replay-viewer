#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

# `all` keeps the original behaviour for local runs. The split lets CI run the
# frontend half on a plain runner and the Rust half, which needs the WebKit
# development packages and the Cargo target cache, only when Rust changed.
mode=${1:-all}
case "$mode" in
    all | frontend | rust) ;;
    *)
        printf 'Usage: %s {all|frontend|rust}\n' "$0" >&2
        exit 2
        ;;
esac

if [[ "$mode" != rust ]]; then
    npm run test:coverage
fi

if [[ "$mode" == frontend ]]; then
    exit 0
fi

expected=0.9.0
installed=$(cargo llvm-cov --version 2>/dev/null | sed -nE 's/^cargo-llvm-cov ([0-9.]+).*/\1/p')
if [[ "$installed" != "$expected" ]]; then
    printf 'cargo-llvm-cov %s is required (found %s).\n' "$expected" "${installed:-not installed}" >&2
    printf 'Install it with: cargo install cargo-llvm-cov --version %s --locked\n' "$expected" >&2
    exit 1
fi

coverage_report=$(mktemp "${TMPDIR:-/tmp}/wic-replay-rust-coverage.XXXXXX.json")
trap 'rm -f "$coverage_report"' EXIT

CCACHE_DISABLE=1 cargo llvm-cov --locked --workspace --all-targets --all-features \
    --no-clean --quiet --json --output-path "$coverage_report"
node scripts/rust-coverage.mjs "$coverage_report"
