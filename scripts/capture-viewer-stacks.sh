#!/usr/bin/env bash
# Explicit opt-in Linux capture. Never called by the automatic reporter.
set -euo pipefail
if [[ $# != 1 || ! $1 =~ ^[1-9][0-9]*$ ]]; then
    printf 'Usage: bash scripts/capture-viewer-stacks.sh APPLICATION_CHILD_PID\n' >&2
    exit 2
fi
capture_pid=$1
capture_executable=$(readlink -e "/proc/$capture_pid/exe")
case "${capture_executable##*/}" in
    wic-replay-viewer | error_reporting_probe) ;;
    *) printf 'Refusing: PID is not a viewer executable.\n' >&2; exit 2 ;;
esac
if ! tr '\0' '\n' < "/proc/$capture_pid/cmdline" | rg -qx -- '--wic-application-session'; then
    printf 'Refusing: PID is not a supervised viewer application child.\n' >&2
    exit 2
fi
command -v gdb >/dev/null
capture_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
umask 077
mkdir -p "$capture_root/local/generated/viewer-stacks"
capture_directory=$(mktemp -d "$capture_root/local/generated/viewer-stacks/capture-XXXXXXXX")
sha256sum "$capture_executable" > "$capture_directory/executable.sha256"
printf 'Capturing native stacks; the viewer pauses briefly. Output: %s\n' "$capture_directory"
# No init scripts, downloaded symbols, argument values, locals or memory dump.
# Backtraces still contain code paths/symbols. Review before sharing.
gdb -nx -nh -batch \
    -iex 'set auto-load off' \
    -iex 'set debuginfod enabled off' \
    -iex 'set pagination off' \
    -iex 'set print frame-arguments none' \
    -p "$capture_pid" \
    -ex 'thread apply all bt 32' \
    -ex 'detach' > "$capture_directory/stacks.txt" 2>&1
printf 'Saved %s/stacks.txt\n' "$capture_directory"
