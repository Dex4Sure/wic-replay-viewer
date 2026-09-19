# Handoff — map overview art, phases 1–5

> **Status: complete.** All five phases have landed. Codec 3 is decoded, the 32
> overview entries are audited, archive precedence is settled as
> higher-number-wins, and the viewer draws real map art read at runtime from the
> user's own installation. See `notes/map-overview-art-extraction-plan.md` for
> the outcome and `findings/map-overview-art-codec-and-precedence-2026-08-24.md`
> for the codec and comparison evidence. What follows is kept as the record of
> what the work involved and why the decisions were made the way they were.


This is a continuation brief for a fresh Codex session picking up
`notes/map-overview-art-extraction-plan.md`. That note holds the objective,
established facts, approach, and non-goals; this one holds the operational detail
needed to execute without rediscovering it. Read both before starting.

Prepared 2026-08-24, against parent `main` and `components/replay-viewer` at
`d8b9ecd`.

## State on arrival

Done and pushed:

- The viewer draws a name-seeded procedural tile on every replay-library row. It
  is the fallback layer and stays one permanently. Do not remove it.
- The asset has been located and the blocker identified. No extraction has been
  attempted and no game art exists anywhere in either repository.

Not started: every phase in the plan note.

## The shape of the work

Art is read at runtime from the user's own game installation and never
redistributed. Phases 1–3 recover and prove the codec in Python, where iteration
is cheap. Phases 4–5 port that capability into the shipped viewer and wire it to
the tile seam that already exists.

## Phase 1 — the one hard problem

`maps/<internal_name>/overviewmap.dds`, all 32 of them, are stored with codec 3.
`scripts/wic_sdf.py` decodes only codec 0 (stored) and codec 1 (zlib), in
`_decode_block` at `scripts/wic_sdf.py:75`, which raises
`codec {codec} extraction is not implemented` for anything else. The codec word is
the top two bits of the block size word, `CODEC_SHIFT = 30`.

Effectively all the risk in this work is here. Everything downstream is
mechanical, and the phase-1 deliverable is a decoder plus the evidence that it is
correct.

Prior work deliberately routed around this: the tactical-aid catalogue recovery
needed only codecs 0 and 1, as recorded in
`findings/tactical-aid-catalogue-2026-08-17.md:85`. There is therefore no existing
partial codec-3 knowledge in the repository to build on. Start from `wic.exe`.

Suggested entry: the archive reader in `wic.exe` dispatches on that same two-bit
codec field. Find the dispatch, then follow the codec-3 arm. "Segmented" is the
existing note's word for it and is a description of the observed layout, not a
proven structure — treat it as a hypothesis to confirm or discard, not a fact.

Use the `ghidra-wic` MCP against `WiCProject`. Do not start a second writer.

Reproduce the listing with:

```bash
"$HOME/.venvs/wic-analysis/bin/python" scripts/wic_sdf.py \
  binaries/game/wic*.sdf --match 'overviewmap' --json findings/<name>.json
```

## Phase 2 — verification bar

A decoder that produces plausible bytes is not done. Decoded output must:

- Match the record's `uncompressed_size` exactly. `_decode_block` already enforces
  this and that check must stay.
- Parse as a real DDS: valid magic, header, and a pixel payload whose dimensions
  agree with the byte count. The two size classes (43,832 and 32,896) should
  resolve to two different dimensions; if they do not, the decode is wrong.
- Round-trip all 32 entries, not a lucky sample.

Record SHA-256 hashes, dimensions, and source archive per entry under `findings/`,
per working rules 1 and 4.

## Phase 3 — archive precedence

`russia3`, `usfarmland1`, and `usfarmland3` each appear in both a base and a patch
archive. Highest archive number is the expected rule but is unproven. Settle it by
comparing decoded images and report the comparison; do not assume the rule and
move on. This is the user's call to confirm.

`usfarmland1` is the useful case: its two copies differ in uncompressed size
(32,896 in `wic3.sdf`, 43,832 in `wic60.sdf`), so a correct decode of both proves
the images are genuinely different rather than duplicated.

## Phase 4 — port to the viewer

The archive reader and the codec-3 decoder move to the viewer's Rust side, because
decoding now happens on the user's machine. `scripts/wic_sdf.py` stays as the
read-only reference implementation and must keep working.

The game installation path is a user setting:

- The `settings` table already exists in `src/database.rs` (created at
  `src/database.rs:50`), and `library_locations` at `src/database.rs:54` is the
  precedent for a user-supplied path, including a migration from an older
  single-value setting at `src/database.rs:105`.
- Detect a likely installation, but always allow a manual override through the
  native picker. Steam, GOG, and the recovered layouts all differ, and a user may
  keep replays on a machine with no game installed at all.
- No installation configured is a normal state, not an error. Every row falls back
  to its procedural tile and nothing warns or nags.

Treat the user's installation exactly as this workspace treats `binaries/`: read
only, never written, never modified.

## Phase 5 — runtime resolution and the tile seam

The seam is already built. `frontend/components/MapTile.vue` renders the artwork
and `frontend/mapTile.ts` generates it from the map name. Real art becomes a new
branch inside `MapTile.vue`, selected by `mapName`, with `mapTileArt` as the
fallback when no art is available.

- The lookup key derives from `ReplaySummary.mapName`, not from
  `mapDisplayName`; the tile uses the display name only because it is drawing
  letters for a human, and that is the wrong key for asset lookup.

  **Correction.** This note originally asserted that `mapName` *is* the internal
  name and is exactly the `<internal_name>` directory component. That is wrong.
  Summaries store an archive path, `maps/ustown4/ustown4.ice`, while art is keyed
  by the bare `ustown4`. Building the lookup on the claim as written made every
  lookup miss, and because a map with no art legitimately falls back to its
  procedural tile, the failure was indistinguishable from normal operation. The
  shipped code reduces the field to the internal name. Check a field's real
  contents against the database before keying anything on it.
- **Decode once per map, not once per row.** The library is virtualized over
  hundreds to thousands of rows and the same map recurs constantly. Resolution
  must be asynchronous and cached by internal map name, so scrolling never blocks
  on an archive read. The existing lazy detail cache in `src/database.rs` is the
  precedent; a decoded image is a natural fit for the same SQLite store.
- A row must render immediately with its procedural tile and upgrade to real art
  when it arrives. Never leave a row blank waiting on a decode.
- The fallback must still fire for a custom or unrecognised map, for a map whose
  art fails to decode, for a user with no game installation, and for a
  misconfigured install path. Test all five.
- Faction tinting lives on the tile's inner edge (`.map-tile-allied`,
  `.map-tile-soviet` in `frontend/styles.css`). Selection owns red on the card
  border, background, and accent stripe. Keep those separate; a USSR-red stripe
  beside the selection state was tried and muddied both.
- `MapTile.vue` deliberately does not reuse `factionToneClass`. Those chip classes
  set `background` and `border-color` and would paint over the artwork. There is a
  comment saying so; leave it.

Do not touch `PARSER_CACHE_KEY` (`components/replay-viewer/src/model.rs:8`). Map
art changes no parser output and no cached replay detail. If this work makes you
want to advance that key, something has gone wrong. A decoded-art cache needs its
own key if it needs one at all.

## Working rules that bite here

- `binaries/`, `replays/`, the `.sdf` archives, and the user's game installation
  are source evidence. Read-only, always. `scripts/wic_sdf.py` must stay a
  read-only inspector.
- No extracted game art is committed to any repository or bundled into any
  package. Local decode output belongs in ignored paths or the runtime cache.
- Viewer changes are committed and pushed in `components/replay-viewer` first,
  then the parent Gitlink is advanced in its own commit staging only that path.
  Never commit a Gitlink pointing at an unpushed child commit.
- Verify the viewer with `components/replay-viewer/scripts/quality.sh`.
- Update the root `CHANGELOG.md` for orchestration and tooling changes; viewer
  implementation history belongs in the viewer's own changelog.
- Separate observation from hypothesis in every note and commit message.
