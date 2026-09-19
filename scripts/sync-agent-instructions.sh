#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
source_file="$repo_root/AGENTS.md"
mirror_file="$repo_root/CLAUDE.md"

if [[ ! -f "$source_file" ]]; then
    printf 'Instruction source is missing: %s\n' "$source_file" >&2
    exit 1
fi

if cmp -s -- "$source_file" "$mirror_file"; then
    exit 0
fi

cp -- "$source_file" "$mirror_file"
printf 'Synchronized CLAUDE.md from AGENTS.md\n'
