# Replay-state reconstruction evidence

Date: 2026-08-27

## Result

A `.wicdemo` can drive a useful state-faithful 2D map replay. The stream contains
authoritative unit transform checkpoints, unit creation/health/ownership/terminal
events, command and perimeter points, ownership transitions, movement commands,
and positioned tactical-aid records. It does **not** currently prove the exact
continuous path between checkpoints, visual effects, fog-of-war, or camera point
of view. The proof viewer therefore holds the last checkpoint and labels that
policy; it does not interpolate invented movement.

The evidence tooling remains isolated in the parent research workspace. The
validated projection is integrated lazily into the replay viewer without changing
the canonical replay-parser schema.

## Source evidence

Client target: `binaries/game/wic.exe`, PE32 x86, version 1.0.1.1 b35, SHA-256
`41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc`.

Read-only Ghidra inspection established this path:

- `wic.exe:0x00b843f0` selects/writes replay unit-frame records and calls the
  frame serializer at `0x00923ad0`.
- `wic.exe:0x00922e20` is the compact encoder; `0x00922c60` writes each compact
  child transform.
- `wic.exe:0x00922d50` is the matching compact decoder. Position is decoded with
  binary32 operations equivalent to `(u16 - 0.5) * 0.023809524`; orientation is
  `(u16 - 20000.5) * 0.0002`. These are not replaced with approximate `/42`
  arithmetic.
- `wic.exe:0x00923170` reads the serialized frame. Replay dispatch at
  `0x00b3a700` reaches `EXP_Player::UnitFrame` at `0x009362e0`, which records the
  frame when capture is active and applies it to the live unit through
  `0x00915db0`.
- A full frame contains seven parent binary32 values (three position, four
  orientation), a 16-bit unit ID, an 8-bit child count, then two binary32
  orientation values, a flag, and an index for each child. A compact frame packs
  the unit ID into header bits 0-11 and child count into bits 12-14, followed by
  three position and four orientation `u16` values; each child carries an
  index/flag byte and two orientation `u16` values.

No Ghidra annotations or source evidence were modified.

## Reproducible implementation

- `scripts/replay_state_reconstruction.py` exports research schema v1 and retains
  raw packed words, binary32 time bits, envelope offsets, and unit generations
  keyed by `(unit ID, creation offset)`. Its corpus mode omits frame payloads but
  exercises the same structural decoder.
- Optional map extraction only reads explicitly supplied SDF archives. The
  export records archive/ICE/DDS SHA-256 provenance and embeds a decoded DXT1
  overview PNG in the ignored JSON output.
- `scripts/playback-proof/index.html` is a dependency-free Canvas playback proof.
  It supports file selection, play/pause, speed, scrubbing, map projection,
  objective ownership, unit state, terminal removal, and positioned TA pulses.
- `scripts/replay_state_oracle.py` supplies a separately authorized runtime
  capture at RVA `0x005362e0` and an offline raw-bit comparator. Capture refuses
  a binary hash mismatch, requires `--confirm-attach`, and creates rather than
  overwrites its JSONL output.

## Focused replay result

Replay: `4v4 MM Quarry 2010 #1 Dexter pr0 Support.wicdemo`, SHA-256
`c0bab85221538f158e1b077eaae033245f308d38c1d273b23a4c2454ecd1a826`.

- Recording span: 1,598.103515625 seconds.
- Unit lifecycle: 480 creates, 219,659 transform checkpoints, 5,369 health
  records, 312 destroys, and 77 removals.
- Movement/event context: 16,331 moving records, 6,696 long-move broadcasts,
  1,076 halts, and 511 order-queue clears.
- Map state: 4 command points, 15 command-point ownership changes, 10 perimeter
  points, and 83 perimeter ownership changes.
- Tactical aid: 113 delayed deployments and 74 positioned markers.
- Frame gaps: p50 0.34375 s, p90 0.349609375 s, p99 0.3671875 s. The 604-second
  maximum is retained as evidence and is not silently interpreted as continuous
  movement.
- The explicit `wic60.sdf` source yielded playfield bounds x=123..1451 and
  z=92..1460. Archive SHA-256 is
  `d712f6e399ea8c7456f8716e81e8a30a5cd1b4b5695eb217431f0bf75477c09e`;
  map ICE SHA-256 is
  `f08ceaf336a2e2fa894427bf9f0a271afe6d185322310ee9940435cad23d697c`;
  overview DDS SHA-256 is
  `64f8c53591d25dd79a5ab2de6805ad93d2de7841e5a0c7e37c5ab9cbfdda7760`.

## Validation status and boundary

Synthetic unit tests cover compact/full parent and child layouts, malformed
frames, unit-ID reuse, DXT1-to-PNG output, map bounds, float32 decode, and exact
offline oracle comparison. A Chromium headless smoke test loads the tracked
synthetic document and observes a rendered unit, objective, and frame counts.

The corpus audit discovered 2,880 replay paths. Its first pass decoded 2,864 and
identified 16 paths for review. After correcting the invalid assumption that an
absent `TeamWins` record means corruption, the focused recheck accepted 15 of
those paths without malformed state records and reported their missing
`TeamWins` evidence separately. The combined result is therefore 2,879 decoded
paths and one structurally undecodable zero-byte input
(`replays/wicgate-documents/demo59.wicdemo`, SHA-256
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`).
This preserves the known absent-`TeamWins` edge cases instead of discarding them.

The live runtime oracle has not been run: attaching to a running game requires
fresh user confirmation immediately before the operation. Until that positive
control matches the original client, checkpoint decode is strongly supported by
the paired binary encoder/decoder and corpus structure but remains short of
runtime-proven parity. Point-of-view completeness and between-frame simulation
also remain explicitly unclassified.

## Recommendation

Keep the export as research schema v1 until the runtime raw-bit comparison passes
and the intended POV policy is settled. The viewer now decodes the stream lazily
when its Replay tab is opened rather than expanding or invalidating the canonical
timeline cache. A future persistent compact state cache should preserve the same
evidence-backed checkpoint/objective/event boundary and unknown-interval policy.
