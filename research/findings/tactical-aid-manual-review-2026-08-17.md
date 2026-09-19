# Tactical-aid playback review — 2026-08-17

## Purpose

This is the final visual corroboration checklist for the both-faction tactical-aid
presentation. The internal name/ID mappings are already exact shipped-data facts;
playback review checks that each friendly viewer label describes the visible effect
and that a marker-enriched player's attribution is plausible in context. Opposing-
faction delayed-spawn rows were the schema-v8 `Unknown player` baseline.

Schema v9 subsequently adds exact `unitSpawnOwnership` players for validated
airborne-infantry, transport, and light-tank drops. The older team-only expectation
now applies to non-unit aids and ambiguous or unmatched unit drops.

Mark each row `confirmed`, `unclear`, or `contradicted`. A contradiction blocks the
friendly label or presentation rule, but does not invalidate the raw support ID,
internal name, or serialized player slot.

## Review replays

Only two replays are needed to cover all 18 multiplayer aid families observed in
the corpus:

| Key | Replay | SHA-256 | Context |
|---|---|---|---|
| A | `replays/main/WicTracker/downloads/1000__demo07.wicdemo` | `6ced4ee1bd5c6a629c21c1873d7a3958ae74e66524c8afd02ba2d39deb914023` | Mauer, recorder `hermach`, 15:52.9, 47 both-faction deployments |
| B | `replays/main/WicTracker/downloads/744__demo08.wicdemo` | `7e0b67507afdf001b9df502d83784edc8f0501859ad94c4e599bc3c6b918202f` | Hometown, recorder `[HD.DK]svinehunden`, 19:54.9, 111 projected rows (110 both-faction deployments plus one marker-only effect) |

**Times are recording-elapsed seconds (timeline schema v13)**, measured from the
first event in the file, and are also shown as rounded `mm:ss` seek hints. This is
the axis the in-game replay player scrubs on, so a seek hint should now land on the
effect directly.

Rows were written against the schema-v11 countdown axis and migrated on 2026-08-22.
Each marker's old timestamp was reconstructed from the replay's own countdown
samples — for these single-phase matches the v11 axis was
`firstRemaining - runningMinRemaining` — and rows were re-matched on
`(supportId, playerId, reconstructed time)`. Reconstruction agreed to within
0.230 s on all 18 rows, and only one row had any alternative candidate, 2.713 s
away against a 0.007 s match. Times shifted by the lobby lead-in plus countdown
drift: 22.2 s at the start of replay A growing to 25.6 s by its end, and 30.8 s
across replay B. No status, player, support ID, or label was changed.
Player names resolve from time-bounded participant sessions (added in schema v11); the
player ID remains the serialized marker fact. The earlier static-directory output
misnamed replay B slot 1 as JonnySky after that occupant had left.

## Automated preparation — updated 2026-08-20

The schema-v11 parser and detail-v7 viewer projection have been checked before
playback. Run the deterministic preparation again with:

```bash
PYTHONPATH=scripts "$HOME/.venvs/wic-analysis/bin/python" \
  scripts/tactical_aid_playback_review.py \
  --json findings/tactical-aid-playback-review-packet.json
```

The generated packet has SHA-256
`dc72b6dfe51e4237354a98611dcd13090970749b7243f869787d7d27123d29dc`.
All 18 checklist rows uniquely match a schema-v13 player-bearing marker, including
the expected support ID/name, timestamp, participant ID, and event-time participant
session name. The pre-migration schema-v11 packet was
`49382d3d44709de0fca2adcc0f68753b0b54068daeee87025fa6d7a87444b2bd`.
The viewer projection produces:

| Replay | Rows | Exact-player rows | Team-only rows | Recorder costs | Excluded unnamed child markers |
|---|---:|---:|---:|---:|---:|
| A | 47 | 23 | 24 | 1 | 13 |
| B | 111 | 61 | 50 | 10 | 42 |

The viewer's private-replay projection test passes independently for both inputs.
These checks establish the data and presentation baseline only. Every visual status
below deliberately remains pending until in-game playback is observed.

## Family checklist

| Status | Replay | Time | Player | Support ID | Internal name | Viewer label |
|---|---|---:|---|---|---|---|
| [ ] | B | 508.808 (`08:29`) | `Sintronic` (13) | `0x35460642` | `AntiAirstrike_US` | Air-to-Air Strike |
| [ ] | B | 1100.437 (`18:20`) | `[WHO]LtDan73` (1) | `0x1f2104c0` | `Artillery_US` | Precision Artillery |
| [ ] | B | 625.185 (`10:25`) | `lysy` (14) | `0x3aff068f` | `BridgeRepairer_US` | Repair Bridge |
| [ ] | A | 561.459 (`09:21`) | `tezetko` (9) | `0x3d37068e` | `Bunkerbuster_NATO` | Laser Guided Bomb |
| [ ] | B | 1083.028 (`18:03`) | `Sintronic` (13) | `0x34760625` | `CarpetBombing_US` | Carpet Bombing |
| [ ] | A | 545.989 (`09:06`) | `-="III"=-IIIuHKOBKA` (11) | `0x34ea05f4` | `ClusterBomb_NATO` | Airstrike |
| [ ] | B | 564.905 (`09:25`) | `Krondan` (4) | `0x290c0579` | `DaisyCutter_US` | Daisy Cutter Bomb |
| [ ] | B | 1044.522 (`17:25`) | `[*G.A*]lt_dan_atc` (9) | `0x1d19047b` | `GasAttack_US` | Chemical Strike |
| [ ] | B | 433.549 (`07:14`) | `[WHO]LtDan73` (1) | `0x4337071e` | `HeavyAirSupport_US` | Heavy Air Support |
| [ ] | A | 479.433 (`07:59`) | `[*CIA*]Thaum` (5) | `0x8a8f09fb` | `HeavyArtilleryBarrage_NATO` | Heavy Artillery Barrage |
| [ ] | A | 325.933 (`05:26`) | `-=[FMC]=-gamermike3240` (7) | `0x8a3b09f6` | `LightArtilleryBarrage_NATO` | Light Artillery Barrage |
| [ ] | A | 481.723 (`08:02`) | `Fire Hook` (15) | `0x2d5b0577` | `Light_Tank_NATO` | Airdropped Light Tank |
| [ ] | A | 192.859 (`03:13`) | `BlueSun` (13) | `0x3ca0067d` | `Napalmstrike_NATO` | Napalm Strike |
| [ ] | A | 490.262 (`08:10`) | `Fire Hook` (15) | `0x36370621` | `Paratrooper_NATO` | Airborne Infantry |
| [ ] | A | 835.440 (`13:55`) | `tezetko` (9) | `0x26d10501` | `RadarScan_NATO` | Aerial Recon |
| [ ] | A | 486.826 (`08:07`) | `Fire Hook` (15) | `0x33a205d8` | `Repair_Jeep_NATO` | Airdropped Transport |
| [ ] | A | 948.361 (`15:48`) | `hermach` (3) | `0x7adc097e` | `TacticalNuke_NATO_British` | Nuclear Strike |
| [ ] | A | 421.848 (`07:02`) | `Marena` (4) | `0x2f9205b5` | `Tankbuster_NATO` | Tank Buster |

## Presentation edge checks

- [ ] In replay A, the Tactical Aid view shows 47 rows spanning NATO and USSR, not
  only recorder `hermach`'s one priced placement.
- [ ] In replay B, the Tactical Aid view shows 111 rows spanning USA and USSR, not
  only recorder `[HD.DK]svinehunden`'s ten priced placements.
- [ ] Non-recorder rows show an em dash for cost; the uniquely linked recorder rows
  show a numeric marginal cost.
- [ ] The human timeline does not duplicate a recorder deployment as both “used”
  and “deployed”.
- [ ] Internal artillery/heavy-air child markers and unit special-ability markers
  do not appear as separate Tactical Aid rows.
- [ ] Player names above match the visible user responsible for each reviewed aid;
  if playback cannot establish that visually, mark the row `unclear`, not confirmed.
- [ ] Opposing-faction non-unit rows show `Unknown player` with `Team only` evidence
  rather than inheriting a visible-faction player's name; validated unit-drop rows
  may show the exact spawned-unit owner.
