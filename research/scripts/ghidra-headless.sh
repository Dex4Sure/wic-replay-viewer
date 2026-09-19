#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
ghidra_install_dir=${GHIDRA_INSTALL_DIR:-$HOME/Tools/ghidra_12.1.2_PUBLIC}
java_home=${JAVA_HOME:-$HOME/Tools/jdk-21}
project_file=${WIC_GHIDRA_PROJECT:-"$repo_root/local/ghidra/WiCProject.gpr"}
project_name=${GHIDRA_PROJECT_NAME:-$(basename -- "${project_file%.gpr}")}
project_dir=$(dirname -- "$project_file")
"$repo_root/research/scripts/clear-stale-ghidra-locks.sh" "$project_file"

mkdir -p "$repo_root/local/state/cache" "$repo_root/local/state/tmp"

export JAVA_HOME="$java_home"
export JAVA_TOOL_OPTIONS="-Dapplication.cachedir=$repo_root/local/state/cache -Dapplication.tempdir=$repo_root/local/state/tmp ${JAVA_TOOL_OPTIONS:-}"

exec "$ghidra_install_dir/support/analyzeHeadless" \
    "$project_dir" "$project_name" "$@"
