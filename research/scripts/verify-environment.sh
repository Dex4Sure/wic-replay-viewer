#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)

"$repo_root/research/scripts/bootstrap-local-data.sh"

"$repo_root/research/scripts/ghidra-headless.sh" \
    -process wic.exe -readOnly -noanalysis -max-cpu 2
"$repo_root/research/scripts/ghidra-headless.sh" \
    -process wic_ds.exe -readOnly -noanalysis -max-cpu 2

analysis_python=${WIC_ANALYSIS_PYTHON:-$HOME/.venvs/wic-analysis/bin/python}

"$analysis_python" -c \
    "import pefile, lief, capstone, unicorn, construct, frida; print('Python RE packages OK')"

cargo test --locked --all-features \
    --manifest-path "$repo_root/parser/rust_parser/Cargo.toml"

printf 'Environment verification passed.\n'
