#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
dry_run=false

for argument in "$@"; do
    if [[ "$argument" == "--dry-run" ]]; then
        dry_run=true
    fi
done

cd "$repo_root"

if [[ "$dry_run" == false ]]; then
    branch=$(git branch --show-current)
    if [[ "$branch" != "main" ]]; then
        printf 'Release preparation requires branch main; current branch is %s.\n' \
            "${branch:-detached}" >&2
        exit 1
    fi

    if [[ -n "$(git status --porcelain)" ]]; then
        printf '%s\n' \
            'Release preparation requires a clean worktree. Commit or stash changes first.' >&2
        exit 1
    fi
fi

WIC_RELEASE_CHECKOUT_VERIFIED=1 node scripts/version.mjs prepare "$@"
