# Tactical-aid support catalogue recovery — 2026-08-17

## Result

The shipped support localization provides an exact name mapping for every support
ID enumerated by the replay corpus. All 200 replay catalogue IDs equal Adler-32 of
exactly one internal definition name from `maps/supportweapons.loc`; there are zero
missing IDs and zero collisions. This resolves the earlier belief that the packed
SDF archives would require an unknown compression format or runtime capture.

The same source supplies a human-facing `myGuiName` for all definitions. Of the 200
definitions, 199 have one GUI name. One unused single-player definition,
`SINGLEPLAYER_DaisyCutter_USSR` (`0x8eed0a0c`), has two localization values and is
retained as ambiguous rather than selecting one by file order.

The recorder-visible-faction `SupportThingMarker` stream contains more than player-selected
tactical aids. Across the current 2,880-replay audit, its 199,451 records divide
into:

| Presentation class | IDs | Marker records | Interpretation |
|---|---:|---:|---|
| top-level faction aid | 54 | 109,854 | visible-faction player-attributed TA effects |
| child effect | 24 | 36,784 | internal sub-effects of artillery/heavy-air aids |
| special ability | 3 | 52,813 | unit/special markers, not TA purchases |

Therefore, the viewer must not present every marker as a separate TA use. The safe
multiplayer presentation layer selects only top-level faction definitions and uses
top-level `SupportThingSpawnedDelayed` for both-faction coverage. It may enrich an
equal exact marker/deployment group with the marker's player and enrich a recorder
marker with its matching `SupportThingUsed` cost; it must not manufacture players or
costs for unmatched effects.

## Input identities

| Input | Identity | Purpose |
|---|---|---|
| `binaries/game/wic.exe` | PE32 x86, version 1.0.1.1 b35, SHA-256 `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` | client SDF loader and independent nuclear strings |
| `binaries/game/wic_ds.exe` | PE32 x86, version 1.0.1.1 b35, SHA-256 `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` | server SDF loader and support archive |
| `binaries/server/wic_ds.sdf` | SDF version 10, SHA-256 `fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3` | authoritative support definitions/localization |
| `maps/supportweapons.loc` | 165,706 decoded bytes, SHA-256 `da43cfbfac2189bb1774049d484305bb9f2307ab91339b49a7fcedf6077f78fb` | internal names and English GUI names |
| `findings/ta-usage-audit.json` | SHA-256 `b24b328476a26dbdf6174dcc5e57e9af32e32c8ba5ef0f655bf06a1893e33a27` | 200 replay catalogue IDs and 54 recorder-activated IDs |
| `findings/timeline-attribution-audit-v3.json` | SHA-256 `46e930ad107c7bf234a9337bae13b2d90ab116e794b7daf4e1a3cac9c8b651ed` | all-player marker inventory and fingerprints |

The JSON files are ignored derived artifacts. Their tracked generators are
`scripts/ta-usage-audit.py`, `scripts/timeline-attribution-audit.py`, and
`scripts/ta_support_catalogue.py`.

## SDF format evidence

All inspected game archives begin with:

```text
+0  3  "RYS"
+3  1  archive version (9 or 10 accepted by the executables)
+4  4  absolute directory-block offset, little-endian
```

At the directory offset is an 8-byte block header:

```text
+0  4  uncompressed size
+4  4  top 2 bits = codec, low 30 bits = stored size
```

Codec 0 is stored/plain and codec 1 is zlib. The decoded directory begins with a
count followed by sorted `(lookup hash, record offset)` pairs. Each referenced
record contains a content ID, uncompressed size, codec/stored-size word, payload
offset, path-prefix offset, and inline NUL-terminated basename. Directory-relative
pointers are validated before use.

The client loader at `wic.exe:0x009f4cc0` and server loader at
`wic_ds.exe:0x00428650` independently validate the `RYS` signature/version, seek to
the stored directory offset, decode the size/codec word, decompress the directory,
and relocate its internal offsets. `wic.exe:0x00bb04b0` and
`wic_ds.exe:0x0043ecc0` mount `wic_ds.sdf`, numbered `wic%i.sdf`, and
`wicloc%i.sdf` archives. This is the binary basis for `scripts/wic_sdf.py`.

The bounded reader enumerated 41,315 entries across the selected game/server
archives. The required files are ordinary codec-1 entries:

- `maps/supportweapons.ice`: internal support database and assets;
- `maps/supportweapons.loc`: localization keys/values;
- `python/wicgame/tacticalaid.pyo`: compiled tactical-aid scripting.

No codec-2 or segmented codec-3 extraction was needed for the catalogue recovery.

## Exact name proof

Localization keys have this form:

```text
SupportWeaponDatabase.<section>.<internalName>.myGuiName<TAB><GUI name>
```

For example:

```text
SupportWeaponDatabase.US.TacticalNuke_US.myGuiName<TAB>Tactical Nuke
```

Hashing `TacticalNuke_US` produces replay support ID `0x2e8f05c0`. Applying the
same operation to all localization definitions produced a bijection with all 200
replay catalogue IDs. The four nuclear names previously recovered independently
from executable strings agree exactly, providing a separate cross-check of the
field semantics.

Costs, faction availability, timing, duration, direction, and visual effects were
not used to create any mapping. They remain corroborating fingerprints only.

## Multiplayer top-level catalogue

The 54 top-level definitions activated by recorder ledgers, plus the unused normal
NATO nuclear definition, group as follows. IDs remain faction-specific even where
the GUI name is shared.

| GUI category | USA | NATO | USSR |
|---|---|---|---|
| Aerial Recon | `1d3e0477` | `26d10501` | `2724051c` |
| Airborne Infantry | `2a640597` | `36370621` | `368a063c` |
| Airdropped Transport | `2861054e` | `33a205d8` | `33f505f3` |
| Airdropped Light Tank | `22dc04ed` | `2d5b0577` | `2dae0592` |
| Repair Bridge | `3aff068f` | `48c20719` | `49150734` |
| Napalm Strike | `301505f3` | `3ca0067d` | `3cf30698` |
| Tank Buster | `2497052b` | `2f9205b5` | `2fe505d0` |
| Laser Guided Bomb | `308a0604` | `3d37068e` | `3d8a06a9` |
| Air-to-Air Strike | `35460642` | `426f06cc` | `42c206e7` |
| Chemical Strike | `1d19047b` | `26b40505` | `27070520` |
| Heavy Air Support | `4337071e` | `521807a8` | `526b07c3` |
| Light Artillery Barrage | `76be096c` | `8a3b09f6` | `8a8e0a11` |
| Precision Artillery | `1f2104c0` | `2946054a` | `29990565` |
| Heavy Artillery Barrage | `77080971` | `8a8f09fb` | `8ae20a16` |
| Airstrike (`ClusterBomb_*`) | `2971056a` | `34ea05f4` | `353d060f` |
| Daisy Cutter / Fuel Air Bomb | `290c0579` | `34a30603` | `34f6061e` |
| Carpet Bombing | `34760625` | `416506af` | `41b806ca` |
| Tactical Nuke | `2e8f05c0` | `3ab4064a` | `3b070665` |

NATO also has `TacticalNuke_NATO_British` (`7adc097e`), GUI name `Tactical Nuke`;
that is the NATO nuclear variant actually observed in the corpus. The normal
`TacticalNuke_NATO` appears in catalogues but has no recorded purchase or marker.

The separate `Airstrike_US`, `Airstrike_NATO`, and `Airstrike_USSR` definitions are
catalogued but were not activated or emitted as top-level markers in this corpus.
Observed player Airstrike deployments use the `ClusterBomb_*` definitions.

## Reproduction

```bash
scripts/wic_sdf.py binaries/game/wic*.sdf binaries/server/wic_ds.sdf \
  --match 'support|tactical' --json findings/sdf-ta-entry-inventory.json

scripts/timeline-attribution-audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --json findings/timeline-attribution-audit-v3.json

scripts/ta_support_catalogue.py \
  --marker-audit findings/timeline-attribution-audit-v3.json \
  --json findings/ta-support-catalogue.json
```

## Implemented boundary

Canonical parser commit `bbdda61` populated `supportName` for the 58 proven
top-level faction definitions without changing the raw `supportId`. It deliberately
leaves internal child effects and special abilities unnamed, so a presentation
layer cannot mistake them for separate player-selected aids merely because they use
the same marker envelope.

Parser schema-v8 implementation commit `fedc579` adds named `tacticalAidDeployed`
events from the
both-faction delayed-spawn stream. The viewer builds its Tactical Aid table from
those deployments and enriches equal exact support/position groups with the
visible-faction marker's serialized player. Unmatched deployments remain
`Unknown player`. A recorder-only `tacticalAidUsed` cost is attached only when
support ID, exact serialized position, player ID, stream order, and the complete
one-to-one assignment are unique. Duplicate raw records are suppressed only in the
human projection; the evidence cache retains them.

Follow-up schema-v9 parser commit `7b2756b` adds optional
`unitSpawnOwnership` players for the nine validated unit-drop definitions. Viewer
commit `3c86822` pins that parser, displays the participant as an exact player, and
uses `timeline-v9/detail-v5` to invalidate older cached details. Non-unit,
ambiguous, and unmatched deployments retain the schema-v8 team-only boundary.

The friendly viewer category is a derived label. The raw cache retains the support
ID and canonical internal definition name. Unknown raw records display their
hexadecimal ID rather than acquiring a guessed name.

Manual replay playback remains the final corroboration step for the human-facing
categories and selected examples. A contradiction blocks the affected display
mapping but does not erase the exact raw ID/internal-name relationship.

The bounded two-replay checklist in
`findings/tactical-aid-manual-review-2026-08-17.md` covers all 18 observed
multiplayer families plus both-faction row counts, recorder-cost enrichment,
duplicate suppression, and internal-marker filtering.
