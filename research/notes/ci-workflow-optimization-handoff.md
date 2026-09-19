# Handoff — CI workflow timings

Continuation brief for a fresh Codex session. Updated 2026-09-04, with the
viewer workflow work consolidated at `b733cb5`.

This is a historical timing snapshot, not a current release-status report.
Its measurements and unverified items apply to that revision. Current hosted
artifacts are Flatpak and Windows portable; consult
`components/replay-viewer/.github/workflows/build-desktop-artifacts.yml` for the
active jobs. Script paths below are relative to `components/replay-viewer/`
unless explicitly identified as parent-workspace paths.

Everything described here is pushed. Nothing is in progress.

**Read the caveats in "Claims you should not inherit" before acting on any number
in this document.** The previous session reported several conclusions confidently
and then retracted them; the retractions are recorded there so they are not
rediscovered or, worse, re-trusted.

## Original problem

Two releases (`v0.3.0`, `v0.3.1`) were slow. The reported pain was release
duration; the push gate came up later, in conversation, and was not the initial
complaint.

Measured baseline for `v0.3.1` (run `33752783194`):

| Job                    | Time    |
| ---------------------- | ------- |
| Windows x64 portable   | 17m43s  |
| macOS Intel            | 15m06s  |
| Linux Flatpak          | 9m33s   |
| Linux DEB and RPM      | 9m17s   |
| macOS Apple Silicon    | 7m09s   |
| quality (gated all)    | 3m26s   |
| **Total wall clock**   | **21m29s** |

Push gate (`Portable quality`) baseline: **201s**, of which ~105s was environment
setup (57s Rust cache restore, 48s apt) and 73s actual work.

## Root cause, and the evidence for it

GitHub scopes Actions caches per ref: a run reads its own ref and the default
branch, nothing else. The desktop build jobs only ran on `v*` tags, so each
release wrote ~1.7 GB of Rust cache into a tag scope no later tag could read.

Evidence, not inference:

- `gh cache list` showed the identical key
  `v0-rust-windows-x64-release-Windows_NT-x64-76eb36f4-e3643531` under both
  `refs/tags/v0.3.0` and `refs/tags/v0.3.1`.
- The v0.3.1 Windows job log contains `... Restoring cache ...` then
  `No cache found.`, followed by ~500 crates compiling.
- 848s of that job's 1063s was the build step; a further 99s was spent *saving* a
  cache into the unreadable tag scope.

## What changed

Viewer (`components/replay-viewer`):

- **`b733cb5`** — Consolidates the complete workflow optimization. Release
  builds warm their native Rust caches on `main` and tag jobs do not save
  unusable tag-scoped caches. The push gate is split into parallel portable and
  Rust jobs, with the Rust half path-filtered and its cache refreshed by the
  scheduled warmer. Hosted Rust CI runs locked tests and strict Clippy, while
  full LLVM coverage remains in the local `full`/pre-push gate. Release
  publication requires both named quality jobs to have succeeded for the exact
  commit. Linux packages are installed by ordinary fail-closed `apt-get`, and
  the rotated Rust cache retains workspace build artifacts. The commit also
  contains the workflow contract tests and concurrency controls. It replaces
  the implementation/repair sequence and omits the abandoned v0.3.2 preparation
  and its matching revert without changing the final tree.

Parent (`wic-re`):

- The post-v0.3.1 parent changes are consolidated into one delivery commit. Its
  `scripts/quality.sh` no longer reruns the child's full gate for a Gitlink-only
  advance when the submodule is clean and its exact HEAD is already published
  on the child's `origin/main`. Dirty, unverifiable, or unpushed states fail
  closed to the full child gate; `WIC_FORCE_REPLAY_QUALITY=1` forces it. The Git
  hook environment is stripped before submodule inspection so parent
  `GIT_DIR`/`GIT_INDEX_FILE` values cannot produce a false clean result. The
  same commit records the final viewer Gitlink and this handoff.

## Verified vs unverified

Verified by measurement:

- Release caches now exist on `refs/heads/main` with keys byte-identical to the
  ones that missed in v0.3.1. Confirmed via `gh cache list`.
- Push gate, no Rust changed: **75s** wall (9s `changes`, 64s `portable`, Rust
  skipped). Run on `08d9097`. Later portable-job samples were **56s** and
  **68s**, with the change detector normally taking 5–6s.
- Push gate, Rust changed, warm corrected cache: the Rust job took **2m44s**.
  Its actual quality gate took **58s**, cache restore **42s**, and fail-closed
  WebKit/GTK installation **50s**. The preceding one-time cache-population run
  took **6m37s**. Run `33822607727`, attempts 1 and 2.
- Parent pre-push on a clean, pushed submodule: **0.77s**, down from minutes.
- `full` still passes locally with V8 coverage, single-pass workspace LLVM
  coverage, all Rust tests, and Clippy warnings denied (14.9s warm on the final
  cache change).

Not verified:

- **The release path has never run end to end since the change.** The 21m29s →
  ~10m estimate is inference from the cold/warm cache difference, nothing more.
  It measures itself on the next real tag. Do not quote a release number until
  then.
- Flatpak and the complete release path remain unmeasured after these changes,
  by explicit user direction not to create another release yet.

## Claims you should not inherit

The previous session got these wrong before correcting them. Recorded so the
errors are not repeated or the stale versions believed:

1. **"Rust-touching pushes regressed by ~25%."** Reported as fact, then
   retracted. In the same runs, `npm ci` went 3s → 7s and the vite build 586ms →
   920ms — work that touches neither Cargo nor anything that changed. The runners
   were roughly 2x slower hardware behind the same `ubuntu-24.04` image label.
   Cross-run timing comparison on GitHub's shared runners is unreliable at this
   resolution. Treat the 237s/257s figures as uninterpretable, not as a
   regression. If you need this answered, you need same-runner A/B, which GitHub
   does not offer; the honest alternative is many samples.
2. **"Exceeding the 10 GB cache cap could evict the useful `main` caches."**
   Wrong. GitHub's documented policy evicts "in order of last access date, from
   oldest to most recent" — LRU. The warmed `main` caches are re-accessed on
   every release and every cron run, so they sit at the back of the queue. The
   stale tag-scoped caches are first to go. No cleanup is needed.
3. **"Switch the release profile to thin LTO."** Initially recommended, then
   withdrawn, and **not applied**. Measured locally on a 16-core machine, leaf
   crate plus link only: `lto = true` + `codegen-units = 1` is 72s wall / 69s
   user (fully serial), 9.6 MB binary; `lto = "thin"` + `codegen-units = 16` is
   19.5s wall / 140s user, 11.4 MB binary. On CI's ~4-core runners the win
   shrinks to roughly 70s → 40s per job. The reason for withdrawing: it is a
   permanent user-facing cost (19% larger binary, less optimised parsing in a
   tool that processes 2,880-file corpora) traded against a once-per-release CI
   cost that the cache fix already addressed. `Cargo.toml` is unchanged. Decide
   this on its merits; do not treat either position as settled.

## Known gaps

- **Flatpak is untouched and is likely the new release ceiling.** It took 9m33s
  and builds inside the flatpak-builder sandbox, so none of the Rust caching
  helps it. `scripts/build-flatpak.sh` deliberately uses `--disable-cache`
  because the manifest builds from a local directory source; enabling it without
  a proven invalidation design risks packaging stale source. Unmeasured.
- **Rust-changing pushes remain above two minutes end to end.** The warm quality
  work itself is under one minute. The remaining fixed costs are the large cache
  extraction and fail-closed WebKit/GTK installation. Going materially lower
  would require a maintained prebuilt runner image or reducing what hosted CI
  validates; neither was introduced here.
- **Pinned upstream actions emit Node 20 deprecation annotations.** GitHub runs
  them on Node 24 successfully. This is an upstream action-runtime warning, not
  a failing or slow project step, but future pinned action updates should remove
  it when those releases are available and reviewed.

## Guards that exist, and how to check they still bite

Three test files encode the invariants. All were mutation-tested when written —
that is, deliberately broken to confirm they fail. Re-do that rather than
trusting them:

- `scripts/packaging.node.mjs` — asserts the warm workflow cannot drift from the
  release jobs' runners, shared keys, or build commands; that no release job
  saves a cache from a tag ref; that the push-gate cache refresh job exists; and
  that Cargo paths remain in the Rust filter.
- `scripts/quality-split.node.mjs` — executes `quality.sh`'s own stage selection
  (`WIC_QUALITY_PRINT_PLAN=1`) for each mode and asserts `ci-portable` plus
  `ci-rust` cover exactly what `full` covers. It reads the real logic rather than
  a copy, so it cannot drift.
- The parent hook's fallback must be exercised **under a hook environment**, not
  a plain shell. That is precisely how the `5429686` bug escaped:

  ```bash
  cd /path/to/historical-wic-re
  touch components/replay-viewer/src/__probe.ts
  env GIT_DIR="$PWD/.git" GIT_INDEX_FILE="$PWD/.git/index" ./scripts/quality.sh fast
  # must run the child gate, not print "skipping its gate"
  rm -f components/replay-viewer/src/__probe.ts
  ```

## Useful commands

```bash
# Per-job and per-step timings for a run
gh run view <run-id> --json jobs \
  --jq '.jobs[]|"\(.name): \(.startedAt) -> \(.completedAt)"'

# Cache inventory by ref — the check that exposed the original bug
gh cache list --limit 100 --json key,ref,sizeInBytes \
  --jq '.[]|"\(.ref)\t\(.sizeInBytes/1e6|floor)MB\t\(.key)"' | sort

# Whether a job's Rust cache actually hit
gh run view <run-id> --log --job <job-id> | grep -iE "Cache hit|No cache found|full match"
```

## Suggested order of work

1. Do not create another release merely to benchmark it. Measure the release
   path when the user next chooses to cut a real tag.
2. If Flatpak dominates then, investigate a safe content-addressed source/cache
   design before removing `--disable-cache`; stale release contents are worse
   than a slow build.
3. Treat a prebuilt Linux CI image as the remaining push-path optimization only
   if the maintenance and supply-chain cost becomes worthwhile.

The user's stated targets: push gate 1–2 minutes (met for non-Rust commits; warm
Rust work is 58s but its complete hosted job is 2m44s), release accepted as
inherently longer because it builds all packages.
