#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
env_file=${WIC_RE_ENV_FILE:-"$repo_root/.env"}

if [[ -f "$env_file" ]]; then
    set -a
    # This is a machine-local, user-controlled configuration file.
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
fi

: "${WIC_DATA_ROOT:?Set WIC_DATA_ROOT in $env_file or the environment}"
data_root=${WIC_DATA_ROOT%/}

required_directories=(
    "$data_root/wic"
    "$data_root/replays"
    "$data_root/wic_settings/Replay"
    "$data_root/massgate_wicgate/bin/client"
    "$data_root/massgate_wicgate/bin/server"
    "$data_root/massgate_wicgate/Documents/World in Conflict/Replay"
)

for directory in "${required_directories[@]}"; do
    if [[ ! -d "$directory" ]]; then
        printf 'Missing evidence directory: %s\n' "$directory" >&2
        exit 1
    fi
done

mkdir -p "$repo_root/local/binaries" "$repo_root/local/replays" "$repo_root/local/generated" "$repo_root/local/captures"

link_evidence() {
    local source_path=$1
    local link_path=$2

    if [[ -L "$link_path" ]]; then
        if [[ $(readlink -- "$link_path") == "$source_path" ]]; then
            return
        fi
        printf 'Refusing to replace existing symlink: %s\n' "$link_path" >&2
        exit 1
    fi

    if [[ -e "$link_path" ]]; then
        printf 'Refusing to replace existing path: %s\n' "$link_path" >&2
        exit 1
    fi

    ln -s -- "$source_path" "$link_path"
}

link_evidence "$data_root/wic" "$repo_root/local/binaries/game"
link_evidence "$data_root/massgate_wicgate/bin/client" "$repo_root/local/binaries/client"
link_evidence "$data_root/massgate_wicgate/bin/server" "$repo_root/local/binaries/server"
link_evidence "$data_root/replays" "$repo_root/local/replays/main"
link_evidence "$data_root/wic_settings/Replay" "$repo_root/local/replays/settings"
link_evidence \
    "$data_root/massgate_wicgate/Documents/World in Conflict/Replay" \
    "$repo_root/local/replays/wicgate-documents"

(
    cd "$repo_root"
    sha256sum --check research/manifests/targets.sha256
)

replay_count=$(find -L "$repo_root/local/replays" -type f -iname '*.wicdemo' -print0 | tr -cd '\000' | wc -c)
printf 'Local evidence ready: %s replay files across linked corpora.\n' "$replay_count"
