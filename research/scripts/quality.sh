#!/usr/bin/env bash
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
mode=${1:-full}
case "$mode" in fast|full) ;; *) printf 'Usage: %s {fast|full}\n' "$0" >&2; exit 2 ;; esac
ruff_bin=${WIC_RUFF:-ruff}
if ! command -v "$ruff_bin" >/dev/null 2>&1; then
    ruff_bin="$HOME/.venvs/wic-analysis/bin/ruff"
fi
cd "$repo_root"
"$ruff_bin" check research/scripts
"$ruff_bin" format --check research/scripts
if [[ "$mode" == full ]]; then
    "${WIC_PYTHON:-python3}" -m unittest discover -s research/scripts -p 'test_*.py'
    "${WIC_PYTHON:-python3}" research/findings/test-event-roster-comparison-2026-09-08.py
fi
