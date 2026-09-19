# Map overview art plan

> **Status: complete.** This document preserves the original 2026-08-24 plan
> alongside its outcomes below. References to missing codec-3 support and the
> earlier packaging targets describe the starting state. Current implementation
> and build instructions live in `components/replay-viewer/README.md`.

## Objective

Give every replay-library row a picture of the map it was played on, sourced from
the game's own top-down overview art rather than from anything inferred. The
viewer currently ships a name-seeded procedural tile as a deliberate fallback; it
distinguishes maps but does not depict them.

The art is read at runtime from the user's own World in Conflict installation. No
game asset is committed to any repository or bundled into any distributed package.

## Established facts

Recorded from a read-only `scripts/wic_sdf.py` listing over `binaries/game/wic*.sdf`
on 2026-08-24.

- The asset is `maps/<internal_name>/overviewmap.dds`, with a companion
  `maps/<internal_name>/overviewextendedmap.dds`. 32 distinct internal map
  directories carry one.
- The directory component is the internal map name, which is exactly the value the
  parser already reports as `ReplaySummary.mapName`, read from the replay header.
  The replay carries only that name; the image lives solely in the game archives.
  The lookup needs no translation table and no display-name matching.
- Every one of the 32 entries is stored with codec 3, the segmented codec that
  `scripts/wic_sdf.py` lists but deliberately does not extract. Codecs 0 and 1 are
  the only ones it decodes today.
- Uncompressed sizes are 43,832 bytes for 30 of them and 32,896 for two
  (`usfarmland1` in `wic3.sdf`, `ustown1` in `wic3.sdf`), consistent with two
  different DDS dimensions rather than a single fixed thumbnail size.
- Three maps appear in more than one archive: `russia3` (`wic2`, `wic60`),
  `usfarmland1` (`wic3`, `wic60`), and `usfarmland3` (`wic3`, `wic45`). The
  `usfarmland1` pair differs in uncompressed size, so these are genuinely
  different images and not duplicate copies.

## Approach: read at runtime, never redistribute

The viewer locates the user's game installation, decodes
`maps/<mapName>/overviewmap.dds` from their own `.sdf` archives on demand, and
caches the decoded image locally. Every user sees the art from the copy of the
game they already own.

This is a deliberate choice over extracting the 32 images once and shipping them.
Extraction onto one's own machine from one's own installation is unremarkable;
committing Massive's textures into a repository or baking them into the deb, rpm,
Flatpak, and Windows bundles the viewer distributes is redistribution of
copyrighted assets. Runtime loading removes that question rather than answering
it, and it fits what is already built: the procedural tile is the fallback for a
user with no installation, an unrecognised or custom map, and any art that fails
to decode.

The cost is that the codec-3 decoder has to live in the viewer's Rust side rather
than in a one-off Python script, because decoding now happens on the user's
machine. `scripts/wic_sdf.py` remains the right place to recover and prove the
codec before porting it.

## Resolved blocker

Codec 3 had no decompressor, and nothing downstream could start until it was
recovered from `wic.exe`. It is now decoded: a 128-byte prefix held in the
archive's version-10 auxiliary metadata, followed by three independently inflated
zlib streams. The layout and the supporting evidence are recorded in
`findings/map-overview-art-codec-and-precedence-2026-08-24.md`.

## Settled decision

**Archive precedence: higher archive number wins.** The three duplicated maps
were decoded and compared, and in every case the later archive carries a
deliberate revision rather than a recompression: `russia3` is regraded into dark
winter, `usfarmland1` revises terrain detail and gains a full mip chain, and
`usfarmland3` carries localized terrain and vegetation changes. Ascending numeric
order with later archives overriding earlier ones therefore selects the revised
art in all three cases, and generalizes to future duplicates.

One caveat is recorded rather than hidden: this rule is grounded in image content,
not in a trace of `wic.exe`'s own archive mount order. Confirming engine parity
would mean tracing the mount and lookup path in the binary. It matters for exactly
one visible tile, `russia3`, since the other two differ only in detail invisible at
tile size.

## Phases

All five are complete.

1. Done. Codec 3 recovered from `wic.exe` and implemented in the read-only
   `scripts/wic_sdf.py`, with a synthetic fixture and negative cases.
2. Done. All 32 `overviewmap.dds` entries round-trip to their exact declared size
   as valid 256x256 DXT1, with hashes, dimensions, and source archives recorded
   under `findings/`. The decoder was additionally checked against 1,171 other
   codec-3 textures so it is not fitted to these 32.
3. Done. Precedence settled as above.
4. Done. The archive reader and codec-3 decoder are ported to the viewer's Rust
   side in `src/sdf.rs` and `src/map_art.rs`, with a game-installation setting,
   best-effort detection, and a manual override through the native picker.
5. Done. Art resolves at runtime behind the existing tile seam, decoded once per
   source set into its own SQLite table and served per internal map name, with
   the procedural tile preserved as the permanent fallback.

Two things were learned after the phases closed, both from running the viewer
against a real library rather than a fixture:

- **Summaries do not store the internal map name.** They store
  `maps/<name>/<name>.ice`. See the correction in the handoff note; this silently
  disabled art entirely until the field was checked against real data.
- **Community maps are a second source.** They live at
  `Documents/World in Conflict/Downloaded/maps`, independent of where the game is
  installed, and ship as ordinary `.sdf` archives named after the map rather than
  as `wic<n>.sdf`. They decode with the same codec 3. On the development library
  they supply 34 of the 63 available overviews, and without them a third of the
  rows had no art. Shipped archives are applied first and downloaded maps second,
  so a community map overrides a shipped map of the same name.

The earlier expectation that the two byte-size classes would resolve to two
different dimensions was disproved: both are 256x256 and differ only by mip count.

## Non-goals

- Committing extracted game art to any repository, or bundling it into any
  distributed package. The whole point of the runtime approach is that neither
  happens.
- Replacing the procedural fallback. It must keep rendering for a custom or
  unrecognised map, for a user with no game installation, and for any map whose
  art fails to decode.
- Deriving map pictures from replay coordinates. Tactical-aid world positions
  describe one match's activity, not the terrain, and a short replay would plot
  almost nothing.
