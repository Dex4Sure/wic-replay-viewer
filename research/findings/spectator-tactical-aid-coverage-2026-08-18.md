# Spectator and opposing-faction tactical-aid coverage — 2026-08-18

> Follow-up: timeline schema v9 now reconstructs exact players for nine validated
> unit-drop definitions through `UnitCreate` ownership. The marker/effect visibility
> boundary below remains correct for all other aids; see
> `opposing-player-tactical-aid-attribution-2026-08-18.md` for the new bridge and
> its limits.

## Result

A single replay can recover the exact tactical-aid type, faction, position, and
deployment time for both sides. Exact player identity has a narrower boundary:

- `SupportThingMarker` carries the issuing player slot, but its stream follows the
  recorder-visible faction rather than covering the whole match;
- top-level `SupportThingSpawnedDelayed` covers both factions in ordinary player,
  one-team spectator, and all-team spectator recordings, but carries no player
  slot; and
- `SupportThingUsed` plus `ChangeHonors` remains the recorder-only purchase/cost
  ledger.

Therefore enemy/team TA usage is trackable from one replay. Enemy **player** usage
is not generally trackable from that replay and must remain unknown unless its
player-bearing marker is present, schema v9 proves a unit-drop owner, or another
point-of-view recording supplies one.

## Exact spectator fields

`wic.exe:0x00b87520` serializes `SpectatorJoinedTeam` as `aSlot`, `aTeam`, and
`aSpectatorLos`. The caller at `wic.exe:0x00939be0` stores the selected team and LOS
mode in the player state. The current corpus contains only these validated shapes:

| Serialized values | Meaning | Occurrences |
|---|---|---:|
| `aTeam` 1/2/3, `aSpectatorLos` 1 | spectate one selected faction | 14 |
| `aTeam` 0, `aSpectatorLos` 2 | spectate all factions | 788 |

The parser retains the last valid pre-game state at timeline time zero and emits
later changes in stream order. It does not mistake discarded pre-game UI toggles for
gameplay view changes.

## Effect-stream boundary

`wic.exe:0x00b86e70` serializes `SupportThingMarker`, including its nominal `aTeam`
field that binary tracing and the recorder-ledger cross-check prove is a player slot.
Across 109,854 top-level markers, 109,853 agree with the issuing player's known
faction, one lacks faction state, and none disagree. During one-team spectator
windows, no marker belongs to a faction other than the selected faction.

`wic.exe:0x00b87030` serializes `SupportThingSpawnedDelayed` with support ID,
position, faction, upgrade, direction, and `aTimeSinceCreation`. It omits the player
slot. Restricting this stream to the 58 proven top-level faction definitions yields
224,126 deployment effects across the linked corpus. Subtracting
`aTimeSinceCreation` from the replay-relative receipt time recovers the deployment
time used by the viewer. All 224,126 serialized `aTeam` values agree with the
faction encoded by the exact support definition; there are zero mismatches or
unknown faction values.

The distinction is visible in the fixed reference replay `4600.wicdemo`: its 41
player-bearing markers are USSR-only, while its 86 top-level delayed-spawn effects
contain 41 USSR and 45 USA deployments. In the all-team spectator sample
`4v4 MM Riviera 2010 #3 Armor Rape.wicdemo`, 52 NATO markers carry players while
delayed-spawn effects contain 52 NATO and 39 USSR deployments.

## Corpus measurements

The read-only audit processed 2,880 linked replay paths, including established
corrupt fixtures retained for evidence accounting. Recorder spectator state occurs
in 715 paths.

| Recorder state containing top-level delayed spawns | Replays | Both factions observed |
|---|---:|---:|
| ordinary player/non-spectator window | 2,164 | 2,138 |
| one-team spectator window | 7 | 5 |
| all-team spectator window | 710 | 695 |

A replay can contribute to more than one row when the recorder changes view. An
absence of the second faction is not a coverage failure when that faction deployed
no top-level TA during the recorded interval.

The ignored detailed audit is `findings/spectator-ta-audit.json`, SHA-256
`4c0244e011e7f8f81b62331c8fad6904ed63afb5fdfb014f73cb9b6f978365bd`.
The independent release-parser corpus gate accepted 2,865 replays, rejected the 15
established corrupt/empty paths, and emitted 223,426 schema-v8 top-level deployments
with zero invalid factions, unknown spectator LOS modes, or timestamps beyond the
observed timeline. Its ignored output is `findings/timeline-corpus-v8.json`, SHA-256
`6e1951cb3800fa1fba9bbb3b6f19df1e72bd6193951352fa0f08060fae94b584`.

## Implemented contract

Timeline schema v8 adds:

- `spectatorViewChanged`, preserving serialized team, LOS, and decoded view mode;
- `tacticalAidDeployed`, preserving both-faction top-level deployment facts without
  a player ID;
- coverage values `visibleFactionWithPlayer` for markers and
  `bothFactionsTeamOnly` for deployments.

The viewer projects marker and deployment records through exact support/position
groups. Equal groups retain the marker's exact player and the deployment's faction.
Unequal groups remain team-only rather than choosing a player. Recorder cost is
still attached only through the independently conservative purchase/marker link.

## Reproduction

```bash
scripts/spectator_ta_audit.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --catalogue findings/ta-support-catalogue.json \
  --json findings/spectator-ta-audit.json

cargo build --release --locked --all-features \
  --manifest-path components/replay-parser/rust_parser/Cargo.toml

scripts/timeline-corpus-check.py \
  replays/main replays/settings replays/wicgate-documents --jobs 4 \
  --binary components/replay-parser/rust_parser/target/release/wic_replay_parser \
  --json findings/timeline-corpus-v8.json
```
