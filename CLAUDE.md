# WiC Replay Viewer and Research

## Project purpose

This repository owns the native replay viewer, its canonical parser, and
authorized reverse engineering of the original *World in Conflict* game,
Windows binaries, and `.wicdemo` replay format. Viewer, parser, and research
changes use the same branch and commit workflow.

Keep routine development and portable quality gates independent of private
game files, replay corpora, machine-specific tools, and active research
databases.

## Instruction synchronization

`AGENTS.md` is the source of truth for repository instructions. The root
`CLAUDE.md` must remain byte-for-byte identical. Run
`scripts/sync-agent-instructions.sh` after editing `AGENTS.md`; the tracked
pre-commit hook performs the same synchronization and stages `CLAUDE.md` when
`AGENTS.md` is part of a commit. Do not edit the root `CLAUDE.md` directly.

## Repository and workspace layout

- `frontend/`, `src/`, and `src-tauri/` contain the replay viewer.
- `parser/` contains the parser, schema documentation, reference code, and
  ground-truth tooling.
- `research/scripts/` contains reproducible analysis and environment helpers.
- `research/findings/` contains tracked evidence reports and probes.
- `research/notes/` contains format and investigation notes.
- `research/manifests/` identifies private source evidence without storing it.
- `research/requirements/` pins research Python environments.
- `local/binaries/` contains user-supplied executables and DLLs.
- `local/replays/` contains private replay corpora and edge-case samples.
- `local/ghidra/` contains the active Ghidra project.
- `local/captures/` contains debugger, instrumentation, and network captures.
- `local/generated/` contains derived artifacts, exports, reports, and patched
  copies.

The `local/` workspace is ignored and machine-specific. A contributor may use a
different replay collection or host layout; portable code and documentation
must not depend on the maintainer's private workspace.

## Critical data and evidence rules

- Treat files in `local/binaries/` and `local/replays/` as immutable source
  evidence. Do not patch, rename, move, or overwrite them.
- Treat every `.wicdemo` as an immutable input. The application may store its
  path and derived data, but must not modify the replay.
- Write modified binaries and derived research output under `local/generated/`.
  Record SHA-256 hashes for inputs used in reproducible findings.
- Never commit copyrighted game binaries, private replay corpora, secrets, or
  generated Ghidra databases.
- Preserve user-supplied Ghidra projects, symbols, names, comments, types,
  scripts, replay expectations, and empirical notes.
- Keep observations, hypotheses, and unknowns distinct. Cite binary addresses,
  replay hashes, decompiler output, debugger captures, or corpus results for
  factual research claims. Do not infer identity or causality without evidence.
- Preserve clearly labelled historical evidence. Do not rewrite old findings
  to match later conclusions without recording the correction.

## Replay viewer and parser architecture rules

- Treat the viewer and embedded parser as one replay product. The parser is the
  canonical implementation for viewer data.
- Keep parser implementation, schema documentation, Python reference code, and
  ground-truth tooling under `parser/`.
- Treat parser JSON changes as API decisions. Update the schema, typed viewer
  projections, cache contracts, tests, and documentation together.
- Treat the parser JSON schema and viewer database schema as independent
  compatibility identifiers. Change each only when its format changes.
- Preserve bounded import concurrency and lazy replay-detail loading.
- Keep library discovery and replay browsing read-only. Restrict filesystem
  writes to explicit management actions and derived application data.
- Retain unknown values when the replay or binary evidence does not determine a
  result.

## Versioning and release rules

- The replay viewer is the single versioned product. `package.json` is the
  product-version source; release preparation synchronizes packaged copies.
- The parser has no separate routine product version while it ships as part of
  the viewer. Parser JSON and viewer database schema versions remain separate
  compatibility identifiers.
- Do not bump versions during ordinary development.
- Prepare a release only from a clean `main` checkout with
  `npm run release:prepare -- patch|minor|major`. The command runs release gates
  but does not commit, tag, push, or publish.
- A `v*` tag must match every packaged application version. Release CI builds
  the Windows portable ZIP and Linux AppImage and creates a draft release.
- Committing, pushing, tagging, and publishing are separate authorization
  boundaries. Create or publish a release only when the user explicitly asks.
- Keep public release titles and notes brief and user-facing. Put technical
  implementation and workflow details in the changelog and contributor docs.

Historical version lineage belongs in `CHANGELOG.md` and
`research/HISTORY.md`; contributor release steps belong in
`docs/contributing.md`.

## Reverse-engineering rules

- Prefer Ghidra headless for routine automation. Use the GUI when an
  investigation specifically benefits from it.
- Do not run concurrent writers against `local/ghidra/WiCProject`. Use
  read-only jobs when inspection does not require project mutation.
- Establish and record the exact game build, executable hash, load address, and
  architecture before trusting addresses.
- Prefer reproducible scripts and exported findings over one-off GUI work.
- Separate observations from hypotheses and preserve the evidence supporting
  both positive and negative results.
- Ask before changing Ghidra annotations in bulk, patching binaries, attaching
  to a running game process, or sending traffic.

## Development and testing rules

- Install the tracked hooks once per clone with `scripts/install-hooks.sh`.
- Keep portable tests independent of private files and installed game data.
  Private tests must remain explicitly ignored in portable Cargo runs and fail
  closed when selected.
- Validate parser changes against relevant available evidence, including
  domination games, mid-game joins, absent `TeamWins` events, and known-corrupt
  replays. Contributors may use their own replay fixtures and manifests.
- Add or update tests for behavior and compatibility changes. Avoid tests that
  merely duplicate implementation details.
- Never bypass failing hooks with `--no-verify` unless the user explicitly
  approves the exception.

## Documentation and changelog rules

- Update root `CHANGELOG.md` for notable product, research-tooling, and workflow
  changes. Preserve detailed research history in `research/HISTORY.md`.
- Keep root `README.md` concise and user-focused: downloads, features, and first
  use first, with links to detailed user and contributor guides.
- Update stale documentation references after code, schema, command, or
  workflow changes.
- Keep current setup and usage in maintained guides. Keep investigations,
  migrations, incidents, and superseded behavior in historical records.
- Preserve player and replay details when they are relevant research evidence;
  remove personal host paths and unrelated private information from public
  documentation.

## Quality gates

- `./scripts/quality.sh fast` is the pre-commit gate.
- `./scripts/quality.sh full` is the portable pre-push gate and is required
  before publishing changes.
- `./scripts/coverage.sh` runs the portable coverage subset.
- `./scripts/quality.sh private` loads the ignored local environment, validates
  private fixtures, runs the portable gate, and requires every configured
  private regression and control to execute successfully.
- `./scripts/quality.sh corpus` runs the complete configured replay-corpus audit
  and is an occasional explicit check.

Do not weaken, skip, or silently reclassify a failing gate to make it pass.

## Reverse-engineering environment

Reverse-engineering tooling and host setup are documented in
`research/README.md`.

Use `research/scripts/ghidra-headless.sh` for normal Ghidra automation. The
project-local `ghidra-wic` MCP server is configured through
`.codex/config.toml`.

Do not run concurrent writers against `local/ghidra/WiCProject`. Do not modify
source evidence under `local/binaries/` or `local/replays/`.
