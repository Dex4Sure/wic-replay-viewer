#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
corpus_root=
repeat=5
jobs=()
replays=()

usage() {
    printf '%s\n' \
        "Usage: $0 [--corpus-root PATH] [--jobs N]... [--replay PATH]... [--repeat N]" \
        '' \
        'Builds the release CLI, hashes every measured JSON result, and reports GNU time' \
        'wall/CPU/RSS measurements. --jobs may be repeated; it defaults to 4 for a corpus.'
}

positive_integer() {
    [[ $1 =~ ^[1-9][0-9]*$ ]]
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
        --jobs)
            if (($# < 2)) || ! positive_integer "$2"; then
                printf '%s requires a positive integer.\n' "$1" >&2
                usage >&2
                exit 2
            fi
            jobs+=("$2")
            shift 2
            ;;
        --replay)
            if (($# < 2)); then
                printf '%s requires a path.\n' "$1" >&2
                usage >&2
                exit 2
            fi
            replays+=("$2")
            shift 2
            ;;
        --repeat)
            if (($# < 2)) || ! positive_integer "$2"; then
                printf '%s requires a positive integer.\n' "$1" >&2
                usage >&2
                exit 2
            fi
            repeat=$2
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

if [[ -z $corpus_root ]] && ((${#replays[@]} == 0)); then
    usage >&2
    exit 2
fi
if [[ -n $corpus_root ]] && ((${#jobs[@]} == 0)); then
    jobs=(4)
fi
if [[ ! -x /usr/bin/time ]]; then
    printf 'GNU time is required at /usr/bin/time.\n' >&2
    exit 1
fi

cargo_bin=${WIC_CARGO:-cargo}
binary=$repo_root/../target/release/wic_replay_parser

cd "$repo_root"
"$cargo_bin" build --release --offline --locked --all-features \
    --manifest-path rust_parser/Cargo.toml

for replay in "${replays[@]}"; do
    if [[ ! -f $replay ]]; then
        printf 'Replay not found: %s\n' "$replay" >&2
        exit 1
    fi
    printf 'replay=%s sha256=' "$replay"
    "$binary" --timeline-json "$replay" | sha256sum | cut -d ' ' -f 1
    /usr/bin/time \
        -f "replay repeat=$repeat elapsed=%e user=%U sys=%S maxrss_kb=%M" \
        bash -c 'for ((run = 0; run < $3; run++)); do "$1" --timeline-json "$2" >/dev/null; done' \
        benchmark-replay "$binary" "$replay" "$repeat"
done

if [[ -n $corpus_root ]]; then
    if [[ ! -d $corpus_root ]]; then
        printf 'Corpus root not found: %s\n' "$corpus_root" >&2
        exit 1
    fi
    for worker_count in "${jobs[@]}"; do
        /usr/bin/time \
            -f "corpus jobs=$worker_count elapsed=%e user=%U sys=%S maxrss_kb=%M" \
            bash -o pipefail -c '"$1" --corpus-summary-json "$2" --jobs "$3" | sha256sum' \
            benchmark-corpus "$binary" "$corpus_root" "$worker_count"
    done
fi
