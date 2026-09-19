#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
workspace_root="$repo_root/local"
cd "$repo_root"

roots=(
    "$workspace_root/replays/main"
    "$workspace_root/replays/settings"
    "$workspace_root/replays/wicgate-documents"
)
for root in "${roots[@]}"; do
    [[ -d "$root" ]] || {
        printf 'Corpus root is unavailable: %s\n' "$root" >&2
        exit 1
    }
done

printf 'Building release parser for the occasional full-corpus audit\n'
cargo build --release --locked --all-features --manifest-path parser/rust_parser/Cargo.toml

corpus_json=$(mktemp --tmpdir wic-corpus-summary.XXXXXX.json)
trap 'rm -f -- "$corpus_json"' EXIT
"$repo_root/target/release/wic_replay_parser" --corpus-summary-json \
    "${roots[@]}" > "$corpus_json"
python3 scripts/verify-corpus-baseline.py \
    --baseline tests/corpus-aggregate-baseline.json < "$corpus_json"
