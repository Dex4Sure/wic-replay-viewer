# Area-effect destruction attribution audit

## Scope

Phase 2 tests whether serialized explosion records can safely extend tactical-aid
attribution beyond schema v17's exact-target homing-projectile subset. It retains
one destruction row per complete victim unit and does not promote temporal or
spatial coincidence into parser output.

The source audit is `scripts/unit_destruction_attribution_audit.py` schema v6. The
full ignored report is `/tmp/unit-destruction-phase2-corpus-v6.json`, SHA-256
`a1c8cdc7acd5f3b6a024d0a4c8bc8f559c9a15c0d7fcef4236adc9bae067bebb`.
It covers 2,880 deduplicated replay paths with zero failures.

## Binary and wire evidence

The dedicated-server damage path explains why one strike can produce many deaths:

1. `wic_ds.exe:0x0051b020` updates a support barrage and creates its support
   projectiles.
2. `wic_ds.exe:0x00520010` executes projectile death parasites.
3. `wic_ds.exe:0x0051f0b0` enumerates units in a blast, computes damage separately
   for every victim, and calls the target damage path for each one.
4. `wic_ds.exe:0x004ce310` applies damage; its sentinel wrapper at
   `wic_ds.exe:0x004ce830` supplies killer unit ID `0x200` when there is no unit
   source.

This confirms the required output cardinality: a nuke, carpet bombing, tank-buster
pass, artillery barrage, or other area strike can kill complete units owned by
several players, and every victim must remain a separate destruction event.

The client replay writers do not preserve the source bridge:

- `wic.exe:0x00b826c0` writes `SpawnExplosion` with only `aPosition`, `aRadius`,
  `aStrength`, `anExplosionForce`, and `aForestDestroyRadius`.
- `wic.exe:0x00b82590` writes `SpawnExplosionWithCrater` with the same fields plus
  `aCraterHitEffectIndex`.

Neither message contains a projectile ID, support ID, player, team, deployment ID,
or damage-source ID. Support-projectile and ordinary-projectile ID namespaces also
have no per-replay overlap, as previously measured by
`scripts/ta_projectile_bridge_audit.py`.

The apparent projectile-destruction lead is not replay evidence. The client string
`OnTAProjectileDestroyed` is used at `wic.exe:0x006f12b0` for a runtime Python
callback and does not call the replay writer. The replay parser's
`ProjectileDestroyAll` command is a global reset and carries no individual
projectile/source bridge.

## Ordered spatial test

For each complete-unit death, schema v6 considers an explosion only when all of the
following hold:

- the explosion has the same exact raw replay timestamp as the death;
- it is serialized before the `UnitDestroy` record;
- the victim has a last serialized `UnitFrame` position; and
- that position lies within the explosion's serialized radius.

This is a candidate test only. Unit-frame positions may be stale, several blasts
can occur in one simulation tick, and the explosion still has no actor identity.

| Population | Deaths | With any blast candidate | Unique candidate | Multiple candidates |
|---|---:|---:|---:|---:|
| Schema-v17 exact-target TA controls | 15,642 | 8,726 | 8,586 | 140 |
| Remaining complete player-unit unknowns | 170,954 | 39,935 | 28,817 | 11,118 |

The control recall is only 55.8%, and even 140 already-proven TA deaths have more
than one containing same-tick blast. Among remaining unknowns, 27.8% of matched
deaths are spatially ambiguous. The corpus contains 11,543,844 parsed explosions,
including 1,190,692 crater variants that the earlier census omitted.

## Conclusion and next boundary

Explosion ordering and containment narrow 39,935 unknown complete-unit deaths to
plausible area-effect damage, but they cannot identify the tactical aid or its
user. The rule is unsuitable for canonical parser attribution and must not change
viewer labels.

The next Phase 2 step is to test the remaining projectile-to-impact hypothesis
using serialized support-projectile origin/vector data and shipped projectile and
support definitions. There is no serialized per-projectile destruction record. The
hypothesis must therefore be calibrated against exact-target TA controls and must
abstain whenever multiple support types, teams, deployments, or impacts remain
possible. If no reliable bridge exists, area-effect deaths stay unknown and the
investigation proceeds to controlled self-deletion evidence.
