# Multi-POV tactical-aid player attribution — 2026-08-22

Corpus test of whether aligning two or more recordings of the same match recovers
the opposing team's exact tactical-aid players. It does, exactly and without
ambiguity, for every deployment that produced a marker in some point of view.

Reproduce with `scripts/multi_pov_attribution_audit.py`; see the bottom of this
file. Observations are separated from the conclusions drawn from them.

## Why a single replay cannot do this

`SupportThingMarker` carries the issuing player slot but follows one faction only.
`spectator-tactical-aid-coverage-2026-08-18.md` establishes the boundary; a
re-check of `findings/spectator-ta-audit.json` for this work quantifies it:

- 2,600 of 2,603 replays with markers carry exactly **one** faction's markers;
- the three exceptions are ordinary player recordings whose recorder changed team,
  so the stream followed them; and
- **all 439 all-team spectator windows carry one faction**, so spectating all
  factions does not widen marker coverage.

One-team spectating is the exception that *is* controllable: across 7 windows the
marker faction equals the selected team, with 0 mismatches corpus-wide.

## The join is exact, not statistical

Both clients receive the same server message, so a deployment's serialized
`(supportId, position, team)` triple is bit-identical across recordings. Position
is a float triple, which makes the key effectively a unique event identifier.

On the reference pair `697__…Round_2` (USSR POV) and `698__…USA` (USA POV) of one
ESL match:

| Property | Observation |
|---|---|
| Roster | Identical: 10 players, same names, same slot IDs, same factions |
| Slot IDs | Identical across recordings, so no name bridging is needed |
| Match timing | `matchDurationSeconds` 637.939 both; clock bounds 1199.9 → 561.961 both |
| Deployments | 63 in each; **exact multiset match** on the full key |
| Key uniqueness | 63 distinct keys, **0 duplicates within a replay** |
| Marker overlap | **0** — one carries USSR slots 5-9, the other USA slots 1-4 |

No timestamp alignment is involved. Recording clocks have per-recording origins
(timeline schema v13) and are never compared.

## Corpus result

The audit scanned 2,880 replay paths. A cheap `(serverName, dateTime, gameMode)`
bucket proposed 796 candidates; every pair was then re-tested on roster agreement
and shared deployment keys.

| Measure | Value |
|---|---:|
| Confirmed multi-POV groups | 22 (21 pairs, 1 triple) |
| Buckets that collapsed to one POV after de-duplication | 353 |
| Same-POV re-uploads dropped | 369 |
| Pairs rejected by the strong test | 20 |
| Deployments in confirmed groups | 4,222 |
| Attributed by a single replay | 1,938 (45.9%) |
| Attributed using the group | **3,037 (71.9%)** |
| Newly attributed | **1,099** |
| Ambiguous | **0** |
| Independent cross-confirmations | **826** |
| Contradictions | **0** |

Every one of the 22 groups gained attributions, and none produced an ambiguous or
contradicted result.

### Cross-confirmation is independent evidence

The 826 confirmations are not self-agreement. A replay attributes some deployments
through the schema-v9 unit-drop `UnitCreate` ownership bridge; the other recording
attributes the same events through a serialized marker `playerId`. These are
unrelated mechanisms, and they agree 826 times with zero disagreement.

### A third point of view closes a match completely

The one three-POV group (`do_Seaside`, 2009-05-19 21:58) contains two player
recordings and a spectator recording:

| Recorder | Alone | With group | Gained |
|---|---:|---:|---:|
| `sh.es^Werek` | 74/165 | 131/165 | 57 |
| `[xya]New Arrivals` | 76/165 | 129/165 | 53 |
| `chaPeL One` (spectator) | 92/165 | **165/165** | 73 |

The spectator recording reaches complete attribution. The two player recordings do
not, because neither sees the markers the other one lacks.

### Rejections behave as intended

| Reason | Pairs |
|---|---:|
| `insufficientSharedDeployments` | 17 |
| `rosterConflict` | 3 |

The `insufficientSharedDeployments` rejections include the established stat-padding
and corrupt fixtures, which serialize no top-level deployments at all.

## The residual limit

1,185 deployments across the confirmed groups remain unattributed after merging.
These are events with **no marker in any available point of view**, led by
`Tankbuster` (371 across factions), `ClusterBomb` (118), `LightArtilleryBarrage`
(111), `Paratrooper`, `AntiAirstrike`, and `HeavyArtilleryBarrage`.

Marker emission is partial even for a recorder's own faction: in the reference
replay `697` there are 35 markers for 63 deployments. This is a serialization gap
rather than a visibility gap, so adding points of view does not reliably close it.
Expect a practical ceiling near 72% for a pair and higher for three, not 100%.

## Why the ground truth does not yield a single-replay rule

The labelled attributions are validation data, not training data. The actor is not
serialized in the deployment record, and
`opposing-player-tactical-aid-attribution-2026-08-18.md` exhausted every companion
record that might carry it: `RequestSent`, support-projectile IDs, the delayed-spawn
and feedback payloads, server `OnTAExecuted`, end-game score, and honors/transfers.
The dedicated server demonstrably holds the actor at `wic_ds.exe:0x004df860` before
serializing player-free global effects. Multi-POV works because a second recording
contains the field, not because the first one implies it.

That document lists role, eligibility, proximity, price, and timing as rankable but
unprovable. A corpus re-test over 155 replays and 10,812 player-bearing markers
quantifies the role half of that row:

- **52 of 53 aid families are called by all four roles.** Tactical aid is not
  role-gated. `Tankbuster_USSR` splits armor 93 / infantry 78 / air 72 / support 60,
  which is close to uniform.
- The strongest skews are still only skews: `Paratrooper_USSR` is 83% infantry,
  `Repair_Jeep_USSR` 86% support, `AntiAirstrike_USSR` 74% support.
- Always guessing an aid's most common role is **53.9%** accurate.
- **Role would not be sufficient even if it were exact.** In 39.0% of cases another
  player on the same team shares the actor's role, so the constraint does not reduce
  to one player.

A perfect role oracle therefore tops out near **33%** identification. Compare the
multi-POV join on the same corpus: 0 ambiguous and 0 contradictions across 4,222
deployments. A single-replay heuristic would be wrong about two-thirds of the time
while presenting as a fact, which is the failure mode the parser's fact/inference
separation exists to prevent.

The labelled set does have a standing use: it is now a test harness. Any future
candidate bridge — for example one found by walking the callers of `0x009240a0` to
enumerate every serializable message — can be measured against it directly, and
accepted only if it is exact rather than correlated.

## Conclusions

- Multi-POV alignment recovers the opposing team's exact players soundly. The join
  is exact, ambiguity was zero across 4,222 deployments, and two independent
  attribution mechanisms agreed 826 times without a single contradiction.
- A deployment with no marker anywhere stays unknown and must be reported as
  unknown. Attribution must never be inferred from cost, timing, or proximity.
- Any implementation belongs in an explicitly derived layer, not the raw parser,
  and must record the contributing replay hashes and matched key as provenance.
- No single-replay rule recovers this. The actor is absent from the file, role is
  not a constraint, and a perfect role oracle would reach roughly 33%.
- Attribution requires recordings whose marker streams cover different factions.
  A player from each side works; a player plus a one-team spectator locked to the
  other side works; a player plus an all-team spectator is uncontrolled.

## Reproduction

```bash
cargo build --release --locked --all-features \
  --manifest-path components/replay-parser/rust_parser/Cargo.toml

scripts/multi_pov_attribution_audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --binary components/replay-parser/rust_parser/target/release/wic_replay_parser \
  --json findings/multi-pov-attribution-audit.json
```

The ignored detailed audit is `findings/multi-pov-attribution-audit.json`,
SHA-256 `ce8fbbeefa5b24bb84bdfa7f01c48db808e1af7430e780b796afbc36214733d7`.
