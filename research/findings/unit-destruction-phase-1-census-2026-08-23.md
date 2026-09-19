# Remaining unknown complete-unit destruction census

## Scope

Phase 1 measures only player-owned complete units whose canonical schema-v17
destruction cause remains `unknown`. It excludes the 27 explicit infantry-member
types, world-owned/ownership-unknown objects, all non-sentinel killer records, and
the exact-target tactical-aid subset already promoted in schema v17.

The source audit is `scripts/unit_destruction_attribution_audit.py` schema v5. The
full ignored report is `/tmp/unit-destruction-phase1-corpus-v5.json`, SHA-256
`8f91f4cd8fbef3a0fffc87cf348eb46e4b91ad59ec41c578f1f6e5908372b910`.
It covers 2,880 deduplicated replay paths with zero failures. Candidate windows in
this report are census features only; they do not establish causality.

## Population

| Population | Deaths |
|---|---:|
| All schema-v17 complete-unit destructions | 713,051 |
| Canonical cause still unknown, including world/unknown ownership | 213,539 |
| Player-owned complete units with sentinel 512 before schema-v17 TA recovery | 186,596 |
| Exact-target TA deaths removed by schema v17 | 15,642 |
| **Remaining player-owned complete-unit unknowns** | **170,954** |

The gap between 213,539 and 170,954 is deliberately out of this player-facing
phase: it consists of world-owned or ownership-unknown complete objects.

## Candidate-evidence census

| Evidence within the existing audit windows | Deaths |
|---|---:|
| Both ordinary and support projectiles | 120,913 |
| Ordinary projectile, no support projectile | 31,252 |
| Support projectile, no ordinary projectile | 15,718 |
| Neither ordinary nor support projectile | 3,071 |
| Any ordinary projectile | 152,165 |
| Any support projectile | 136,631 |
| Recent TA deployment | 61,547 |
| Inside a serialized TA marker lifetime | 137,483 |
| Same-tick explosion | 80,088 |
| Last serialized victim position available | 145,088 |
| Exact-target support projectile but schema-v17 ambiguity/eligibility abstained | 9 |

The dominant 120,913-death overlap disproves any rule that chooses TA or ordinary
fire merely because one kind of projectile is nearby. Marker lifetime is also
weak by itself: markers are long-lived, recorder-visibility-dependent, and do not
prove that a victim occupied the damaging footprint.

## Multi-player death structure

Temporal clusters partition consecutive deaths within each replay. A cluster is
counted here only when at least two distinct victim players occur. These are
candidate structures, not proof that one strike caused every death.

| Maximum adjacent gap | Unknown clusters | Unknown deaths in clusters | Exact-TA control clusters | Exact-TA deaths in clusters |
|---|---:|---:|---:|---:|
| 0.25 seconds | 10,125 | 38,614 | 1,531 | 4,311 |
| 1 second | 14,188 | 61,160 | 1,776 | 5,518 |
| 3 seconds | 17,860 | 83,564 | 2,022 | 6,792 |

The exact-target controls confirm the product requirement: one tactical-aid action
can destroy units belonging to several players. Canonical output must therefore
remain one destruction event per victim unit. The viewer should show every
attributed loss after the deployment rather than collapsing a nuke, carpet bombing,
tank-buster pass, or other strike into one generic kill row.

## Leading victim definitions

| Shipped unit definition | Unknown deaths |
|---|---:|
| `USSR_Squad_Airborne` | 24,671 |
| `USSR_Squad_Infantry` | 16,537 |
| `US_Squad_Airborne` | 15,805 |
| `US_Squad_Infantry` | 10,312 |
| `USSR_T80U` | 9,246 |
| `USSR_SA13_Gopher` | 8,859 |
| `NATO_Squad_Airborne` | 7,018 |
| `US_Tank_Abrahms` | 6,892 |
| `US_M730A2_AA` | 6,287 |
| `USSR_Bmp_R` | 5,745 |
| `NATO_Squad_Infantry` | 5,312 |
| `US_Sheridan` | 3,963 |
| `USSR_Squad_Mechanized` | 3,601 |
| `USSR_VT55` | 3,230 |
| `USSR_UAZ469` | 2,957 |

Complete infantry squad parents remain the largest class even after individual
soldier deaths are removed. This is expected gameplay evidence, not the earlier
member/squad presentation duplication.

## Leading nearby support definitions

Counts are deaths with at least one nearby projectile of that support ID. One death
may appear under several IDs, so this table must not be summed.

| Support definition | Deaths |
|---|---:|
| `SPECIAL_Direct_Artillery` | 22,571 |
| `Tankbuster_USSR` | 18,105 |
| `CHILD_LightArtilleryBarrage_USSR_2` | 16,310 |
| `CHILD_LightArtilleryBarrage_USSR` | 14,209 |
| `Tankbuster_US` | 13,800 |
| `TacticalNuke_USSR` | 13,604 |
| `HeavyArtilleryBarrage_USSR` | 13,545 |
| `CHILD_LightArtilleryBarrage_US_2` | 11,797 |
| `CHILD_LightArtilleryBarrage_US` | 10,204 |
| `TacticalNuke_US` | 10,130 |
| `ClusterBomb_USSR` | 9,716 |
| `HeavyArtilleryBarrage_US` | 8,750 |
| `CHILD_LightArtilleryBarrage_USSR_3` | 8,378 |
| `ClusterBomb_US` | 6,939 |

This ranking makes area-effect TA the highest-value next phase, especially
artillery, tank busters, tactical nukes, cluster bombs, and their child effects.
The presence of `SPECIAL_Direct_Artillery` also requires a strict distinction
between player unit abilities and purchased tactical aid.

## Phase conclusion

Phase 1 does not justify another parser attribution rule. It narrows the next
research target to the support damage-instance path: connect a serialized
deployment/support team to an impact or server damage source and from that source
to every victim lifecycle. Spatial footprint or time alone is insufficient.
Player deletion remains a separate controlled-instrumentation phase because its
replay-visible blink signal is not unique.
