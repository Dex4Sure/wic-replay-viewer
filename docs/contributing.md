# Contributing and building

The viewer, embedded parser, and research tools share one repository. Start with
[AGENTS.md](../AGENTS.md) for repository working rules. Normal product development
does not require private game files or Ghidra.

## Build from source

The development environment requires Node.js 24, Rust 1.98.0 as selected by the
tracked `rust-toolchain.toml`, and the
[Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/).
Install the locked frontend dependencies and launch the development window:

```bash
npm ci
npm run tauri dev
```

Available production builds:

| Command                                     | Output                                            |
| ------------------------------------------- | ------------------------------------------------- |
| `npm run build:linux` (or `build:appimage`) | `artifacts/WiCReplayViewer-linux-x86_64.AppImage` |
| `npm run build:windows`                     | Windows executable when run on Windows            |
| `npm run build:windows-cross`               | Local MSVC-target executable built from Linux     |
| `npm run tauri build -- --bundles app,dmg`  | Local macOS application and DMG                   |

For distributable Linux builds, use Ubuntu 22.04 with the Tauri prerequisites,
`objcopy` (binutils), `patchelf`, and the FUSE 2 library (`libfuse2`). Building on a
newer distribution can raise the minimum system-library versions required by the
AppImage. Direct Fedora bundling is not supported: the bundled `linuxdeploy`
strip tool cannot read Fedora’s newer library sections. Hosted release builds use
Ubuntu 22.04; see
[Tauri's AppImage guidance](https://v2.tauri.app/distribute/appimage/).

macOS is available as a best-effort local source build rather than a routine
release artifact. On an Apple Silicon Mac, the final command above produces the
application and DMG. The build is ad-hoc signed, not Developer ID signed or
notarized, so Gatekeeper may require manual approval under **System Settings →
Privacy & Security** before the first launch.

The Linux cross-build is intended for local testing. Published Windows artifacts
are built on Windows by GitHub Actions. Cross-building requires `cargo-xwin`, the
`x86_64-pc-windows-msvc` Rust target, and the `clang-cl`, `llvm-lib`, `llvm-rc`, and
`lld-link` host tools.

### Running an AppImage build

After building `WiCReplayViewer-linux-x86_64.AppImage`, make it executable and run it:

```bash
chmod +x WiCReplayViewer-linux-x86_64.AppImage
./WiCReplayViewer-linux-x86_64.AppImage
```

No installation is required. Linux release builds use
Ubuntu 22.04 for x86_64 Linux; a graphical desktop is required. If your system
cannot mount AppImages with FUSE, run it with `--appimage-extract-and-run`.
The app uses your normal user permissions to access replay and game folders.
Native file pickers and error-report dialogs default to dark to match the viewer;
explicit `APPIMAGE_GTK_THEME` or `GTK_THEME` overrides are respected. Updates are
manual: download the next published AppImage and replace the previous file. No
APT or DNF update repository is currently provided.

### Linux rendering policy

On Linux, the npm launcher and native entry point default to `GDK_BACKEND=wayland`
and shared-memory WebKit transport. This uses native Wayland for the viewer and
its WebKit process, including direct binary launches. Set
`WIC_REPLAY_VIEWER_GDK_BACKEND=x11` to use X11 explicitly if Wayland rendering
fails on a particular desktop.

## Technology

| Layer                | Technology                                                                                         |
| -------------------- | -------------------------------------------------------------------------------------------------- |
| Desktop application  | Tauri 2 with the platform system webview                                                           |
| Frontend             | Vue 3, TypeScript 6, Vite 8, and Tailwind CSS 4                                                    |
| Native core          | Rust, 2024 edition                                                                                 |
| Replay parser        | Embedded Rust workspace crate with Python reference tooling                                        |
| Storage              | SQLite through bundled `rusqlite`                                                                  |
| Import concurrency   | Rayon with a bounded import coordinator                                                            |
| Large-list rendering | TanStack Vue Virtual                                                                               |
| Icons                | Lucide Vue plus the viewer's game-role assets                                                      |
| Verification         | Vitest/Vue Test Utils, V8 and LLVM coverage, TypeScript, ESLint, Prettier, Cargo, Clippy, and Ruff |
| Packaging            | Linux AppImage and Windows portable executable                                                     |
| Automation           | GitHub Actions with synchronized version and release checks                                        |

The Vue webview never reads replay files or opens SQLite directly. Typed Tauri
commands connect it to a reusable native Rust library that owns replay parsing,
storage, map assets, playback projection, and file-management operations. See
[ARCHITECTURE.md](../ARCHITECTURE.md) for the process boundary and reliability
invariants.

## Verification

Use the gate appropriate to the work:

| Gate     | Command                        | Evidence                                                                                              |
| -------- | ------------------------------ | ----------------------------------------------------------------------------------------------------- |
| Fast     | `./scripts/quality.sh fast`    | formatting, lint, types, schema/cache and version contracts                                           |
| Portable | `./scripts/quality.sh full`    | all redistributable tests, coverage floors, production build, formatting, types, and lint checks      |
| Coverage | `./scripts/coverage.sh`        | standalone frontend V8 and single-pass workspace LLVM coverage check                                  |
| Private  | `./scripts/quality.sh private` | optional installed-game, selected private replay, 17-fixture ground-truth, and Quarry playback checks |
| Corpus   | `./scripts/quality.sh corpus`  | occasional explicit comparison of all 2,880 linked replay inputs with the tracked aggregate baseline  |

Run the portable repository gate before publishing a change:

```bash
./scripts/quality.sh full
```

It checks synchronized product metadata, parser/cache compatibility, Python and
frontend tests, type-aware ESLint rules, TypeScript, Prettier formatting, production
frontend output, Rust formatting and tests, V8 and per-crate LLVM coverage floors,
and Clippy with warnings denied. The locked npm tree is installed once, and Rust
coverage instruments the complete workspace in one pass while allowing Cargo to
reuse valid instrumented artifacts.

The Python checks use Python 3.14 and Ruff 0.16.3 (the tracked development pin).
If these are not your shell defaults, select their absolute paths explicitly;
macOS system Python 3.9 is too old for the research tests:

```bash
WIC_PYTHON="$HOME/.venvs/wic-analysis/bin/python" \
WIC_RUFF="$HOME/.venvs/wic-analysis/bin/ruff" \
  ./scripts/quality.sh full
```

The gate canonicalizes its temporary-directory root so macOS `/var` aliases agree
with the importer's canonical paths. Portable tests create their own synthetic
fixtures and do not require game binaries, Ghidra, or the private replay corpus.

Apply or check frontend, configuration, and documentation formatting directly with:

```bash
npm run format
npm run format:check
```

Run or automatically fix the focused TypeScript, Vue, and JavaScript lint rules with:

```bash
npm run lint
npm run lint:fix
```

The lint policy requires braces around every control-flow body, including one-line
guard clauses, so later edits cannot silently escape the intended condition.

Private tests are marked `ignored`, so portable Cargo output names them explicitly
instead of reporting a false pass. Invoking one without configuration fails. Copy
`.env.example` to the gitignored `.env` and set the two machine-local roots; replay
fixtures are resolved under `local/` through the tracked relative manifest in
`tests/private-fixtures.json`:

```bash
WIC_GAME_INSTALL="/path/to/World in Conflict"
WIC_CUSTOM_MAPS="/path/to/Documents/World in Conflict/Downloaded/maps"
```

Set `WIC_DATA_ROOT` and run `./research/scripts/bootstrap-local-data.sh` to
create the ignored replay links for the original evidence layout before running
the private gate. The tracked fixtures identify that corpus, which is not
included in Git. A contributor with other replays can run the full portable
gate, pass replay directories directly to research scripts, and set
`WIC_GROUND_TRUTH_FILE` plus `WIC_REPLAY_ROOT` for a separate parser regression
set. `WIC_PRIVATE_FIXTURES_FILE` selects a different relative fixture manifest,
but the repository's strict private gate still includes regressions for the
original fixtures. See the
[research setup](../research/README.md#quick-start).

The private gate validates every configured directory and manifest entry before it
runs and requires all 17 parser ground-truth fixtures. It does not traverse the full
corpus. Run that slower audit separately when parser or corpus-wide behavior warrants
it:

```bash
./scripts/quality.sh corpus
```

The corpus gate compares all linked inputs with
`tests/corpus-aggregate-baseline.json`; neither replay bytes nor absolute paths enter
that baseline. Regenerate it only as an explicit reviewed operation:

```bash
target/release/wic_replay_parser --corpus-summary-json \
    local/replays/main local/replays/settings local/replays/wicgate-documents \
  | python3 scripts/verify-corpus-baseline.py \
      --baseline tests/corpus-aggregate-baseline.json --update
```

The full importer path has a separate optional ignored regression that copies a
deterministic 32-replay sample into temporary storage. It is intentionally not part
of either standard opt-in gate.

Frontend coverage requires 85% lines, statements, and functions and 75% branches.
LLVM line floors are 85% for the parser, 80% for the native viewer core, and 75% for
the Tauri runtime-independent command core. The framework adapter in
`src-tauri/src/lib.rs` and native supervisor/widget adapters in
`src-tauri/src/error_helper*` are explicitly outside that metric; the portable helper
monitor in `src/diagnostics/helper.rs` remains covered. Private-only inline
regressions are likewise omitted from portable instrumentation while remaining
visible as ignored tests in portable Cargo output. `scripts/coverage.sh` requires pinned
`cargo-llvm-cov 0.9.0`; generated reports remain ignored. The local pre-push `full`
gate invokes it. Hosted pull-request and `main` checks run the same Rust tests and
strict Clippy gate without repeating the full LLVM instrumentation pass; release
publication requires both hosted quality jobs to have succeeded for the tagged
commit. Hosted jobs cache Rust downloads and reusable build outputs plus
content-addressed frontend analysis by repository, toolchain, target, and lockfile.
Vue TypeScript checks reuse compiler-managed incremental metadata.

The parser also has its own ground-truth manifest and verification instructions in
[parser/README.md](../parser/README.md).

## Versions and releases

The viewer and embedded parser ship as one versioned product. `package.json` is the
product-version source; Tauri reads it directly, and release preparation synchronizes
the Cargo, lockfile, and changelog copies. The parser crate retains its
historical `6.0.0` identity but is unpublished and is not versioned separately.
Parser JSON schemas and the viewer database schema are independent compatibility
identifiers.

Do not edit version fields during ordinary development. Record user-visible work
under `Unreleased` in [CHANGELOG.md](../CHANGELOG.md). To preview the next release
without writing files:

```bash
npm run release:prepare -- minor --dry-run
```

Run `npm run release:prepare -- patch|minor|major` only from a clean `main`
worktree that is ready for release. The command updates metadata and runs the full
quality gate, but it does not commit, tag, push, or publish. A matching pushed `v*`
tag builds the Linux AppImage and Windows portable ZIP, then creates a draft GitHub
Release from that version's short `Highlights` changelog section for manual review.
Keep technical details in the remaining changelog sections. macOS builds remain
available locally but are not produced by hosted release automation.

## Dependency notices

After updating dependencies, regenerate `THIRD_PARTY_NOTICES.txt` with
`npm run licenses:generate`. This performs a clean locked npm install, verifies
Cargo crate archives against `Cargo.lock`, and rejects dependencies whose licence
expression or notice source has not been reviewed.

## Research setup

See [research/README.md](../research/README.md) for analysis environments, evidence
links, and Ghidra setup. Private evidence and generated research output stay under
ignored `local/`. There is no parent checkout or submodule update step.
