# Replay research

This area contains the investigations and tools supporting the parser and viewer.
Run examples from the repository root. Product build instructions are in the
[root README](../README.md). The research roadmap is [TODO.md](TODO.md); dated
reports are under [findings](findings/) and investigation notes under [notes](notes/).
The former workspace changelog is [HISTORY.md](HISTORY.md).

The [Tactical Aid timer investigation](findings/tactical-aid-timers-2026-09-11.md)
traces the game's countdown renderer and provides a shipped-data extraction
command, including the distinction between call-in and deployment-event times.

Historical reports retain replay identities, hashes, and measured results. Private
absolute path prefixes are shown as placeholders. For new
runs, use `research/scripts/`, `local/binaries/`, `local/replays/`, and
`local/generated/`; the parser is under `parser/`. Rust probes under findings are
source templates to copy into the crate examples directory specified in their
header, not independently built product crates. Some probes target the historical
revision identified by their report; do not silently adapt evidence semantics.

### Replay viewer Linux rendering

The replay viewer applies Linux-only X11 GTK and WebKitGTK shared-memory defaults
before the native webview starts. They prevent a fully initialized Vue/Tauri window
from presenting as blank white on the validated Fedora mixed-scale Wayland and
DMA-BUF paths while leaving other platforms unchanged. The diagnosis, direct native
window evidence, override boundary, and future retest condition are recorded in
`research/notes/replay-viewer-webkit-white-window.md`; component run instructions remain in
the root `README.md`.

### Replay-state reconstruction research

`research/scripts/replay_state_reconstruction.py` is a research decoder
for map playback evidence. It exports versioned JSON containing unit lifecycle
generations, raw full/compact `UnitFrame` words, health and ownership changes,
movement orders, command/perimeter points, and tactical-aid positions. It does
not change the canonical parser schema, and it labels the unrecorded interval
between authoritative checkpoints as hold-last rather than inventing motion.

```bash
"$HOME/.venvs/wic-analysis/bin/python" \
  research/scripts/replay_state_reconstruction.py export replay.wicdemo \
  --map-archive local/binaries/game/wic60.sdf --json /tmp/replay-state.json
```

Open `research/scripts/playback-proof/index.html` and select that JSON to exercise the
dependency-free Canvas proof viewer. `?src=synthetic.json` loads its public
synthetic smoke fixture. Map art is only read from an explicitly named local
SDF and embedded in the ignored export; no copyrighted asset is tracked.

`research/scripts/replay_state_oracle.py compare` performs an offline bit-for-bit check
against a runtime capture. Its `capture` command verifies the supplied binary
hash and requires `--confirm-attach`; attaching to a running game remains a
separately approved operation under this workspace's evidence rules.

## Private-data boundary

The following stay local and are ignored by Git:

- Game executables, DLLs, and other copyrighted assets
- Replay corpora and runtime captures
- The active Ghidra project database
- `.env` and machine-specific paths
- Generated analysis output and build caches
- Writable runtime state

`research/manifests/targets.sha256` identifies the canonical executables without
including them. Analysis tools treat the source evidence as read-only; derived
output belongs under `local/generated/`; reviewed reports stay in `research/findings/`.

## Quick start

Run commands from the unified repository root. There are no submodules to
initialize. The portable product gate needs no game files or replay collection:

```bash
./scripts/quality.sh full
```

For the original research evidence layout, copy `.env.example` to `.env` and
configure your private paths.

Set `WIC_DATA_ROOT` in `.env` to an evidence tree containing `wic/`, `replays/`,
`wic_settings/`, and `massgate_wicgate/`, then initialize the ignored links:

```bash
./research/scripts/bootstrap-local-data.sh
```

The bootstrap script checks the executable identities against
`research/manifests/targets.sha256` before reporting the evidence ready.

### Use a different replay collection

The bootstrap script and tracked binary hashes describe the original evidence
set; they are not required for your own replays. Keep your `.wicdemo` files
outside Git and pass their directory to research scripts that accept a replay
path. For scripts that use the conventional `local/replays/main` path, create an
ignored link to your collection in a fresh checkout:

```bash
mkdir -p local/replays
ln -s /path/to/your/replays local/replays/main
```

Do not replace an existing path or run the bootstrap script over a different
layout. Record the hashes and game build of inputs used in new findings. The
tracked `parser/ground_truth.json` and `tests/private-fixtures.json` describe
the original private corpus, which is not included in Git. You can point the
parser harness at your own expectation file with `WIC_GROUND_TRUTH_FILE` and
your replay root with `WIC_REPLAY_ROOT`. The repository's strict private gate
also runs tests written for the original fixtures; a different collection can
use the portable gate and its own focused tests without those files.

## Host environment

Research tools run directly on the host. Install these prerequisites when
setting up another machine:

- Ghidra 12.1.2 PUBLIC under `$HOME/Tools/ghidra_12.1.2_PUBLIC` and a separate
  JDK 21 under `$HOME/Tools/jdk-21`; Temurin 21.0.12 was tested.
- Python 3.14 for analysis and a separate Python 3.11 environment for Ghidra MCP.
- Rust through rustup, using the toolchain selected by
  `rust-toolchain.toml`, including rustfmt and Clippy.
- 32-bit MinGW, GCC/G++, Clang, CMake, Ninja, GDB, Git, `lsof`, `jq`, and binary
  utilities. WiC targets use the `i686-w64-mingw32-*` tools.

With both Python interpreters available, create the separate environments and
install the tracked dependency pins from the repository root:

```bash
python3.14 -m venv "$HOME/.venvs/wic-analysis"
"$HOME/.venvs/wic-analysis/bin/python" -m pip install -r research/requirements/analysis.txt
python3.11 -m venv "$HOME/.venvs/wic-ghidra-mcp-py311"
"$HOME/.venvs/wic-ghidra-mcp-py311/bin/python" -m pip install -r research/requirements/mcp.txt
```

The analysis environment uses the purpose-based name `wic-analysis`. Dated research
reports may retain the former `wic-re` environment path as historical context; use
`~/.venvs/wic-analysis/bin/python` when rerunning their commands.

Keep these environments separate. Python requirements and Cargo lockfiles cover
their respective dependencies; Ghidra, Java, and system tools need their own
installation. The viewer's [build instructions](README.md#build-from-source)
cover Node.js and desktop application dependencies.

## Ghidra and Codex

Start a new Codex session from the repository root after setting up the optional
Ghidra environment:

```bash
cd /path/to/wic-replay-viewer
codex
```

Codex discovers the root `AGENTS.md` and the trusted project-local
`.codex/config.toml`, which makes the `ghidra-wic` MCP server available only to
sessions opened in this workspace. Nothing needs to be started beforehand: the
STDIO server is launched on demand using the host Ghidra and Python installations.

The local working project is `local/ghidra/WiCProject.gpr` and currently contains
`wic.exe` and `wic_ds.exe`. Read-only headless inspection uses:

```bash
./research/scripts/ghidra-headless.sh -process wic.exe -readOnly -noanalysis
./research/scripts/ghidra-headless.sh -process wic_ds.exe -readOnly -noanalysis
```

`research/scripts/ghidra-mcp.sh` launches the project-specific STDIO MCP server. Its
registration is tracked in `.codex/config.toml`; start a fresh Codex session
after changing MCP configuration. Automatic whole-project semantic indexing is
disabled, and ordinary metadata/decompiler/read tools do not trigger it as a
side effect. Code and string searches request their corresponding index lazily.
The launcher also restores the persisted Ghidra analyzed state for existing
project programs, working around the upstream `pyghidra-mcp 0.2.3` behavior that
otherwise reports them as incomplete and rejects most tool calls.
On POSIX hosts it also replaces MCP SDK 1.26's thread-backed server-side STDIO
reader with a selector-driven transport, which avoids a stalled initialize
handshake under the Codex 0.147 Linux command sandbox.
Before opening the project, the launcher verifies the `.gpr` and companion `.rep`,
uses `lsof` to reject any active project holder, and removes only the exact
`WiCProject.lock`/`WiCProject.lock~` pair when they are proven stale. It fails closed
when project integrity or holder status cannot be established.

Validate both MCP transport and a real analysis operation with:

```bash
"$HOME/.venvs/wic-ghidra-mcp-py311/bin/python" \
  research/scripts/verify-ghidra-mcp.py
```

The host Ghidra/JDK installations, Python virtual environments,
private evidence, and active Ghidra database are machine-local and are
not carried by Git. The project-scoped Codex MCP registration is carried by
Git. On another machine, complete the bootstrap described above and install or
restore those prerequisites under the documented paths in `$HOME`. Restore a
working copy of the Ghidra project under `local/ghidra/`, preserving the recovery
snapshot. If different paths are required, export `GHIDRA_INSTALL_DIR`, `JAVA_HOME`, and
`WIC_GHIDRA_MCP_VENV` before starting Codex.

```bash
cd /absolute/path/to/wic-replay-viewer
codex mcp list
codex
```

## Analysis scripts

Replay-format research runs from tracked scripts rather than throwaway probes, so
the reports in `research/findings/` have reproducible supporting artifacts. These tools are read-only with
respect to `local/binaries/` and `local/replays/`.

| Script                                                   | Purpose                                                                                                                                                                                                                                                                                                     |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `research/scripts/wic_bintag.py`                         | Shared reader for the `.wicdemo` container and its `Event` envelope chain. Documents the envelope and field layout, walks messages exactly rather than by pattern matching, and inverts BinTag name hashes using identifier strings mined from the shipped binaries. Imported by the replay research tools. |
| `research/scripts/ta-usage-audit.py`                     | Bounded parallel corpus audit of the `ChangeHonors` / `SupportThingUsed` pair. Measures the record pairing instead of assuming it, and cross-checks tactical-aid gifts to establish that the honors ledger is point-of-view local.                                                                          |
| `research/scripts/timeline_attribution.py`               | Shared extraction and exact-assignment helpers for purchases, recorder-visible-faction markers, delayed spawns, feedback, projectiles, and requests.                                                                                                                                                        |
| `research/scripts/timeline-attribution-audit.py`         | Applies the attribution graph to one replay or a bounded corpus, retaining exact links only when the complete one-to-one assignment is unique.                                                                                                                                                              |
| `research/scripts/spectator_ta_audit.py`                 | Decodes serialized spectator team/LOS changes and compares top-level player-bearing markers with both-faction delayed-spawn/feedback effects under ordinary, one-team, and all-team recorder views.                                                                                                         |
| `research/scripts/opposing_ta_player_audit.py`           | Validates the schema-v9 unit-drop ownership bridge against player-bearing marker ground truth and measures newly recovered opposing/spectator players.                                                                                                                                                      |
| `research/scripts/ta_request_bridge_audit.py`            | Tests whether player-bearing tactical-aid requests can identify later users; current evidence rejects requests as actor attribution.                                                                                                                                                                        |
| `research/scripts/ta_projectile_bridge_audit.py`         | Tests support-projectile IDs against normal player-owned projectile records; current evidence finds no shared identifier bridge.                                                                                                                                                                            |
| `research/scripts/ta_projectile_simulation.py`           | Strictly decodes all four support-projectile families, binds them to shipped mover/effect definitions, reproduces straight and ballistic free flight only with explicit tick deltas, and inventories corpus effects without converting proximity into impact or death attribution.                          |
| `research/scripts/unit_destruction_attribution_audit.py` | Resolves exact `SendTATaunt` actor/target/support damage notifications, preserves unit lifecycle evidence, and reports same-tick victim destructions only as non-causal candidates.                                                                                                                         |
| `research/scripts/player_identity_session_audit.py`      | Audits name-bearing player-entry and leave lifecycle records, reporting every replay where a numeric player slot carries different occupants.                                                                                                                                                               |
| `research/scripts/wic_sdf.py`                            | Bounded read-only inspector and plain/zlib extractor for the shipped RYS/SDF v9/v10 archives, based on independently traced client and dedicated-server loaders.                                                                                                                                            |
| `research/scripts/ta_support_catalogue.py`               | Recovers support definition and GUI names from `maps/supportweapons.loc`, proves each replay ID by Adler-32 round trip, detects collisions, and classifies top-level aids separately from child effects and special abilities.                                                                              |
| `research/scripts/timeline-corpus-check.py`              | Runs the bounded parallel release parser over a corpus and summarises raw timeline output by game mode, so thin Assault and Tug of War coverage stays visible next to the Domination-heavy bulk.                                                                                                            |

```bash
research/scripts/ta-usage-audit.py local/replays/main --jobs 4 --json local/generated/ta-usage-audit.json
research/scripts/timeline-attribution-audit.py local/replays/main --jobs 4 \
  --json local/generated/timeline-attribution-audit-v3.json
research/scripts/ta_support_catalogue.py \
  --marker-audit local/generated/timeline-attribution-audit-v3.json \
  --json local/generated/ta-support-catalogue.json
research/scripts/opposing_ta_player_audit.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 4 \
  --summary-only --json local/generated/opposing-ta-player-audit-summary.json
research/scripts/ta_request_bridge_audit.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 4 \
  --json local/generated/ta-request-bridge-audit.json
research/scripts/ta_projectile_bridge_audit.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 4 \
  --json local/generated/ta-projectile-bridge-audit.json
research/scripts/ta_projectile_simulation.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 8 \
  --json local/generated/ta-projectile-simulation-corpus.json
research/scripts/unit_destruction_attribution_audit.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 4 \
  --summary-only --json local/generated/unit-destruction-attribution-audit.json
PYTHONPATH=research/scripts "$HOME/.venvs/wic-analysis/bin/python" \
  research/scripts/player_identity_session_audit.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 8 \
  --json local/generated/player-identity-session-corpus-audit.json
cargo build --release --locked --all-features \
  --manifest-path parser/rust_parser/Cargo.toml
research/scripts/timeline-corpus-check.py \
  local/replays/main local/replays/settings local/replays/wicgate-documents --jobs 4 \
  --binary target/release/wic_replay_parser \
  --json local/generated/timeline-corpus.json
```

The parser is at timeline schema v18 and still retains the schema-v9 unit-drop
attribution boundary. The counts below were measured on the schema-v11 run:
the corpus gate accepts 2,865 replays and retains the 15 known
corrupt/empty rejects. It emits 223,426 both-faction top-level tactical-aid
deployments; 91,932 carry the validated `unitSpawnOwnership` player and none has an
invalid player, participant reference, faction, or timeline timestamp. The evidence
boundary and ruled-out actor bridges are recorded in
`research/findings/opposing-player-tactical-aid-attribution-2026-08-18.md`.
Schema v11 additionally resolves reused player slots through time-bounded sessions;
the 2,880-path lifecycle audit found different named occupants in 983 replays and
2,525 slot instances. See `research/findings/player-identity-sessions-2026-08-20.md`.

Their bulk JSON output and logs are ignored by Git — they enumerate private corpus
paths and run to tens of megabytes. The reviewed reports in `research/findings/` stay tracked.

## Verification

Install the tracked native Git hooks once per clone:

```bash
./scripts/install-hooks.sh
```

One hook installer serves the entire repository. Pre-commit runs the shared fast
gate, and pre-push runs the full gate. Research linting and portable tests are
included; no parent repository or Gitlink publication check is required.

The viewer's fast gate covers Python and Rust formatting, type-aware ESLint,
Prettier, and Vue TypeScript checks. Its full gate also runs the portable tests,
coverage checks, frontend build, and Clippy with warnings denied. Private inputs
are checked separately as described below. Do not bypass a failing hook with
`--no-verify` unless the user explicitly approves the exception.

For optional parser WASM verification, the retained
`parser/scripts/verify-release.sh --corpus-root <path>`
requires `wasm-pack` and cached Cargo dependencies. Product release preparation
uses `npm run release:prepare` in the viewer repository.

Run the research environment baseline on the host when the Ghidra project is
free. Close any session holding the project first; this script opens the same
database used by MCP:

```bash
./research/scripts/verify-environment.sh
cargo clippy --locked --all-targets --all-features \
  --manifest-path parser/rust_parser/Cargo.toml -- -D warnings
```

The baseline verifies executable hashes, discovers the local replay corpora,
opens both recovered Ghidra programs read-only, imports the Python RE packages,
and runs the locked Rust tests. It uses `$HOME/.venvs/wic-analysis/bin/python` by
default; set `WIC_ANALYSIS_PYTHON` for another analysis environment. For Ghidra
installed outside the configured host paths, set `GHIDRA_INSTALL_DIR` and
`JAVA_HOME`. Cargo can fetch locked dependencies on first use; offline runs
require those dependencies to be cached already.

For private replay and installed-game validation, configure the viewer's private
test environment as described in its README, then run:

```bash
scripts/quality.sh private
```

That gate requires all 17 ground-truth fixtures to pass and fails on missing
private inputs; skipped fixtures do not constitute corpus validation.

## Working principles

Preserve original evidence, record input hashes, and write modifications only
to derived-output locations. Keep observations distinct from hypotheses and
attach important conclusions to binary addresses, replay hashes, decompiler
output, or repeatable tests. See the root `AGENTS.md` for the complete operating rules
used by coding agents in this workspace.
