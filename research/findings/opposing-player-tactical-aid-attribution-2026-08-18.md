# Opposing and spectator tactical-aid player attribution — 2026-08-18

## Result

The single-replay evidence supports exact opposing/spectator player attribution for
the nine faction unit-drop aids, but not for every tactical aid.

The successful bridge is `SupportThingSpawnedDelayed` → shipped support definition
→ `UnitCreate`. The dedicated server creates a unit-drop aid's exact configured unit
inside the issuing player's container and marks it `aSpawnSource=1`; the replay then
serializes that unit's player, faction, type, position, and spawn source. A bounded
join that requires one candidate owner agrees with 52,875 independently
player-bearing marker deployments and contradicts none.

Timeline schema v9 now exposes that result as optional deployment fields:

```json
{
  "playerId": 7,
  "playerAttribution": "unitSpawnOwnership"
}
```

Ambiguous or unmatched unit drops remain `null`. Every non-unit aid still remains
player-unknown when no player-bearing marker is visible. This is a real serialization
gap: request records, projectile IDs, score/economy fields, feedback records, and the
server's Python callback do not provide a credible general player join.

Consequently the requested ideal of zero unknown TA players cannot be reached from
the current single-replay evidence without inventing identities. Alternate-POV
alignment remains the next general route when suitable recordings become available.

## Evidence identity

| Input | SHA-256 |
|---|---|
| `binaries/client/wic.exe` | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` |
| `binaries/server/wic_ds.exe` | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` |
| `binaries/server/wic_ds.sdf` | `fd6bbe78096f1f5af77cf66cb08423107b54f8cafb25112ebce7a650bd7086b3` |
| decoded `maps/supportweapons.ice` | `e046924206a1ed849fb79ce417e4b33579c3812f1712fe338b51ca4df556e94c` |

The decoded support database is 895,291 bytes. Binaries and replay corpora were
read-only throughout. Ghidra inspection was read-only and changed no project
annotations.

## Proven unit-drop ownership path

### Binary and shipped-data facts

- `wic.exe:0x00b845d0` serializes `UnitCreate`, including `aPlayer`, `aTeam`,
  `aUnit`, `aType`, `aPosition`, a one-byte replacement flag, and
  `aSpawnSource`. Its client-side creation caller is at `wic.exe:0x00941600`.
- `wic_ds.exe:0x00520010`, the `PP_UNITSPAWNER` branch of
  `EXG_Projectile::ExecuteDeathParasites`, resolves the player container through
  `FUN_004b3dc0(*(byte *)(projectile+0x1c))`, obtains the configured unit type,
  and calls `FUN_004c7da0` with spawn source `1`.
- `wic_ds.exe:0x004cb020` propagates that spawn source from a spawned squad root to
  its child units.
- `SupportThingSpawnedDelayed` itself still has no player field. Its serializer is
  `wic.exe:0x00b87030` and its receive path is `wic.exe:0x0093fcd0`.
- The independent exact-player ground truth is `SupportThingMarker`, serialized at
  `wic.exe:0x00b86e70` and received near `wic.exe:0x0093fa10`.

The shipped support definitions map the unit drops as follows:

| Faction | Support ID / internal name | Created unit type / ID |
|---|---|---|
| USA | `0x2a640597` `Paratrooper_US` | `US_Squad_Airborne` `0x39740697` |
| USA | `0x2861054e` `Repair_Jeep_US` | `US_HUMWEE` `0x0e9802d3` |
| USA | `0x22dc04ed` `Light_Tank_US` | `US_Sheridan` `0x184f0436` |
| NATO | `0x36370621` `Paratrooper_NATO` | `NATO_Squad_Airborne` `0x43810721` |
| NATO | `0x33a205d8` `Repair_Jeep_NATO` | `NATO_LandRover` `0x23ac051f` |
| NATO | `0x2d5b0577` `Light_Tank_NATO` | `NATO_AMX30` `0x107902db` |
| USSR | `0x368a063c` `Paratrooper_USSR` | `USSR_Squad_Airborne` `0x4569073c` |
| USSR | `0x33f505f3` `Repair_Jeep_USSR` | `USSR_UAZ469` `0x14f00340` |
| USSR | `0x2dae0592` `Light_Tank_USSR` | `USSR_Bmp_R` `0x12b5037d` |

### Emitted rule

For one of those nine support IDs, schema v9 computes deployment time as the
delayed-spawn message time minus `aTimeSinceCreation`, then considers only
`UnitCreate` records satisfying all of these conditions:

1. `aSpawnSource == 1`;
2. exact shipped unit type;
3. same serialized faction/team;
4. unit arrival 29–35 seconds after an airborne-infantry deployment, or 16–22
   seconds after a transport/light-tank deployment;
5. horizontal distance at most 30 units for infantry or 15 for vehicles; and
6. every compatible unit is owned by one player slot.

The aid receives that player only when the candidate-owner set has cardinality one.
Several squad children may be present, but they must all name the same owner. A
broader exploratory 120-second/256-unit rule produced 29 marker contradictions and
is retained only as a rejected research baseline; it is not in the parser.

## Corpus validation

The forensic audit read all 2,880 linked replay paths, including the known corrupt
fixtures retained for evidence accounting.

| Unit-drop validation result | Deployments |
|---|---:|
| total nine-definition deployments | 111,002 |
| strict player attributed | 92,236 (83.09%) |
| agrees with independent marker player | 52,875 |
| contradicts independent marker player | **0** |
| marker ground truth present but strict rule unresolved | 1,875 |
| attributed despite no player-bearing marker | 39,361 |
| no marker and still unresolved | 16,891 |

The strict rule resolves 96.58% of unit-drop deployments that have independent
marker ground truth. More importantly for this investigation, it resolves 69.97% of
the unit drops whose player was previously unavailable.

New no-marker identities by recorder state are:

| Recorder state at deployment | Newly attributed |
|---|---:|
| ordinary player | 29,117 |
| one-team spectator | 74 |
| all-teams spectator | 10,170 |

By aid family, the strict rule attributes 62,915 airborne-infantry, 18,652
light-tank, and 10,669 transport deployment records.

The production release-parser gate separately accepted 2,865 replays and rejected
the 15 established corrupt/empty paths. It emitted schema v9 for every accepted
replay: 223,426 total top-level TA deployments, of which 91,932 carry
`unitSpawnOwnership`. There are zero invalid deployment player slots, zero
deployment players without participant names, zero missing participant references,
zero invalid factions, and zero deployment timestamps outside the timeline.

## Opposing and spectator examples

`replays/main/4600.wicdemo` is recorded by USSR player `[-->].CrEativE.`. Schema v9
recovers five opposing USA deployments with players that were absent from the
recorder-visible marker stream: three for `[NONWO]Urza`, one for `[GROT>]Chana`, and
one for `$k!lL€d g3R`.

`replays/main/4v4 MM Riviera 2010 #3 Armor Rape.wicdemo` is an all-teams spectator
recording. Schema v9 resolves 24 NATO unit drops across players 2, 4, and 6, plus 18
USSR drops across players 0, 7, and 9.

`replays/main/old/1v1 Ben Dover vs Jaroslav Seaside spec view.wicdemo` changes
between all-teams and one-team spectator modes. The parser resolves three USA drops
to player 1 and seven USSR drops to player 2. One USSR airborne deployment has no
compatible spawned unit and deliberately remains unknown.

These are parser-output checks, not a claim of visual review. Manual playback is
left to the user's requested final review phase.

## Other routes exhausted

| Candidate bridge | Evidence | Conclusion |
|---|---|---|
| `RequestSent` requester | `wic.exe:0x00b81b70` serializes request type/ID/creator/TTL/optional unit/position/TA; receive parsing is near `0x00778b90` | A request is team communication, not a purchase. Of 81,662 requests, only 395 name a TA. None has an exact-position marker within its TTL. Of 61 same-type marker cases, 55 exclude the requester, five contain only the requester, and one contains requester plus others. Rejected as actor evidence. |
| Support-projectile ID → normal projectile/shooter | Support projectile serializer at `wic.exe:0x00b86340`, caller near `0x0093f990` | Across 1,713,693 support-projectile records, all have zero trailing bytes. Their 1,710,727 distinct IDs have zero per-replay overlap with 26,983,987 normal-projectile IDs. No shared identifier bridge exists. |
| `SupportThingSpawnedDelayed` / feedback payload | Client serializers and complete BinTag field walks | Exact TA/faction/effect, but no player or hidden trailing payload. This remains the global team-level stream. |
| Server `OnTAExecuted` | `wic_ds.exe:0x004df860` retains the actor, deducts points, creates the barrage, and calls Python dispatch near `0x004e88a0` with player slot plus TA ID | The callback is server-local. SDF inspection finds the event declaration and a single-player map consumer, but no general multiplayer replay/network record. |
| End-game score | `wic.exe:0x00b82a30` serializes player position and aggregate `aTacticalAidScore` in `SetScoreAtGameEnd` | Aggregate impact score is neither TA spend nor event-specific and cannot assign deployments. Live `SetScore` exposes only total score. |
| Honors and transfers | `ChangeHonors`, `SupportThingUsed`, `TacticalAidTransferred` corpus traces | Honors purchases are recorder-local. Transfers give exact sender, receiver, and amount but not the eventual spender/event. Team balance equations are underdetermined without every player's ledger. |
| Role, eligibility, proximity, price, or timing | roster/timeline-derived constraints | Useful for ranking hypotheses but not proof. Multiple teammates can share role/access, prices change by bundle/role/mode, and position/timing has observed collisions. Not emitted as identity. |

The client and dedicated-server paths were both inspected. The server demonstrably
knows the actor before it deliberately serializes player-free global effects; no
stable request ID, event ID, projectile ID, score delta, or companion multiplayer
message was found that reconnects the non-unit effects to that actor in a lone
replay.

## Implemented contract

The canonical parser worktree now uses timeline schema v9:

- `tacticalAidDeployed.playerId: number | null`;
- `tacticalAidDeployed.playerAttribution: "unitSpawnOwnership" | null`;
- coverage value
  `bothFactionsWithValidatedUnitDropPlayers`;
- unit creation remains internal attribution state and is not emitted as timeline
  noise; and
- every attributed deployment player participates in the canonical participant
  directory and player-reference integrity checks.

The viewer worktree accepts both schema-v8 and schema-v9 coverage, resolves the new
player through the participant directory, and labels the deployment `Exact player`.
Its raw timeline description retains the `unit-spawn ownership` provenance.

The implementation is now locally committed child-first: parser `7b2756b`, viewer
`3c86822`, and parent workspace `0d6e836`. The viewer pins that parser and uses
`timeline-v9/detail-v5`, so opening the corrected build invalidates schema-v8 detail
rows before reparsing. All three commits remain unpublished until they are pushed
parser, viewer, then parent; see `TODO.md` for the delivery checkpoint.

## Reproduction

```bash
scripts/opposing_ta_player_audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --summary-only --json findings/opposing-ta-player-audit-summary.json

scripts/ta_request_bridge_audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --json findings/ta-request-bridge-audit.json

scripts/ta_projectile_bridge_audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --json findings/ta-projectile-bridge-audit.json

cargo build --release --locked --all-features \
  --manifest-path components/replay-parser/rust_parser/Cargo.toml

scripts/timeline-corpus-check.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --binary components/replay-parser/rust_parser/target/release/wic_replay_parser \
  --json findings/timeline-corpus-v9.json
```

Ignored machine-local audit outputs:

| Artifact | SHA-256 |
|---|---|
| `findings/opposing-ta-player-audit-summary.json` | `b873b88827ea861bf1c3cae59c9317e15a8780ba42b50c2fcc977d7b8d6c69c1` |
| `findings/ta-request-bridge-audit.json` | `ca7c9b8625957f1d1c4c1175ae26adb2c6fed6e6777067666e5cc49dfbcd7541` |
| `findings/ta-projectile-bridge-audit.json` | `5d7a08e73516cb3dad9cb50ffb7cbe6ca5f407382c86b7b27d9e130f9742d42b` |
| `findings/timeline-corpus-v9.json` | `6e9bfdcc39716f789cfb6ab4845d021da715e632a1591a8c515d725e4b24d644` |

## Remaining work

1. Manually review representative schema-v9 unit-drop rows in playback.
2. Commit the parser, then advance the viewer's nested parser Gitlink and cache key,
   commit the viewer, and finally advance both parent Gitlinks when publication is
   authorized.
3. If full player attribution is still required for artillery, bombing, nuclear,
   recon, and other non-unit aids, collect alternate POV recordings of the same
   match and implement exact event alignment using replay hashes and shared effect
   sequences.
