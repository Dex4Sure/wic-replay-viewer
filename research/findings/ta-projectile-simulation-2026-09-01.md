# Tactical-aid projectile simulation and tracking boundary

## Scope

This milestone implements research-only support-projectile decoding, catalogue
binding, lifecycle tracking, and deterministic free-flight stepping. It does not
read `UnitDestroy`, attribute deaths, or change the replay parser/viewer schema.
Its policy is exact-or-abstain: no time, distance, or proximity threshold is used.

**Final status:** closed at the static/replay boundary. Runtime game launch and
instrumentation are not being pursued, so this work is not a pending parser or
viewer feature.

The compact machine report is the ignored local artifact
`findings/ta-projectile-simulation-corpus.json`, SHA-256
`26457f4cd6203cd927110c926b60f02b6b3b8728ce5825d2a63d0d7ebbcc8c9e`.
It covers all 2,880 deduplicated replay paths and omits each replay's duplicated
end-summary pass at the recording-time reset.

## Implemented evidence model

`scripts/ta_projectile_simulation.py` validates the complete field order, field
type, and empty trailing payload for all four client serializers:

| Replay message | Client serializer | Shipped mover |
|---|---:|---|
| `ProjectileStraightSupportCreate` | `wic.exe:0x00b86340` | `STRAIGHT` |
| `ProjectileBallisticSupportCreate` | `wic.exe:0x00b86250` | `BALLISTIC` |
| `ProjectileHomingSupportCreate_Position` | `wic.exe:0x00b85fc0` | `HOMING` |
| `ProjectileHomingSupportCreate_Unit` | `wic.exe:0x00b86100` | `HOMING` |

Every accepted creation retains the serialized projectile ID, support ID,
source, vector, upgrade, and homing target fields. Its identity is
`(projectileId, creationOffset)`, so a later reuse cannot collapse two
lifecycles. The corpus contains no within-primary-chain reuse, but the compound
key makes that an observed result rather than an assumption.

The shipped `maps/supportweapons.ice` catalogue decoder covers 200 unique support
IDs and 201 serialized variants. Of those variants, 180 produce projectiles: 145
straight, 25 homing, and 10 ballistic. It preserves ordered effect bundles and
all direct scalar fields for six executable-verified parasite classes:

| Parasite class | Catalogue nodes |
|---|---:|
| `PP_BlastDamage` | 142 |
| `PP_DirectDamage` | 94 |
| `PP_ForestDestroyer` | 81 |
| `EXCO_CloudType` | 79 |
| `PP_UnitSpawner` | 24 |
| `PP_BloomBurner` | 9 |

Support ID `0x8eed0a0c` (`SINGLEPLAYER_DaisyCutter_USSR`) has two genuinely
different shipped definitions, so the decoder retains both instead of choosing
one. Neither variant occurs in this replay corpus.

## Static simulation result

The server evidence is b35 `wic_ds.exe`, SHA-256
`c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf`,
PE32 i386. The relevant path is:

1. `EXG_Projectile::Update` at `0x00520450` invokes mover/world processing at
   `0x006837d0` and then death-parasite execution at `0x00520010`.
2. The straight mover at `0x00683970` performs float32
   `position += velocity * tickDelta`.
3. The ballistic mover at `0x00683a20` first performs the same position update,
   then subtracts `9.81 * tickDelta` from vertical velocity.
4. The homing mover at `0x00684a00` additionally depends on changing target and
   world state.
5. `MI_Time::Update` at `0x0041b150` derives a variable, smoothed frame delta;
   it is not a fixed simulation tick.

The implementation reproduces straight and ballistic float32 free-flight steps
when an explicit tick-delta sequence is supplied. It deliberately does not call
that a complete impact simulator: `0x006837d0` also performs collision, ground,
out-of-bounds, and lifetime checks against world state.

## Corpus result

| Primary-chain population | Records |
|---|---:|
| Straight support creations | 1,667,858 |
| Ballistic support creations | 21,227 |
| Homing-unit support creations | 20,776 |
| Homing-position support creations | 0 |
| **All valid support creations** | **1,709,861** |
| Ordinary projectile controls | 26,974,686 |

All 1,709,861 support records have an exact wire shape and an exact shipped
support/mover binding. None has enough replay state for an exact impact:

| Impact-link status | Records | Reason |
|---|---:|---|
| `unsupported` | 1,689,085 | missing tick deltas and collision state |
| `unsupported` | 20,776 | additionally missing dynamic homing target/world state |
| `exact` | 0 | no effect record carries the projectile ID |
| `ambiguous` | 0 | no threshold-based candidate links were created |
| `unmatched` | 0 | every replay support ID exists in the shipped catalogue |
| `malformed` | 0 | every support wire record passed full-shape validation |

The ordinary-projectile control population also has zero per-replay ID overlap
with support projectiles. One malformed ordinary straight-projectile record is
retained in the ledger at decompressed offset `11157450` in
`1050__LOLATCOOLGUY.wicdemo`; it is not a support-projectile decoder failure.

Effect records were inventoried independently—11,526,276 explosions,
1,676,459 clouds, and 333,039 `UnitCreate` records with `aSpawnSource=1`—but no
effect was joined to a projectile. `SpawnExplosion`,
`SpawnExplosionWithCrater`, `CreateCloud`, and `UnitCreate` serialize no support
projectile ID.

## Deterministic boundary and final disposition

The replay is sufficient to identify a support projectile's creation and exact
shipped effect bundle, but not the frame on which the server terminated it or the
effect records produced by that termination. Raw event-time differences cannot
recover silent simulation ticks, and a finite trajectory bound cannot be derived
without inventing a tick count or collision geometry.

Therefore this milestone resolves **0%** of unknown deaths and makes no death or
player attribution claim. Unknown `UnitDestroy` deaths remain unknown, including
deaths plausibly caused by nuclear strikes, carpet bombing, artillery, gas, and
other tactical aids.

The only identified next evidence source would require launching the game and
capturing per-tick deltas, mover termination reason/position, and projectile ID at
death-parasite execution. That runtime route has been declined, so the
investigation stops here. The catalogue, strict decoders, lifecycle model, tests,
and corpus ledger are retained as reproducible evidence of the boundary; they do
not justify canonical parser/viewer changes. Reopen this question only if runtime
capture becomes acceptable or a new replay-visible causal field is independently
discovered.
