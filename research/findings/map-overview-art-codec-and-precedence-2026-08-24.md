# Map overview art codec and archive-precedence findings

## Scope and source identity

This report completes phases 1-3 of
`notes/map-overview-art-extraction-plan.md`. It does not change the replay viewer
and does not place decoded game art in the repository. Visual comparisons used
temporary files under `/tmp/wic-map-overview-compare` only.

The codec was recovered from the 32-bit little-endian x86 PE loaded in Ghidra at
image base `0x00400000`:

- file: `binaries/game/wic.exe`;
- version: `1.0.1.1 (b35)`;
- SHA-256: `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc`.

## Phase 1: codec 3

### Binary observations

The prior "segmented" label is confirmed, with a more exact layout:

1. `wic.exe:0x009f4cc0` reads the codec from the top two bits of the entry size
   word. Its codec-3 arm requires a type-1 per-entry descriptor from the
   version-10 auxiliary table. The descriptor names a 128-byte prefix and gives
   the first two stored segment lengths; the third length is the entry's stored
   size minus those two lengths. The function copies the prefix from auxiliary
   metadata rather than from the entry payload.
2. `wic.exe:0x009f4320` is the codec-3 streaming read path. It seeks to the three
   stored segments, initializes the same inflate implementation used by codec 1,
   and inflates each segment independently into consecutive output ranges.
3. `wic.exe:0x009f3fc0` partitions the post-prefix output into three ranges for
   streaming reads. A whole-entry decoder does not need to reproduce those seek
   ranges because each zlib stream is self-terminating, but its concatenated
   result must still match the entry's declared uncompressed size.

The resulting layout is therefore:

```text
decoded entry = 128-byte auxiliary prefix
              + inflate(payload segment 1)
              + inflate(payload segment 2)
              + inflate(payload segment 3)
```

### Descriptor spans and prefix sharing

Observed across all archives: every codec-3 descriptor is type 1, and its span in
the segment table is either 140 or 12. A 140-byte span carries the 12-byte header
followed by its own inline 128-byte prefix. A 12-byte span carries only the header,
and its `prefix_offset` points at a prefix stored elsewhere in the descriptor
block and shared with other entries. In `wic65.sdf`, 563 of 646 codec-3 entries
take the shared form. Because the decoder always slices 128 bytes at
`prefix_offset` rather than assuming the prefix follows the header, both forms
decode correctly.

Two fields of the 28-byte auxiliary header remain unidentified. The sixth word is
`7471201` in every archive inspected, which suggests a format magic or version
rather than per-archive data; the second varies. Neither is needed to decode.

`scripts/wic_sdf.py` now parses the bounded version-10 auxiliary blocks and
implements this codec at `_decode_block`. It checks all archive/table/descriptor
offsets, all stored lengths, each zlib stream boundary, and the final declared
size. The tool remains read-only. `scripts/test_wic_sdf.py` includes a synthetic
codec-3 archive fixture and negative cases for wrong descriptor kind,
auxiliary entry-count mismatch, oversized segments, and empty segments.

The decoder is not fitted to the 32 overview maps. All 10,032 codec-3 entries
across the 26 shipped archives are `.dds` in version-10 archives, and every one
of their descriptors passes validation at archive open. A 1,171-entry random
sample decoded to its exact declared size with a valid DDS header in every
case, spanning DXT1, DXT3, DXT5, and uncompressed pixel formats at square and
non-square sizes from 8 x 8 to 2048 x 2048.

## Phase 2: all overview entries

`scripts/map_overview_art_audit.py` decoded all 32 archive entries (29 unique map
names), required the exact declared uncompressed size, parsed the DDS header, and
independently recomputed the DXT1 payload length from width, height, and mip count.
The generated machine-readable report is
`findings/map-overview-art-audit-2026-08-24.json` (ignored derived output).

Every entry is a valid `256 x 256` DXT1 DDS. The expected two-dimension result in
the handoff is disproved by the archive headers: the 128-byte DDS header is the
verbatim auxiliary prefix, so codec decompression cannot alter those dimensions.
The two byte-size classes instead differ by mip chains:

- `32,896 = 128 + 32,768`: one 256 x 256 DXT1 level;
- `43,832 = 128 + 43,704`: all nine levels from 256 x 256 through 1 x 1.

This accounts for every decoded byte exactly. `usfarmland1` and `ustown1` in
`wic3.sdf` are the two one-level files; the other 30 entries carry nine levels.

| Map | Source archive | Bytes | Dimensions | Mips | Decoded SHA-256 |
| --- | --- | ---: | ---: | ---: | --- |
| `europe1` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `055afceda9f2d778f65674af2ec589f41db236e1df72a136140f98892694094d` |
| `europe2` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `588f6415b915364934936ced0992eb74937919169d6ca3b59d2a1b6d9ae983e1` |
| `europe4` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `68a77441b2ea3fb1dd71c69d3a84bc57770bc8173d5a68bb96b3792dcdef0a1a` |
| `newyork1` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `dd5c210ae306b7bc28133b1a0b6205bd3d168a9db0b6cde365b3ed9e825ee14b` |
| `russia1` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `3b53d557d1cc756725a5c5638bbec0704dd7bfe79f39398255a6fcb1c7d68a40` |
| `russia2` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `2a9a9bf8d831988f345737018444e2ffa09e52e12b1c1432ad9e30469050d963` |
| `russia3` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `644aeab67d49a4ec0490cc77d4b0de6af2300be3a1d388b1c32bb566711d79d3` |
| `russia4` | `wic2.sdf` | 43832 | 256 x 256 | 9 | `6f6ce7a6d8d65423bc602308b86d5ae3e4d113ee32a4a366d1448e514f16447c` |
| `seattle1` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `a39451e346cb2aff27a3067b6165954f3092aacf10edefdfd67fdfc5822786f7` |
| `seattle2` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `7489878650a1b6840324917cfb86bdb866bda1014f642a915c59bf62d76d8621` |
| `seattle3` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `a3b389f7a18cb819d9883da559b93db70067ff49b8bb0096d7f739d5f2b8353c` |
| `seattle4` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `9e2ea347a711b9c39b6262e59ca1f65f4b7b009bf85d3cc67dd13fe1edbfd790` |
| `usdesert1` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `a70dcefa42df23d828f9dafbb5474c5ac341726d85fb45385b07a33a0f05789a` |
| `usdesert2` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `add2e9f1ddc230308fb72cc7c9743f57df69744307ef1ff10641bfe949cce834` |
| `usfarmland1` | `wic3.sdf` | 32896 | 256 x 256 | 1 | `023ed05c42e8865f77391e981bd1c442a08a6f37e270b4387df57354a02bfc8f` |
| `usfarmland2` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `1edb4d7852a7484f021cdcacbde0ef56897ca84a7c374c9c2a5e87c0c2f67ded` |
| `usfarmland3` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `19089f29ebc0d4c9af3a9b7ba5eee56cce1b31d312bc5e8839857711461553a7` |
| `usfarmland4` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `70799ca5e7821e67303e2ba389747d037e32b876077b79e8d9bb00b26f02cf3d` |
| `ustown1` | `wic3.sdf` | 32896 | 256 x 256 | 1 | `e5dde0fca9c4037827baaa79d99b7390b37d142486ee710d1f51a1db5a44e4fd` |
| `ustown3` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `8045d6b8b344c3062b4b02299bf6f90f988647683e2eb7390af4afb73f6bb9cd` |
| `ustown4` | `wic3.sdf` | 43832 | 256 x 256 | 9 | `ca25669b506a521427fddd76282cfe102c646859fce1028087b13a3ee822e51c` |
| `usfarmland3` | `wic45.sdf` | 43832 | 256 x 256 | 9 | `9c4f791708539df89ddf2c740e971eb747094c595c30d2970326b8500273dbf6` |
| `berlin1` | `wic60.sdf` | 43832 | 256 x 256 | 9 | `c8b52ee76e3b940b8a94292c06d2f4fb2e09b3e094afc78d7f975e36c02e28d3` |
| `europe3` | `wic60.sdf` | 43832 | 256 x 256 | 9 | `77d68b665c95054284c5172663836c8057f62065f07bb9416d6ae594b7866aa5` |
| `norway1` | `wic60.sdf` | 43832 | 256 x 256 | 9 | `2f8ea3bb52fc689f71177c1fa4c82f013f3c7cca87fceddc8771a0fa1f89ef22` |
| `russia3` | `wic60.sdf` | 43832 | 256 x 256 | 9 | `64f8c53591d25dd79a5ab2de6805ad93d2de7841e5a0c7e37c5ab9cbfdda7760` |
| `usfarmland1` | `wic60.sdf` | 43832 | 256 x 256 | 9 | `8d6ebf599d59458545a7e1e73306521e86bb4b50c2650495858c13f2bf5c8d8d` |
| `usfarmland5` | `wic60.sdf` | 43832 | 256 x 256 | 9 | `8ec7218a1f07c88d443f70c7729042c561d1929a846d324bb2a196e68653d2cc` |
| `do_apocalypse` | `wic65.sdf` | 43832 | 256 x 256 | 9 | `2ad4c769b2f8a66f6c9d6dd8bed9d8dd14da0d1f80a4cb4f5bddd77f4c0f7d76` |
| `do_studio` | `wic65.sdf` | 43832 | 256 x 256 | 9 | `0ba8d9c2ac990317616c76f5629ec5c60f46a898433d6e62a87211f9f1f891f1` |
| `do_tequila` | `wic65.sdf` | 43832 | 256 x 256 | 9 | `7a65033b6296b974c237877f416412f7a9fe93ea4ed82223144a0c1bd15f2279` |
| `europe5` | `wic65.sdf` | 43832 | 256 x 256 | 9 | `a2a88eeac9e56f45f52ff4dae54d9d6fc115a349e92a9bc16e6cd8d00740eb18` |

## Phase 3: duplicate-map comparison

All three base/patch pairs differ in their top-level decoded pixels, not only in
metadata or lower mip levels.

| Map | Pair | Changed top-level pixels | Observation |
| --- | --- | ---: | --- |
| `russia3` | `wic2` / `wic60` | 65,536 of 65,536 | Same terrain geometry, but the `wic60` art is a comprehensive dark blue winter regrade/retexture; the base image is much brighter and nearly white. |
| `usfarmland1` | `wic3` / `wic60` | 59,723 of 65,536 | Same field/road/river layout. The `wic60` image has revised color and fine detail, concentrated visibly along water, roads, buildings, and vegetation, and adds the full mip chain. |
| `usfarmland3` | `wic3` / `wic45` | 17,255 of 65,536 | Same layout and nearly identical overall tone, with subtle localized terrain, tree, water, and road-detail changes throughout the image. |

The comparison supports this recommendation: process numbered game archives in
ascending numeric order and let a later archive replace an earlier entry with the
same normalized path. That selects `wic60` for `russia3`, `wic60` for
`usfarmland1`, and `wic45` for `usfarmland3`, preserving the shipped patch
overrides. This is a recommendation only; no precedence rule has been adopted in
viewer code.

