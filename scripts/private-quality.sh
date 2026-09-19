#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
workspace_root="$repo_root/local"
cd "$repo_root"

for env_file in "$repo_root/.env"; do
    if [[ -f "$env_file" ]]; then
        set -a
        # shellcheck disable=SC1090
        . "$env_file"
        set +a
    fi
done

manifest=${WIC_PRIVATE_FIXTURES_FILE:-$repo_root/tests/private-fixtures.json}
[[ -f "$manifest" ]] || { printf 'Missing private fixture manifest: %s.\n' "$manifest" >&2; exit 1; }
[[ -f "${WIC_GROUND_TRUTH_FILE:-$repo_root/parser/ground_truth.json}" ]] || {
    printf 'Missing private parser ground truth.\n' >&2
    exit 1
}

: "${WIC_GAME_INSTALL:?private gate requires WIC_GAME_INSTALL in .env}"
: "${WIC_CUSTOM_MAPS:?private gate requires WIC_CUSTOM_MAPS in .env}"
[[ -d "$WIC_GAME_INSTALL" ]] || { printf 'WIC_GAME_INSTALL is not a directory.\n' >&2; exit 1; }
[[ -d "$WIC_CUSTOM_MAPS" ]] || { printf 'WIC_CUSTOM_MAPS is not a directory.\n' >&2; exit 1; }

export WIC_REPLAY_ROOT="$workspace_root/replays/main"
export WIC_REPLAY_VIEWER_CORPUS="$workspace_root/replays/main"
while IFS=$'\t' read -r variable relative; do
    absolute="$workspace_root/$relative"
    [[ -f "$absolute" ]] || { printf 'Private fixture missing for %s: %s\n' "$variable" "$relative" >&2; exit 1; }
    printf -v "$variable" '%s' "$absolute"
    export "$variable"
done < <(jq -r '.fixtures | to_entries[] | [.key, .value] | @tsv' "$manifest")

printf 'Portable quality gate\n'
"$repo_root/scripts/quality.sh" full

printf 'Ignored private Rust regressions\n'
cargo test --locked --all-features --package wic-replay-viewer --lib \
    configured_ -- --ignored
cargo test --locked --all-features --package wic-replay-viewer --lib \
    playback::tests::playback_real_replay -- --ignored --exact
cargo test --locked --all-features --package wic-replay-viewer \
    --test real_install -- --ignored

printf 'Strict 17-fixture parser ground truth\n'
cargo build --release --locked --all-features --manifest-path parser/rust_parser/Cargo.toml
WIC_RUST_PARSER="$repo_root/target/release/wic_replay_parser" \
WIC_REQUIRE_ALL_GROUND_TRUTH=1 \
    python3 parser/test_ground_truth.py
