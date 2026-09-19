# Changelog

Notable changes to the `wic-re` orchestration and reverse-engineering workspace
are recorded here. Active component implementation history belongs in the unified
`wic-replay-viewer` repository; current proxy development belongs in `wicgate`.

## 2026-09-09

### Changed

- Remove the obsolete standalone proxy submodule. Current proxy development lives
  in the separate `wicgate` repository. Narrow automatic submodule merges to the
  replay-viewer Gitlink and update workspace setup and contribution instructions.

### Added

- Audit the one-sided result fallback across 2,499 distinct replay contents in
  Flatpak: 43 restored opposing rosters, unchanged acceptance, and no teams above
  eight players. Retain reproducible comparison probes.

- Add a read-only Flatpak comparison of replay end-screen tables and current
  occupants, checked against the previous viewer audit and an independent Python
  decoder. Document the game-code evidence in the viewer's final-screen report.
- Record the eight final-screen fallback contents with SHA-256 identities, library
  aliases, and the distinction between unavailable primary results and incomplete
  player identity.

## 2026-09-08

### Added

- Add reproducible Flatpak corpus comparison and independent Python checks for
  explicit departure markers and the viewer's eight-player overflow policy.

- Add a SHA-256-keyed before/after audit and independent Python verification for
  targeted event-backed score, identity, and spectator corrections across the
  4,089-path replay library. Implementation history lives in the viewer component.

- Comparison-only event-roster probes and a 4,089-path Flatpak audit against the
  published viewer. Document snapshot/participation trade-offs, remaining roster
  candidates, and explicit-score discrepancies without changing product behavior.
  Ignore bulk JSONL audit evidence alongside existing JSON outputs.

## 2026-09-07

### Changed

- Preserve the declined missing-role extension investigation and its three replay
  inspection scripts. Make the historical audit input explicit, include the entry
  decoder, and record the later in-game playback failure reported for the
  byte-identical CG's epic fail / demo17 fixture. No parser behavior changes.

- Standardized the research workspace on its existing host toolchain. Removed
  the optional container image recipe, Compose and devcontainer configuration,
  build-context filters, and container launcher. Documented host setup and
  verification while retaining the Ghidra launchers, separate Python dependency
  pins, and evidence checks. The environment verifier now selects the host
  analysis environment and lets Cargo fetch locked dependencies when needed.
- Refreshed workspace guidance for the existing unified releases, conditional
  parent quality gate, and optional private/WASM checks. Retired the obsolete
  AppImage task, corrected the viewer license reference and child publication
  order, and distinguished historical research/CI notes from current instructions.

## 2026-09-04

### Changed

- Stopped the parent pre-push gate from re-running the replay product's full
  quality gate for a commit that only advances the submodule Gitlink. That
  re-tested code the child's own hooks and its portable CI had already gated,
  costing minutes per push. The parent now runs its own checks and verifies the
  pointer instead: the submodule must be clean and its commit already on the
  child's `origin/main`, which is the documented rule worth enforcing here. A
  dirty or unpushed submodule still falls back to the full child gate, and
  `WIC_FORCE_REPLAY_QUALITY=1` forces it. Measured 0.77s in place of minutes.

## 2026-09-03

### Changed

- Aligned the isolated workspace container with the replay product's pinned Rust
  1.98.0 toolchain so local, container, and hosted quality gates use the same
  compiler and Clippy lint set.

## 2026-09-02

### Added

- Documented the original client's match-summary score ordering from the b35
  executable. The screen sorts each score field independently across 16 slots with
  a deterministic, unstable score-only quicksort, so tied leaders are selected
  arbitrarily rather than by total score, name, faction, or a consistent slot
  preference. A hashed real replay supplies a three-way tie regression vector.

### Changed

- Added the replay product's pinned, type-aware ESLint gate to the workspace's fast
  pre-commit verification and documented its focused TypeScript, Vue, and JavaScript
  coverage alongside the existing formatter and compiler checks.

- Updated the workspace hook documentation for the replay product's pinned Prettier
  gate. Fast pre-commit verification now covers frontend and documentation formatting
  plus strict Vue TypeScript checks alongside Ruff and rustfmt; the full pre-push gate
  continues through the locked test suites, production frontend build, and Clippy.

## 2026-09-01

### Added

- Added research-only tactical-aid projectile catalogue decoding, strict wire
  validation for all four support creation families, compound lifecycle tracking,
  float32 straight/ballistic free-flight stepping with explicit tick inputs, and a
  full-corpus effect inventory. The exact-or-abstain report documents that replay
  data lacks per-frame deltas, collision results, and projectile IDs on effect
  records, so no impact or death attribution is emitted; the investigation is
  closed at the replay/static boundary because runtime capture is out of scope.

## 2026-08-31

### Changed

- Unified the replay parser and native viewer in `wic-replay-viewer`, with parser
  source under `parser/`. The parser's complete rewritten history is connected to
  the viewer graph, its former sync branch is retained by an archival tag, and a
  tracked commit map documents every original-to-imported parser commit.
- Removed the standalone parser Gitlink from this orchestration repository and
  redirected quality gates, hooks, container dependency prefetching, corpus tools,
  and documentation to the unified replay product. Dependabot now watches only the
  viewer and proxy Gitlinks.
- Preserved the experimental in-game playback line on `dev`; stable `main` remains
  free of that experiment. The independent `wic-proxy` component was not changed.
- Defined one semantic-version lineage for the unified replay product, continuing
  the viewer's initial `0.1.0` and Tauri/Vue `0.2.0` application versions into a
  first formal unified `0.3.0` release. Parser and database schema numbers remain
  compatibility identifiers and parser crate history remains distinct. The viewer
  now owns a synchronized, release-time bump command plus tag/version CI enforcement
  and draft GitHub Release creation, so ordinary development does not depend on
  remembered version edits.

### Fixed

- Align Docker-compatible build-context filtering with the unified replay product;
  both `.dockerignore` and `.containerignore` now include the viewer workspace and
  embedded parser instead of the removed standalone parser path.

## 2026-08-29

### Changed

- Removed automatic `main`-to-`dev` synchronization. The branches are now
  reconciled manually only when `dev` is actively in use, avoiding guaranteed
  GitHub merge failures when parent Gitlinks intentionally reference divergent
  component branches. Manual reconciliation remains child-first: merge and push
  component branches before recording their exact parent Gitlinks.

## 2026-08-27

### Added

- Added an isolated replay-state reconstruction experiment: a versioned,
  raw-word-preserving decoder and corpus audit for unit checkpoints, lifecycle,
  health, movement commands, objectives, and tactical-aid positions; an optional
  user-install map extractor; a dependency-free Canvas playback proof; and a
  hash-gated Frida runtime oracle with offline bit-exact comparison. The proof
  explicitly holds the last serialized checkpoint instead of presenting inferred
  interpolation as replay truth. The native viewer now exposes that stream through
  a lazy Replay tab without changing the canonical parser schema.

### Fixed

- Bounded the replay viewer's independent development scanners. Vite's dependency
  optimizer uses the single application HTML entry instead of a repository-wide
  glob; its watcher excludes Cargo `target/` trees and does not follow symlinks;
  `.taurignore` excludes build and package trees from Tauri's Rust watcher; and
  Tailwind scans only frontend sources. This prevents a Wine prefix's `z:` drive from
  traversing the host and `/proc`, removing the associated `EINVAL` crash, SELinux
  denial, multi-gigabyte optimizer run, and prolonged white window. Startup reads
  cached SQLite summaries and does not parse replay folders until an explicit scan.

## 2026-08-24

### Added

- Extended the replay viewer's map art to community maps and automatic folder
  detection, and fixed the lookup key. Replay summaries store the map as
  `maps/<name>/<name>.ice` rather than as the bare internal name the art is keyed
  by, so every lookup missed and every row silently kept its procedural tile; the
  handoff note asserted the opposite and is corrected in place. Downloaded maps
  under `Documents/World in Conflict/Downloaded/maps` are now a second art source,
  supplying 34 of 63 overviews on the development library. Both folders are
  detected on first run across Steam, GOG, Ubisoft, Wine, and Proton layouts, with
  manual selection always available.

- Completed the map overview art work end to end. The replay viewer now draws each
  map's real overview image, read at runtime from the user's own game installation
  and cached locally; no game art is bundled or redistributed, and the installation
  is only ever read. The SDF reader and codec-3 decoder are ported to the viewer's
  Rust side, verified byte-for-byte against `scripts/wic_sdf.py` on all 32 shipped
  entries. Archive precedence is settled as higher-number-wins, grounded in decoded
  image comparison rather than an engine mount-order trace; that caveat is recorded
  in the plan note. Both map overview art notes are marked complete.

- Recovered client SDF codec 3 from `wic.exe` and extended the read-only
  `scripts/wic_sdf.py` inspector with its exact 128-byte auxiliary-prefix plus
  three independent zlib-stream layout. Added a synthetic codec-3 archive test and
  `scripts/map_overview_art_audit.py`; all 32 shipped `overviewmap.dds` entries
  decode to byte-exact valid DXT1 payloads. Recorded per-entry hashes and the three
  base/patch visual comparisons in
  `findings/map-overview-art-codec-and-precedence-2026-08-24.md`. Verified the
  decoder generalises beyond those 32: all 10,032 codec-3 entries across the 26
  shipped archives validate at open, and a 1,171-entry sample decodes byte-exactly
  across DXT1/DXT3/DXT5 and uncompressed formats from 8 x 8 to 2048 x 2048. The
  two file-size
  classes are both 256 x 256 and differ by mip count, correcting the earlier
  dimension hypothesis. Later numbered archives visibly revise all three duplicate
  maps, supporting highest-numbered precedence as a recommendation pending the
  user's decision. No viewer code or bundled art changed.

- Rewrote both map overview art notes around runtime loading. The viewer will read
  `maps/<mapName>/overviewmap.dds` from the user's own game installation and cache
  the decoded image locally, rather than extracting the 32 images once and shipping
  them. Extraction onto one's own machine is unremarkable; committing Massive's
  textures or baking them into the distributed deb, rpm, Flatpak, and Windows
  bundles is redistribution, so the approach removes the licensing question instead
  of answering it. The cost is that the codec-3 decoder must be ported to the
  viewer's Rust side, with `scripts/wic_sdf.py` remaining the read-only reference
  implementation. Archive precedence stays the one open decision.

- Recorded `notes/map-overview-art-handoff.md`, a continuation brief for a fresh
  session taking the map art work through phases 1-5. It names the codec-3 decode
  as effectively the whole of the risk, fixes the correctness bar for a decoder as
  a full 32-entry round trip with valid DDS dimensions rather than plausible bytes,
  marks the licensing and archive-precedence calls as the user's to make, and
  records the viewer integration seam and the constraints already settled there.

- Recorded `notes/map-overview-art-extraction-plan.md`. A read-only
  `scripts/wic_sdf.py` listing over `binaries/game/wic*.sdf` locates the game's
  top-down map art at `maps/<internal_name>/overviewmap.dds` for 32 maps, keyed by
  the exact internal name the parser already reports, so no translation table is
  needed. All 32 are stored with the segmented codec 3 that the tool lists but does
  not decode, which blocks extraction behind recovering that codec from `wic.exe`.
  The note also records the three maps duplicated across base and patch archives and
  leaves both the archive-precedence rule and the asset-licensing decision open. The
  viewer ships a name-seeded procedural tile as the fallback in the meantime.

- Completed Phase 10 and rejected the `buildingDamage` mechanical context. The
  building damage handler `0x004da180` records its attacker at building `+0x10c` but
  passes a literal null killer object to the resident splash `0x00517f10`, so
  `0x004ce830` selects sentinel `0x200` and a synthetic terminal direction. The
  attacker is discarded by design, so the context could never have carried an actor,
  and a genuine splash death must be sentinel-synthetic. A 2,880-replay audit with
  zero failures shows the same-raw-tick residency rule firing 2,476 times on classes
  that provably cannot be building splash against 1,569 where it could be right, so a
  majority of its matches are false by construction. Rejected on determinism and on
  yield; added `scripts/building_damage_context_audit.py`.

- Completed Phase 9 and refuted the sentinel-plus-directional support hypothesis, the
  largest remaining unattributed unit-destruction class. `EXG_Unit::Kill`
  (`0x004ccef0`, `.\EXG_Unit.cpp:0x4e0`) asserts `aKillerUnit <= EX_MAX_UNITS` and
  gates both scoring calls on `killerId < 0x200`, so the sentinel `512` is the
  engine's own encoding for "no owning unit" rather than a lost value.
  `EXG_BlastContainer` (`0x004f2b50`) passes that literal sentinel while supplying a
  real world position, and its blasts are enqueued only from `.\EXG_Death.cpp`
  (`0x00513450`, reading the shipped `myDamage` and `myBlastRadius` properties) and
  drained one per tick by `EXG_BlastContainer::Update`. Death explosions therefore
  provably produce sentinel-killer directional deaths, so that class is not a
  tactical-aid signature. A 2,880-replay scan with zero failures confirms the
  predicted temporal signature (50.4% same-tick co-death against a 10.2% attributed
  control, 4.93x) with a clean negative control in the synthetic-direction class
  (59.9% against 62.1%). `UnitDestroy` serializes no position, so no per-death parent
  is recoverable and the population stays unattributed in the viewer.
- Added `scripts/wic_source_map.py`, a deterministic address to compilation-unit map
  for the shipped binaries built from assert-string push sites and rel32 call
  targets, requiring no Ghidra code indexing, and
  `scripts/death_explosion_chain_scan.py` for the corpus adjacency measurement.
- Completed Phase 8 forced-death caller research and closed the last two bounded
  binary questions in the unit-destruction investigation. A whole-image pointer scan
  proves the fatal-path caller lists are exhaustive, and RTTI plus shipped ICE data
  identify all six forced-death callers: the spawner branch is unreachable because no
  multiplayer unit type has zero maximum health, `EXG_SelfDestruct` appears on no
  playable unit, `EXG_Blower` never fires in 2,880 replays, `EXG_Game` slot 4 is the
  debug/cheat interface, `WICG_FortificationPoint` unfortify is a typed fortification
  loss, and only the unserialized `WICG_Bridge`/destroyable-path kill box remains
  live. The blink scheduler's reason byte at unit `+0x128` completes the
  self-deletion split, but it is unserialized, defaults to the disband value, and its
  branch ends in `UnitRemove`. Phase 8 also corrects the terminal-direction signature
  to mean "fatal damage with no direction vector" after finding a third caller of the
  direction generator on the ordinary damage path. Added
  `scripts/forced_death_partition_scan.py`, which partitions all 1,168,735 raw corpus
  destructions by killer sentinel and direction signature with zero failures. No
  parser or viewer attribution changed.

- Completed Phase 7 support damage-cloud attribution research. Ghidra and shipped
  definitions establish the real category, relation, damage, lifetime, radius, and
  server-tick rules, while audit schema v13 applies them to all 2,880 replay paths.
  Broad cloud overlap reaches 78,014 remaining unknown deaths but also 71,063
  known-killer controls; the deterministic same-tick geometry/lethality rule recovers
  zero cases because replay victim frames are not current on the fatal tick. The
  final binary handoff bounds the central fatal routine to normal damage, forced
  death, and inherited resident/container death and records the six still-unclassified
  forced-death callers. No parser or viewer attribution changed from this
  candidate-only result.

- Documented deterministic building-collapse and destroyed-container lifecycle
  contexts for unit deaths. Ghidra proves that both server paths copy the initiating
  killer/damage-source state through the resident fatal call and emit a reproducible
  synthetic direction; exact occupancy/relation generations, same-tick terminal
  state, teardown ordering, and killer agreement bound production use. The
  schema-v18 parser corpus emits 14,723 building and 2,779 container contexts across
  2,865 valid replays, with zero context overlap. Only 67 container children inherit
  an independently exact tactical-aid cause; all other contexts leave attacker
  identity unchanged.

## 2026-08-23

### Added

- Advanced the replay viewer so genuinely unresolved destruction rows render as
  `UNIT LOST` with neutral direct loss prose. Proven player and tactical-aid
  causes remain `UNIT DESTROYED`; parser evidence and cache semantics are unchanged.

- Added unit-destruction audit schema v8 for the Phase 5 ordinary exact-target
  hypothesis. Removing schema v15's independent killer-unit equality produces
  2,654 wrong players among 19,364 unique-actor controls and four wrong teams
  among 19,708 unanimous-team controls, while reaching only 175 of 170,954
  remaining unknown complete-unit deaths. The bridge is rejected and canonical
  parser/viewer attribution remains unchanged.

- Documented the Phase 4 player-disband attribution boundary. The shipped action
  refunds selected units through a shared blink-before-death path, while an exact
  scan of 2,880 replay paths found zero serialized `DisbandUnit` requests. The
  replay-visible `SetState_Unit` transition is shared with combat and other unit
  lifecycle paths, so no unknown death was relabeled as a self-deletion.

- Added unit-destruction audit schema v7 for the Phase 3 support-projectile impact
  hypothesis. Constant-vector projection produced three wrong exact-control causes
  at the first materially useful 10-unit threshold and more errors at wider
  thresholds; tighter zero-mismatch thresholds recovered at most 98 of 15,642
  complete-unit controls and lack validated vector semantics. The bridge is
  rejected and canonical parser/viewer attribution remains unchanged.

- Extended the unit-destruction research audit to schema v6 for Phase 2 area-
  effect tracing. It now includes both explosion wire variants and measures the
  strict preceding/same-tick/radius-containing candidate rule. Across 2,880 replay
  paths, it narrowed 39,935 remaining unknown complete-unit deaths to plausible
  blast damage, but 11,118 have multiple blast candidates and the explosion wire
  carries no projectile, support, player, or team identifier. No parser or viewer
  attribution changed from this candidate-only result.

- Added unit-destruction audit schema v5 and the Phase 1 remaining-unknown census.
  After excluding infantry members, world/unknown ownership, non-sentinel killers,
  and schema-v17 exact TA deaths, 170,954 player-owned complete-unit deaths remain
  unknown. Of those, 120,913 have both ordinary and support projectiles nearby,
  demonstrating that temporal proximity cannot select a cause. The audit also
  inventories multi-player death clusters so future area-effect TA attribution
  preserves every affected player and unit rather than collapsing a strike.

- Implemented parser timeline schema v17 tactical-aid destruction attribution for
  the exact-target homing-support subset. The rule requires the target's active
  lifecycle, a projectile no more than three seconds before sentinel-512 death,
  one same-ID deployment team, and agreement across all qualifying projectiles.
  Ghidra confirmed the serialized support/target/team fields and server lethal-
  damage path; 302 known-killer controls had zero known-team contradictions. The
  2,865 accepted corpus replays now classify 16,328 of 713,051 complete-unit
  destructions as tactical aid and retain 213,539 as unknown. The viewer names the
  proven team and aid for attributed cases and renders unresolved cases as neutral
  unit losses without changing their raw `unknown` cause. Self-deletion remains
  unresolved because its replay-visible blink signature is not deletion-specific.

- Implemented parser timeline schema v16 infantry lifecycle separation. The 27
  explicit shipped multiplayer soldier definitions now emit into
  `infantrySoldierDeaths`, while `unitDestroyed` is reserved for complete units and
  squad-parent objects. The 2,880-replay corpus produced 451,465 soldier deaths and
  713,051 unit/squad destructions across 2,865 valid replays, with the same 15
  established rejects and no out-of-duration soldier timestamps.

- Started the sentinel-512 majority investigation with unit-destruction audit
  schema v4: decode compact/full `UnitFrame` positions, preserve persistence keys,
  inventory shooter-target and health records, measure projectile/explosion spatial
  controls, and aggregate sentinel victim types. Focused controls reject naive
  constant-vector attribution and identify infantry member/squad duplication as a
  high-coverage presentation lead.

- Implemented timeline schema-v15 parser attribution for the bounded exact-target
  homing-projectile subset of otherwise unresolved non-sentinel unit destructions.
  The rule preserves projectile-time ownership and victim lifecycle generation,
  rejects ordering/reuse/actor ambiguity, and leaves sentinel-512 deaths unknown.

- Extended the unit-destruction attribution audit with unit-ID lifecycle endings
  and historical-killer projectile evidence. The 2,880-replay pass found 9,062
  unresolved non-sentinel killers with one historical owner, including 2,886 with
  a nearby projectile from that killer unit and 209 whose homing projectile names
  the exact destroyed unit as its target. Ghidra confirmed the projectile
  firing-unit/target and destruction-killer field semantics, while 1,362 active-
  killer positive controls agreed with zero mismatches. The 209 exact-target cases
  are ready for a bounded parser implementation; sentinel-512 `cause unknown`
  events are unchanged.

## 2026-08-22

### Added

- Added `scripts/ranked_bucket_audit.py` and
  `findings/ranked-server-mode-hypothesis-2026-08-22.md`, resolving whether the
  unlabelled server-mode bucket means Ranked. It does: the residual left after
  FPM, Match Mode, clan, tournament, and bots is ranked games. No replay-side bit
  says so — enumerating the demo header across the corpus confirms it carries no
  spare boolean — so the conclusion is an elimination argument closed by operator
  ground truth that unranked public servers were not run in practice, and by
  ruling out client-hosted games: `WicData.myDefaultGameName` is the dedicated
  server's own fallback name, so the four `World in Conflict Game` replays are
  unnamed dedicated servers, not listen servers. Corpus support: all 573
  unlabelled replays are public games at default round length, one of 661 servers
  straddles the unlabelled and labelled buckets, and all 189
  revived-Massgate-era unlabelled replays come from ranked servers. The audit also
  recovered what Ranked means server-side — `wic_ds.exe` coerces a ranked server's
  mod, password, MinPlayers, time-limit multiplier, and bot mode, clears RankedFlag
  when `[ReportToMassgate]` is 0, and permits Ranked alongside Match Mode, clan,
  and tournament servers, so the inference runs one way only. `ranked` stays null
  in the parser contract; the label belongs in the viewer.
- Added `scripts/bintag_message_inventory.py` and
  `findings/bintag-message-inventory-2026-08-22.md`, completing the replay-writer
  coverage item. Walking the callers of `wic.exe:0x009240a0` enumerated 178 call
  sites and 169 distinct messages with zero unresolved names and complete field
  lists, replacing message discovery by replay scanning. It closes the
  tactical-aid actor question: only three of the 169 messages carry both a player
  and a tactical-aid identity, and the parser already consumes all three. It also
  found that messages are rate-limited per name before being written, so a
  throttled event is absent from every recording; confirmed the envelope timestamp
  is elapsed time from a writer origin, matching timeline schema v13; and
  corrected the countdown message name to `SetGameModeData_Float`.

### Fixed

- Migrated `findings/tactical-aid-manual-review-2026-08-17.md` to the schema-v13
  recording axis. Its 18 checklist times were countdown-derived, so after the
  timeline retiming the review harness matched zero markers and the manual review
  was blocked. Each marker's old timestamp was reconstructed from the replay's own
  countdown samples and re-matched on `(supportId, playerId, reconstructed time)`,
  agreeing to within 0.230 s on all 18 rows with only one row carrying any nearby
  alternative. The harness passes again and all 18 statuses remain unreviewed. The
  times are now on the axis the in-game player scrubs on, which should also settle
  the review's outstanding seek-timing concern.

- Fixed a dead entry in `scripts/wic_bintag.py`'s name table. It listed
  `SetGameModeDataFloat`, which hashes to `0x4eb1079c` and appears in no replay,
  so the countdown record resolved to a bare hex hash in every probe using that
  helper. The correct name carries an underscore. Validating the whole table
  against the 169-message writer inventory found this as its only wrong entry.

### Changed

- `scripts/quality.sh` now gates Python formatting as well as linting
  (`ruff format --check scripts`), matching what the replay-parser submodule
  already does. Only `ruff check` ran before, so two audit scripts drifted
  unnoticed; both are reformatted and the whole tree is clean. The check runs in
  `fast` mode too, so the pre-commit hook catches drift at commit time.

- Expanded `scripts/wic_bintag.py`'s `KNOWN_NAMES` from a hand-picked sample to
  the exhaustive 169-message list from the writer inventory, so an unresolved
  message hash now means the client does not write that record at all.

- Added `scripts/multi_pov_attribution_audit.py` and
  `findings/multi-pov-tactical-aid-attribution-2026-08-22.md`, validating that
  aligning multiple recordings of one match recovers the opposing team's exact
  tactical-aid players. A deployment's serialized `(supportId, position, team)`
  triple is bit-identical across recordings, so the join is exact and needs no
  timestamp alignment. Across 22 confirmed multi-POV groups and 4,222
  deployments, attribution rose from 45.9% to 71.9% with 0 ambiguous results, 0
  contradictions, and 826 independent cross-confirmations. 1,185 deployments
  produced no marker in any point of view and remain unknown.

- Added `scripts/envelope_clock_audit.py`, the read-only corpus scan comparing
  the `Event` envelope clock against the game-mode countdown. Its 156-replay run
  over `replays/main` is the evidence behind replay-parser timeline schema v13:
  the envelope clock starts at `0.0` and advances monotonically in every replay,
  while the countdown starts a median of 66.8 s later (max 1321 s), restarts
  between Assault rounds, and ticks at 0.84-1.25 s of clock per second of
  recording, with one replay at ~2.4x. Sixty-two replays reported a match longer
  than the recording that observed it.

## 2026-08-21

### Added

- Added a Linux cross-build path for the viewer's portable Windows executable
  (`npm run build:windows-cross` in `wic-replay-viewer`). It requires the host
  packages `clang`, `llvm`, and `lld` for `clang-cl`, `llvm-lib`, `llvm-rc`, and
  `lld-link`, alongside `cargo-xwin` and the `x86_64-pc-windows-msvc` Rust target.
  Only `clang-cl` is a usual Linux development dependency, and this workspace's
  documented toolchain does not otherwise install the rest. The native Windows CI
  job remains the release path because a cross-linked executable is not
  bit-identical to it.

- Added `notes/domination-bar-and-match-timing.md`, recording corpus-verified
  `aFactor` semantics: POV anchoring and its mirror on team change, mode-dependent
  quantisation, why value-only mirror detection is unsound, the three match
  endings separable from the countdown, and the late-join distinction between
  match time and recording time.

- Added a read-only server-mode evidence audit over 2,880 linked replay paths. It
  validates serialized FPM and Match Mode header flags plus bot participant types,
  records that Ranked is absent from the demo header, and preserves the unique
  FPM-with-bots counterexample instead of forcing mutually exclusive categories.
- Added parser and viewer support for orthogonal nullable server classification:
  FPM, Match Mode, bots, clan matches, and tournament matches are derived only
  from replay evidence, while Ranked remains unknown. The viewer refreshes cached
  summaries, makes positive classifications searchable, and displays combined
  labels such as `FPM · Bots` without presenting unknown as Unranked.

## 2026-08-20

### Added

- Added a read-only 2,880-replay lifecycle audit proving that numeric player slots
  are reused by different named occupants in 983 replays and 2,525 slot instances.
- Added timeline schema-v11 half-open participant sessions reconstructed from exact
  `PlayerEntersGame` names and `PlayerLeavesGame` boundaries, while preserving the
  established static participant directory as a compatibility fallback.

### Fixed

- Closed the recurring native white-window failure by defaulting the Linux viewer to
  the visibly verified X11 GTK path in addition to shared-memory transport. The
  launcher overrides the desktop Wayland backend on the mixed 1.0x/1.5x monitor
  layout and includes a regression test plus a viewer-only Wayland escape hatch.
- Changed the viewer and tactical-aid playback preparation to resolve player names
  at each event time. Replay B's slot-1 strikes at 402.673 and 1069.512 now name
  `[WHO]LtDan73`, who replaced `[-HH-]JonnySky`, instead of carrying JonnySky's
  stale static identity through the rest of the match.

### Documentation

- Recorded that `WIC_SUBMODULES_TOKEN` must be scoped to every component repository,
  after the daily Dependabot job failed from 2026-08-16 onward because the token
  predates `wic-replay-viewer` and returned `403` for it. The parser and proxy
  submodules kept updating, so the whole job failed while that one component
  silently lost its fallback check.

## 2026-08-19

### Added

- Expanded the unit-destruction research audit into a schema-v2 temporal evidence
  ledger covering lifecycle ownership, schema-v10 timeline time, ordinary and
  support projectiles, deployments, active markers, explosions, and exact TA
  damage-threshold records without promoting candidates into kill attribution.
- Added a deterministic tactical-aid playback preparation tool that validates all 18
  manual-review rows against current parser evidence and reproduces the viewer's
  conservative 47/111-row projection for the two review replays.

### Fixed

- Diagnosed the replay viewer's blank Fedora window as a WebKitGTK DMA-BUF
  presentation failure after proving that Vue mounted and the populated render tree
  remained invisible. The earlier compositing-only fallback regressed on WebKitGTK
  2.52.5, and setting the replacement only from Rust `main()` proved too late for
  stable UI-side initialization. The npm launcher now forces WebKit's valid
  shared-memory renderer transport in Tauri's initial environment.

### Documentation

- Recorded the machine evidence, eliminated frontend/parser causes, normal launch
  behavior, quality results, override boundary, and future graphics-stack retest in
  `notes/replay-viewer-webkit-white-window.md` and the workspace TODO.

## 2026-08-18

### Added

- Added a deterministic 2,880-replay `SendTATaunt` and unit-destruction audit. It
  recovers exact tactical-aid actor, affected player, catalogue-resolved support,
  and upgrade while retaining same-tick unit deaths as candidates only.
- Added timeline schema-v10 tactical-aid damage-threshold events from the exact
  server notification and viewer prose naming actor, aid, and target without
  asserting a unit kill.
- Added a deterministic spectator tactical-aid audit covering serialized team/LOS
  state and top-level marker, delayed-spawn, and feedback coverage across 2,880
  linked replay paths.
- Documented the exact opposing-faction boundary: both factions' TA type, faction,
  position, and time are available from delayed-spawn effects. Enemy player identity
  remains unknown without a player-bearing marker, a validated unit-drop ownership
  bridge, or another POV.
- Added timeline schema-v8 spectator-view and both-faction team-only deployment
  validation to the full Rust corpus summary.
- Added reproducible unit-spawn, request, and projectile identifier audits for
  opposing-player tactical-aid attribution, backed by both client and dedicated-
  server binary traces and the full 2,880-path replay corpus.
- Added timeline schema-v9 validation for exact player ownership on the nine faction
  unit-drop aids; the 2,865 accepted replays contain 91,932 attributed deployments
  with no invalid player references or unresolved participant names.

### Changed

- Replaced the viewer's ambiguous `player lost a unit` wording with distinct exact
  same-player destruction and missing-killer `cause unknown` descriptions.
- Established from client/server call paths and 62,624 corpus records that
  `SendTATaunt` is cumulative tactical-aid damage evidence, not a kill event; 564
  same-tick victim deaths with another killer reject timestamp equality as a
  canonical TA-kill bridge.
- Reconciled workspace, parser, viewer, TODO, and dated findings documentation with
  timeline schema v9, decoded SDF support names, the local child-first commit chain,
  and the remaining single-replay attribution boundary.
- Corrected the earlier match-global marker wording. Player-bearing markers follow
  the recorder-visible faction; the separate top-level deployment stream covers
  both factions in ordinary, one-team spectator, and all-team spectator recordings.
- Advanced the viewer's nested parser pin and cache key together, and changed the
  human Tactical Aid projection to show faction/evidence, exact players only where
  serialized or supplied by the validated unit-drop ownership bridge, neutral
  unknown-player rows otherwise, and the recorder's spectator view.
- Implemented the conservative schema-v9 unit-drop ownership bridge in the parser
  and taught the viewer projection to distinguish reconstructed exact players from
  team-only deployments. Ambiguous and unmatched drops remain player-null.

## 2026-08-17

### Added

- Added a written timeline player-attribution plan and a deterministic read-only
  evidence-graph audit for tactical-aid purchases, markers, delayed spawns,
  feedback, projectiles, and requests.
- Added a global one-to-one candidate solver that propagates the recorder's exact
  purchase identity to uniquely matched effect instances without a timing cutoff.
- Added a corpus findings report covering 42,344 recorder purchases and the broader
  match-visible tactical-aid effect stream.
- Confirmed through client/server binary tracing and a zero-mismatch corpus
  cross-check that `SupportThingMarker.aTeam` carries the issuing player slot, giving
  exact actors for all 199,451 observed support markers.
- Added exact marker event-ID lifecycle linking for future
  `SupportThingMarkerStopped` records; none occur in the current 2,880-replay corpus.
- Added a bounded read-only SDF reader based on independently traced client/server
  loaders, then recovered all 200 replay catalogue definitions from shipped support
  localization with zero missing hashes or collisions.
- Added a tactical-aid catalogue report and two-replay manual playback checklist
  covering all 18 observed multiplayer aid families.

### Changed

- Made child-first manual Gitlink commits to parent `main` the primary component
  delivery workflow, with daily guarded Dependabot updates retained as a
  fallback, and corrected the documented component count from two to three.
- Corrected the older claim that other tactical-aid activations are absent from a
  replay: only their `SupportThingUsed` purchase ledger is absent; markers carry an
  exact actor slot, while spawn, feedback, and projectile effects still omit it.
- Replaced heuristic server-title guessing with exact `myGameName` metadata decoding;
  all 2,865 accepted corpus replays now resolve one of 664 serialized server titles.
- Added a viewer display projection that hides raw feed identifiers, orders playable
  result teams before spectator/unknown groups, and preserves the raw parser JSON.
- Integrated the validated marker actor rule into timeline schema v7 and the viewer:
  visible-faction markers retain exact players, internal child effects stay raw-
  only, and recorder costs attach only through unique exact one-to-one links.

## 2026-08-16

### Added

- Added the standalone native `wic-replay-viewer` component, with a bounded
  parallel importer, persistent SQLite replay summaries, lazy detail caching,
  and virtualized master-detail views for large replay libraries.
- Added the viewer Gitlink to guarded Dependabot updates alongside the parser
  and proxy components.

### Changed

- Added timeline schema v6's canonical participant directory, covering every player
  reference while retaining schema-v5 chat names as compatibility mirrors.
- Added the eight observed WicTracker custom-map translations to both standalone
  parser implementations and synchronized the Python parser's two Tug of War maps.
- Added bounded, deterministic parallel workers to the native timeline corpus
  runner and Python tactical-aid audit.
- Added conservative Ghidra MCP startup hygiene that removes only proven-stale
  project lock files after project-integrity and `lsof` checks.
- Extended full-corpus validation to all 26,233 schema-v6 participant entries, with
  no unresolved names, duplicate IDs, missing references, or identity conflicts.
- Extended timeline corpus verification to require resolved chat sender names and
  distinguish chat-only slots from true conflicts against the match-results roster.
- Extended the shared full quality and pre-push gate to run locked offline replay
  parser tests before Clippy, and added a release verifier covering the optimized
  native build, WASM package, and optional private ground truth.

### Fixed

- Replaced MCP SDK 1.26's thread-backed server-side STDIO reader with a
  selector-driven POSIX transport, restoring `ghidra-wic` initialization under
  the Codex 0.147 Linux command sandbox while preserving ordinary host use.
- Corrected the project-local `pyghidra-mcp 0.2.3` adapter so previously
  analyzed Ghidra programs retain their persisted analysis state instead of
  being rejected by decompile, metadata, byte-read, and symbol tools.
- Limited expensive semantic/string indexing to explicit search requests rather
  than starting a full background index as a side effect of every ordinary MCP
  tool call.
- Strengthened the MCP verifier to require analyzed project binaries and a real
  symbol-search call, closing the gap where binary listing passed while analysis
  tools remained unusable.

## 2026-08-15

### Added

- Added schema-v4 all/team chat extraction for messages visible to the replay
  recorder, with pre- and post-match chat kept outside the gameplay scrub range.
- Added explicit timeline point-of-view coverage and a recorder-only tactical-aid
  placement summary derived exactly from raw schema events.
- Added a single-process native Rust corpus-summary mode and converted the saved
  ground-truth harness to consume native Rust JSON rather than the Python parser.
- Added `findings/replay-chat-2026-08-15.md` documenting the chat record layout,
  point-of-view semantics, post-match boundary, and full-corpus measurements.

- Added `scripts/wic_bintag.py`, a reusable read-only reader for the `.wicdemo`
  `Event` envelope chain, replacing throwaway `/tmp` probes. The chain is
  self-describing — envelope size, float gameplay timestamp, and message name
  hash — and accounts for ~100% of the decompressed stream, so message walking
  is exact rather than pattern-matched.
- Added `scripts/ta-usage-audit.py`, a reproducible corpus audit of the
  `ChangeHonors`/`SupportThingUsed` pair that measures the pairing and
  cross-checks tactical-aid gifts instead of assuming either.
- Added `scripts/timeline-corpus-check.py`, which runs the release parser over a
  corpus and reports timeline results grouped by game mode so thin Assault and
  Tug of War coverage stays visible.
- Added `findings/tactical-aid-usage-2026-08-15.md` recording the record layout,
  the binary-confirmed message senders, the proof that tactical-aid attribution
  is point-of-view only, and the confirmation that recorded costs are marginal
  multi-strike prices charged per placement (nuke 80/60/40, carpet 45/30/20),
  validated against player ground truth and a corpus-wide max-triple check.
- Added a root `CLAUDE.md` mirror with `AGENTS.md` as the instruction source of
  truth.
- Added deterministic instruction synchronization to the tracked pre-commit
  hook and a quality-gate check that rejects mismatched instruction files.

### Changed

- Revised timeline output to schema version 4 and validated it across 2,865
  accepted replays: 35,801 pre-match, 23,597 in-match, and 5,488 post-match chat
  messages, with bot responses excluded and no invalid sender slots,
  out-of-range gameplay times, or TA-summary mismatches.
- Reworked `scripts/timeline-corpus-check.py` to invoke one recursive Rust corpus
  process and aggregate compact summaries instead of launching 2,880 parsers and
  serializing every timeline event.

- Revised tactical-aid timeline output to schema version 3: the parser now emits
  only evidence-backed raw placements and no longer reconstructs
  single/double/triple bundles from cost or timing. This removes an unbounded
  quadratic grouping path and avoids misreporting held strikes, partial recordings,
  role-priced aids, and Few Player Mode.
- Updated the timeline corpus checker to validate raw activation fields without
  depending on removed bundle metadata.

## 2026-08-14

### Added

- Pinned Ruff 0.16.3 in the host/container analysis environment.
- Added rustfmt to the container alongside the pinned Rust 1.97.1 toolchain and
  Clippy.
- Added tracked native pre-commit and pre-push hooks for the parent workspace
  and replay-parser component.
- Added shared fast/full quality commands. Pre-commit runs Ruff linting and
  formatting plus `cargo fmt --check`; pre-push additionally runs locked,
  offline Clippy checks with warnings denied.
- Added a standalone Ruff configuration and dependency pin to the replay-parser
  component.
- Formatted the Python reference parser and ground-truth runner with Ruff,
  normalized their line endings to LF, and verified unchanged behavior against
  all 16 private ground-truth replays.
- Added this project-level changelog.

### Changed

- Fast-forwarded `wic-replay-parser/main` to the tested parser sync history so
  Dependabot follows the canonical component branch.
- Replaced the global Ghidra MCP registration with trusted project-local Codex
  configuration, preventing unrelated Codex sessions from racing for Ghidra's
  exclusive project lock.
- Redirected the Ghidra MCP Java user home to ignored, writable project state so
  the server can initialize inside Codex's read-only home-directory sandbox.

## 2026-08-13

### Added

- Created the portable, headless World in Conflict reverse-engineering
  workspace and project operating instructions.
- Added the reproducible Podman/dev-container environment with Ghidra 12.1.2,
  JDK 21, Python RE libraries, Rust, 32-bit MinGW, native build tools, and
  binary-inspection utilities.
- Added scripts for private-evidence bootstrapping, executable hash checks,
  Ghidra headless analysis, project-specific Ghidra MCP, container lifecycle,
  and environment verification.
- Added the standalone replay parser and WiC proxy as independent Git
  submodules.
- Added daily Dependabot checks for private submodule updates targeting `dev`,
  guarded automatic merging, and automatic synchronization from promoted
  `main` commits back into `dev`.
- Documented the exact Codex session entry point and portable MCP registration
  procedure.

### Security

- Kept game binaries, replay corpora, Ghidra databases, credentials, and
  generated state outside Git.
- Mounted private evidence read-only in the container and disabled container
  runtime networking by default.
- Restricted automatic Dependabot merges to bot-authored changes affecting only
  the declared component submodule paths.
