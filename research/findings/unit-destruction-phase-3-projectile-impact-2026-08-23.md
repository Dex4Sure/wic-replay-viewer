# Support-projectile impact bridge audit

## Scope

Phase 3 tests the remaining replay-only bridge from a faction tactical-aid
projectile to a qualified Phase 2 explosion and from that projectile's support ID
to one deployment team. This is audit schema v7 only; it does not change the
canonical parser schema.

The full ignored report is `/tmp/unit-destruction-phase3-corpus-v7.json`, SHA-256
`7edfabed5a81274be68e76426ae1a49ab2bb794d9a3922f1d805a49255a66c74`.
It covers 2,880 deduplicated replay paths with zero failures and uses the same
15,642 complete-unit exact-target TA controls as Phase 2.

## Candidate rule

For each Phase 2 blast candidate, the audit:

1. considers only earlier faction-TA support projectiles within three seconds;
2. treats the serialized vector as constant velocity and projects the projectile
   origin to the explosion time;
3. requires the projected point to fall within a tested distance of the explosion;
4. requires exactly one recent deployment team for that support ID; and
5. accepts only one surviving `(support ID, team)` pair.

The constant-velocity interpretation is explicitly a hypothesis. The replay has no
per-projectile destruction/impact record with which to verify the join directly.

## Corpus result

| Maximum projected distance | Exact controls matched correctly | Exact controls matched wrongly | Controls with multiple causes | Remaining unknowns with one cause | Remaining unknowns with multiple causes |
|---|---:|---:|---:|---:|---:|
| 1 unit | 0 | 0 | 0 | 44 | 0 |
| 3 units | 24 | 0 | 0 | 541 | 0 |
| 5 units | 98 | 0 | 0 | 2,887 | 0 |
| 10 units | 486 | 3 | 0 | 11,407 | 0 |
| 25 units | 2,647 | 26 | 5 | 23,373 | 18 |
| 50 units | 6,826 | 28 | 69 | 25,115 | 60 |

The 3- and 5-unit thresholds have no observed mismatches but recover only 0.15%
and 0.63% of the controls, respectively, while lacking binary proof that the vector
is linear velocity. The first threshold with material unknown coverage, 10 units,
already selects three wrong causes. Wider thresholds increase both wrong and
competing causes.

One 10-unit counterexample occurs in
`414__sKpl_CS-E_1_2.wicdemo`: schema v17 exactly identifies
`CHILD_HeavyAirSupport_USSR_AT_2` against unit 206, while the trajectory rule
selects an overlapping `ClusterBomb_USSR` projectile from the same team. The
spatial bridge therefore confuses simultaneous TA effects even when it returns one
apparently unique cause.

## Conclusion

Constant-vector projectile-to-explosion matching is rejected for parser
attribution at every tested threshold. Tight thresholds are unvalidated and too
sparse to address the unknown majority; useful thresholds have demonstrated false
attribution. No parser or viewer output changes are justified.

The next phase is controlled self-deletion investigation: capture or identify the
server command path used when a player removes one of their own units, then compare
its exact replay serialization with combat sentinel-512 deaths. The existing blink
signal is not sufficient because it is shared by other game behavior.
