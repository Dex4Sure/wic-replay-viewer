#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    printf 'Usage: %s /absolute/path/to/Project.gpr\n' "$0" >&2
    exit 2
fi

project_file=$(realpath -- "$1")
if [[ ! -f "$project_file" || "$project_file" != *.gpr ]]; then
    printf 'Refusing lock cleanup: expected an existing .gpr file: %s\n' "$project_file" >&2
    exit 1
fi

project_base=${project_file%.gpr}
project_store="$project_base.rep"
if [[ ! -d "$project_store" ]]; then
    printf 'Refusing lock cleanup: companion project store is missing: %s\n' "$project_store" >&2
    exit 1
fi

locks=("$project_base.lock" "$project_base.lock~")
existing_locks=()
for lock_file in "${locks[@]}"; do
    if [[ -L "$lock_file" || ( -e "$lock_file" && ! -f "$lock_file" ) ]]; then
        printf 'Refusing lock cleanup: unexpected lock-file type: %s\n' "$lock_file" >&2
        exit 1
    fi
    if [[ -f "$lock_file" ]]; then
        existing_locks+=("$lock_file")
    fi
done

if [[ ${#existing_locks[@]} -eq 0 ]]; then
    exit 0
fi
if ! command -v lsof >/dev/null 2>&1; then
    printf 'Refusing lock cleanup: lsof is required to prove the project is unused.\n' >&2
    exit 1
fi

project_files=("$project_file" "${existing_locks[@]}")
for project_path in "${project_files[@]}"; do
    if lsof -- "$project_path" >/dev/null 2>&1; then
        printf 'Refusing lock cleanup: a process still holds the Ghidra project.\n' >&2
        exit 1
    fi
done
if lsof +D "$project_store" >/dev/null 2>&1; then
    printf 'Refusing lock cleanup: a process still holds the Ghidra project.\n' >&2
    exit 1
fi

rm -- "${existing_locks[@]}"
printf 'Removed stale Ghidra project lock files after an empty lsof check.\n' >&2
