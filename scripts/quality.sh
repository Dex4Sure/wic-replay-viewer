#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
mode=${1:-full}

# `fast` and `full` are the local hook gates and are unchanged: pre-commit runs
# fast, pre-push runs full, and full remains the complete gate.
#
# `ci-portable` and `ci-rust` are the lighter hosted-CI gate. They run as
# parallel jobs and the Rust half can be skipped when no Rust changed. CI keeps
# the tests and Clippy, while the local pre-push `full` gate owns the expensive
# LLVM coverage measurement; scripts/quality-split.node.mjs enforces that this
# is the only difference.
case "$mode" in
    fast | full | ci-portable | ci-rust) ;;
    private)
        exec "$repo_root/scripts/private-quality.sh"
        ;;
    corpus)
        exec "$repo_root/scripts/corpus-quality.sh"
        ;;
    *)
        printf 'Usage: %s {fast|full|ci-portable|ci-rust|private|corpus}\n' "$0" >&2
        exit 2
        ;;
esac

# Stage selection. `static` is the cheap lint and consistency layer, `portable`
# is everything that needs no WebKit toolchain, and `rust` is clippy plus Rust
# coverage, which is the only part needing the system WebKit development
# packages and the large Cargo target cache.
run_static=true
run_portable=false
run_rust=false
run_rust_coverage=false
fetch_crates=false
install_node_modules=false
build_frontend=false

case "$mode" in
    fast) ;;
    full)
        run_portable=true
        run_rust=true
        run_rust_coverage=true
        fetch_crates=true
        install_node_modules=true
        build_frontend=true
        ;;
    ci-portable)
        run_portable=true
        fetch_crates=true
        install_node_modules=true
        build_frontend=true
        ;;
    ci-rust)
        # Clippy and tests run with --all-features, which enables Tauri's
        # custom-protocol and therefore embeds ../dist at compile time. The
        # frontend must exist before either runs.
        run_static=false
        run_rust=true
        fetch_crates=true
        install_node_modules=true
        build_frontend=true
        ;;
esac

# Report the resolved stages without running them, so the split can be verified
# against the real logic rather than a copy of it.
if [[ -n "${WIC_QUALITY_PRINT_PLAN:-}" ]]; then
    printf 'static=%s portable=%s rust=%s rust_coverage=%s fetch=%s node_modules=%s frontend=%s\n' \
        "$run_static" "$run_portable" "$run_rust" "$run_rust_coverage" \
        "$fetch_crates" "$install_node_modules" "$build_frontend"
    exit 0
fi

cd "$repo_root"

# macOS exposes temporary storage through /var -> /private/var. Import paths are
# canonical, so fixtures must start with the same physical temporary root.
TMPDIR=$(cd -- "${TMPDIR:-/tmp}" && pwd -P)
export TMPDIR

if [[ "$install_node_modules" == true ]]; then
    npm ci --no-audit --no-fund
fi

if [[ "$run_static" == true ]]; then
    parser_schema=$(
        sed -nE 's|const TIMELINE_SCHEMA_VERSION: u32 = ([0-9]+);|\1|p' \
            parser/rust_parser/src/parser.rs
    )
    cache_schema=$(sed -nE 's|.*timeline-v([0-9]+)/.*|\1|p' src/model.rs)

    if ! cmp -s -- AGENTS.md CLAUDE.md; then
        printf 'CLAUDE.md differs from AGENTS.md.\n' >&2
        exit 1
    fi
    if [[ "$cache_schema" != "$parser_schema" ]]; then
        printf 'PARSER_CACHE_KEY schema %s does not match parser schema %s.\n' \
            "${cache_schema:-missing}" "${parser_schema:-missing}" >&2
        exit 1
    fi

    npm run version:check
    ./parser/scripts/quality.sh fast
    ./research/scripts/quality.sh fast
    npm run format:check
    npm run lint
    cargo fmt --all -- --check
fi

if [[ "$fetch_crates" == true ]]; then
    cargo fetch --locked --quiet
fi

if [[ "$run_portable" == true ]]; then
    python_bin=${WIC_PYTHON:-python3}
    ./research/scripts/quality.sh full
    "$python_bin" -m unittest discover -s parser -p 'test_*.py'
    node --test scripts/tauri-environment.node.mjs
    node --test scripts/packaging.node.mjs
    node --test scripts/quality-split.node.mjs
    node --test scripts/rust-coverage.node.mjs
    node --test scripts/version.node.mjs
    npm run licenses:check
fi

if [[ "$build_frontend" == true ]]; then
    npm run build
fi

if [[ "$run_portable" == true ]]; then
    ./scripts/coverage.sh frontend
fi

if [[ "$run_rust" == true ]]; then
    cargo clippy --quiet --workspace --locked --all-targets --all-features -- -D warnings
    if [[ "$run_rust_coverage" == true ]]; then
        ./scripts/coverage.sh rust
    else
        cargo test --quiet --workspace --locked --all-targets --all-features
    fi
fi

if [[ "$mode" == fast ]]; then
    npm run typecheck
fi
