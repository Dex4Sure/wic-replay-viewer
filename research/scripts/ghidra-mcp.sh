#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
ghidra_install_dir=${GHIDRA_INSTALL_DIR:-$HOME/Tools/ghidra_12.1.2_PUBLIC}
java_home=${JAVA_HOME:-$HOME/Tools/jdk-21}
mcp_venv=${WIC_GHIDRA_MCP_VENV:-$HOME/.venvs/wic-ghidra-mcp-py311}
project_file=${WIC_GHIDRA_PROJECT:-"$repo_root/local/ghidra/WiCProject.gpr"}

"$repo_root/research/scripts/clear-stale-ghidra-locks.sh" "$project_file"

mkdir -p "$repo_root/local/state/cache" "$repo_root/local/state/home" "$repo_root/local/state/tmp"

export GHIDRA_INSTALL_DIR="$ghidra_install_dir"
export JAVA_HOME="$java_home"
export JAVA_TOOL_OPTIONS="-Djava.awt.headless=true -Duser.home=$repo_root/local/state/home -Dapplication.cachedir=$repo_root/local/state/cache -Dapplication.tempdir=$repo_root/local/state/tmp ${JAVA_TOOL_OPTIONS:-}"

exec "$mcp_venv/bin/python" "$repo_root/research/scripts/pyghidra-mcp-no-eager-index.py" \
    --project-path "$project_file" \
    "$@"
