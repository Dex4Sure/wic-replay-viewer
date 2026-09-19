# World in Conflict Replay Parser

The parser is maintained in the `parser/` directory of the canonical
`wic-replay-viewer` repository. This documentation and the parser's full rewritten
history were imported from the former standalone `wic-replay-parser` repository.
Commands in this document are run from `parser/` unless they explicitly start at the
combined repository root.

Parses World in Conflict (.wicdemo) replay files to extract match results and an optional lightweight gameplay timeline. Built through reverse engineering of the game's BinTag binary serialization format.

## Files

| File                   | Description                                                                         |
| ---------------------- | ----------------------------------------------------------------------------------- |
| `wic_replay_parser.py` | Python parser (standalone CLI tool)                                                 |
| `rust_parser/`         | Rust parser (native CLI + WASM source)                                              |
| `SCHEMA.md`            | Public JSON compatibility, facts/derivations, and timeline-version policy           |
| `ARCHITECTURE.md`      | Standalone parser pipeline, boundaries, and validation design                       |
| `CHANGELOG.md`         | Historical standalone parser history; new unified changes live in `../CHANGELOG.md` |
| `parse_replay.bat`     | Windows: drag a .wicdemo onto it to parse                                           |
| `parse_all.bat`        | Windows: double-click to parse all .wicdemo in folder                               |
| `ground_truth.json`    | 17 diverse replays with expected output                                             |
| `test_ground_truth.py` | Regression harness that compares native Rust JSON against ground truth              |

The Rust crate supports native CLI and WebAssembly builds. This repository's CLI parser keeps its own built-in `map_display_name()` lookup so it works standalone. The established community table is its compatibility baseline, with verified corrections and revision paths normalized to the same public name instead of inventing numbered variants. An unknown map retains its raw replay path.

### Map-name policy

Runtime parsing uses only the built-in community-compatible table. It performs an
exact replay-path lookup, derives the game mode from the `do_`, `as_`, or `tw_`
prefix, and preserves the raw path when no entry exists. A local game installation
is never consulted at runtime for map display names, keeping native, WASM, and
cloud results identical.

Installed `maps/<map>/<map>.loc` files are development evidence for auditing the
table, not another resolution layer. Their exact
`myWicedMap.myMissionStats.myName` value can corroborate a correction, but it must
be checked against the replay path and game UI before changing the table: an
installed archive can describe a different revision under a related internal name.
Revision paths that are known aliases reuse one canonical public name. The map-name
tests require every public name to carry a recognized mode prefix, forbid obsolete
numbered aliases, and require the Python and Rust tables to remain identical.

## Features

- **Winner & domination bar** — Final domination split (e.g. `USA 65% - USSR 35%`)
- **Team detection** — Players grouped by faction (USA, NATO, or USSR)
- **Player scores** — Final scores via BinTag score hash
- **Player roles** — Primary role from per-role score totals, with an explicit end-summary role fallback for scored players whose totals are all zero
- **Recorder detection** — Identifies which player recorded the replay (100% on valid replays)
- **Game metadata** — Map name, game mode, server, date/time, duration
- **Lightweight timeline** — Countdown-derived phases, sampled domination state, recorder-visible chat, recorder-only tactical-aid purchases, exact tactical-aid actor/target damage notifications, visible-faction player-attributed markers, both-faction deployments with validated unit-drop owners where proven, and discrete gameplay events
- **Corrupt file detection** — Rejects corrupted replays (missing TeamWins event) with a clear error
- **Incomplete match detection** — Flags replays with incomplete results (map-vote, late-join, opponent left)
- **Match-start roster resolution** — Corrects confirmed lobby swaps and scored-slot replacements before gameplay, plus a narrowly verified late entrant into a vacant lobby slot
- **Unicode support** — Handles extended Latin, CJK, and symbol characters in player names
- **Event stream player fallback** — Recovers players missing from metadata (older replays)
- **Map name translation** — Converts internal paths to human-readable names (e.g. `do_Hometown`) using a built-in lookup
- **Bounded decompression** — Rejects compressed input above 64 MiB, aggregate output above 96 MiB, more than 6,144 accepted chunks, 16,384 header candidates, or 256 MiB of cumulative inflate input, and rejects oversized or truncated chunks instead of accepting partial data

## Usage

### Rust (native binary)

```bash
# Single file
wic_replay_parser demo01.wicdemo

# Multiple files / glob
wic_replay_parser *.wicdemo

# Directory
wic_replay_parser --dir ~/replays/

# Match result plus versioned timeline JSON
wic_replay_parser --timeline-json demo01.wicdemo

# Match-result JSON only
wic_replay_parser --json demo01.wicdemo

# Compact recursive validation summaries for a large corpus (all detected processors)
wic_replay_parser --corpus-summary-json ~/replays/

# Override the bounded worker count; output order stays deterministic
wic_replay_parser --corpus-summary-json ~/replays/ --jobs 2
```

`--timeline-json` emits one `{ replay, timeline }` object for a single file and a
JSON array for multiple files. The existing human-readable output remains the default.

### Replay resource limits

The Rust parser used by the native viewer, CLI, and WASM build applies the same
per-replay bounds. Native path entry points additionally require a case-insensitive
final `.wicdemo` extension; raw-byte WASM calls have no filename to validate.

| Resource                           |     Limit | Purpose                                                                        |
| ---------------------------------- | --------: | ------------------------------------------------------------------------------ |
| Compressed replay                  |    64 MiB | Bounds file reads and the initial allocation                                   |
| Aggregate decompressed data        |    96 MiB | Bounds retained replay memory                                                  |
| Decompressed output per zlib chunk |    64 KiB | Rejects oversized or truncated chunks rather than retaining partial output     |
| Accepted zlib chunks               |     6,144 | Bounds per-stream overhead from many small streams                             |
| Candidate zlib headers             |    16,384 | Bounds repeated failed decompression attempts                                  |
| Cumulative zlib input work         |   256 MiB | Bounds total inflate work across successful and failed candidates              |
| Indexed parser events              | 1,000,000 | Bounds the retained index of event types used by match and timeline extraction |
| Live attribution source events     |   100,000 | Bounds projectile and Tactical Aid attribution state                           |
| Unit-drop attribution comparisons  | 1,000,000 | Bounds the exact ownership join                                                |
| Playback units                     |    65,536 | Bounds retained unit histories                                                 |
| Playback frames                    | 1,000,000 | Bounds retained position checkpoints                                           |
| Playback objectives                |    16,384 | Bounds retained map objectives                                                 |
| Playback objective changes         | 1,000,000 | Bounds retained ownership history                                              |

Normal WiC chunks expand to roughly 16 KiB, so 6,144 ordinary chunks also equal
96 MiB. The chunk limit remains distinct because a hostile input can contain many
smaller streams. There is no generic event-envelope count or replay-duration limit:
the 96 MiB decompressed bound, 21-byte minimum envelope, and checked forward
progress already cap a replay at 4,793,490 minimal envelopes. Game duration is not
directly proportional to compressed size, decompressed size, or chunk count.

Corpus imports use every logical processor visible to the process by default.
`--jobs` can request fewer workers, while the shared active-work limiter prevents
simultaneous parser, detail, and playback work from exceeding detected capacity.

### Python (standalone)

```bash
python3 wic_replay_parser.py <replay_file.wicdemo>
```

On Windows, drag a `.wicdemo` file onto `parse_replay.bat`, or double-click `parse_all.bat` to parse every replay in the folder.

### Example output

```
Map: do_Hometown | Mode: Domination
Date/Time: - 2025-06-08 - 00:15
Duration: 14.3 min | Players: 11
Recorded by: MoonShot
Domination: USA 100% - USSR 0%

[USA] Total: 4375 - WINNER
   1. [ARM] MoonShot                      1372
   2. [AIR] strikingred                    917
   3. [AIR] pivo^BlitzBakedAF              797

[USSR] Total: 3907 - LOSER
   1. [SUP] [S]DIAMOR                      975
   2. [ARM] House^dEEpdenim                790
   3. [INF] [S]kick149                     777
```

## `ReplayData` interface (TypeScript)

```typescript
interface ReplayData {
  gameInfo: GameInfo; // map, server, date, mode
  rawServerFlags: RawServerFlags;
  serverClassification: ServerClassification;
  players: Player[]; // name, id, score, team, faction, role
  playerScores: number[]; // all final scores sorted descending
  durationSeconds: number | null; // match time elapsed, not recording length
  timing: MatchTiming;
  matchEnding: 'totalDomination' | 'timeout' | 'forfeit' | 'unknown';
  winner: string | null; // "USA", "NATO", or "USSR"
  winnerDominationPct: number | null; // 0.5-1.0
  loserDominationPct: number | null; // 0.0-0.5
  dominationShares: DominationShare[] | null; // Domination and Tug of War only
  dominationAnchor: 'povTeam' | 'winnerInferred' | null;
  incomplete: boolean; // true if match results are incomplete
  recorder: string | null; // player who recorded the replay
}

interface MatchTiming {
  capturedMatchStart: boolean; // false => recorder joined mid-match
  recordingSeconds: number; // real length of the replay file
  observedGameplaySeconds: number | null; // gameplay between countdown samples
  matchElapsedSeconds: number | null; // match time when the recording ended
  roundLengthSeconds: number | null;
  roundLengthExact: boolean; // false => inferred, not observed
  joinedAtRemainingSeconds: number | null; // set only for a late join
  finalRemainingSeconds: number | null; // may be negative (post-match overtime)
}

interface DominationShare {
  faction: string; // "USA", "NATO", or "USSR"
  pct: number; // final bar share, the two entries sum to 1.0
}

interface RawServerFlags {
  fewPlayerModeFlag: boolean | null;
  matchModeFlag: boolean | null;
  tournamentMatchFlag: boolean | null;
  clanMatchFlag: boolean | null;
}

interface ServerClassification {
  fewPlayerMode: boolean | null;
  matchMode: boolean | null;
  hasBots: boolean | null;
  clanMatch: boolean | null;
  tournamentMatch: boolean | null;
  ranked: boolean | null;
}

interface GameInfo {
  mapName: string; // "maps/ustown1/ustown1.ice"
  mapDisplayName: string; // "do_Hometown"
  replayName: string | null; // ReplayName metadata stored inside the .wicdemo
  serverName: string;
  dateTime: string;
  gameMode: string; // "Domination", "Assault", "Tug of War"
}

interface Player {
  id: number;
  name: string;
  team: number | null; // 0=Spectator, 1=USA, 2=NATO, 3=USSR
  faction: string | null; // "Spectator", "USA", "NATO", "USSR"
  score: number | null;
  role: string | null; // "infantry", "armor", "air", "support"
  scoreInfantry: number | null;
  scoreSupport: number | null;
  scoreArmor: number | null;
  scoreAir: number | null;
  scoreCapturing: number | null;
  scoreFortification: number | null;
  scoreTransportation: number | null;
  scoreRepair: number | null;
  scoreBridgeLaying: number | null;
  scoreUnitDamage: number | null;
  scoreTacticalAid: number | null;
  scoreTotal: number | null;
}
```

## Timeline interface

Timeline output is opt-in and versioned independently from `ReplayData`:

```typescript
interface ReplayWithTimeline {
  replay: ReplayData;
  timeline: TimelineData;
}

interface TimelineData {
  schemaVersion: 18;
  durationSeconds: number; // length of the recording; the timestamp axis
  matchDurationSeconds: number; // countdown-derived gameplay span
  initialClockSeconds: number | null;
  finalClockSeconds: number | null;
  phases: TimelinePhase[];
  dominationSamples: { timeSeconds: number; value: number }[];
  participants: TimelineParticipant[];
  participantSessions: TimelineParticipantSession[];
  events: TimelineEvent[]; // complete unit/squad deaths only
  infantrySoldierDeaths: InfantrySoldierDeath[];
  preMatchChat: PreMatchChatMessage[];
  postMatchChat: PostMatchChatMessage[];
  coverage: {
    chat: 'visibleToRecorder';
    tacticalAid: 'recorderOnly';
    tacticalAidMarkers: 'visibleFactionWithPlayer';
    tacticalAidDeployments: 'bothFactionsWithValidatedUnitDropPlayers';
  };
  recorderTacticalAidUsage: RecorderTacticalAidUsage;
}

interface TimelineParticipant {
  playerId: number;
  playerName: string | null;
}

interface TimelineParticipantSession {
  playerId: number;
  sessionIndex: number;
  playerName: string | null;
  startSeconds: number; // inclusive
  endSeconds: number | null; // exclusive; null while active at replay end
  identitySource: 'staticMetadata' | 'playerEnteredGame';
}

interface TimelinePhase {
  index: number;
  startSeconds: number;
  endSeconds: number;
  initialClockSeconds: number;
  finalClockSeconds: number;
}

interface InfantrySoldierDeath {
  timeSeconds: number;
  unitId: number;
  unitTypeId: number;
  playerId: number | null;
  team: number | null;
  killerUnitId: number | null;
  killerPlayerId: number | null;
  killerTeam: number | null;
  cause: 'unknown' | 'unit' | 'tacticalAid';
  destructionContext: UnitDestructionContext | null;
  tacticalAidSupportId: number | null;
  tacticalAidSupportName: string | null;
}

type UnitDestructionContext =
  | { type: 'buildingCollapse'; buildingId: number }
  | { type: 'destroyedWithContainer'; containerUnitId: number };
```

`TimelineEvent` is a tagged union using the `type` field. Current output emits:

- player entered/left, team changes, exact spectator-view changes, and role changes
- command-point ownership changes
- complete unit and squad-parent destruction with victim ownership/type and killer attribution when known,
  including the schema-v15 unit-projectile and schema-v17 tactical-aid recovery below
- tactical-aid transfers, recorder-only purchases, visible-faction player-bearing
  markers, both-faction deployments with conservative unit-drop ownership, and votes
- all/team chat visible to the replay recorder
- every structurally valid `TeamWins` event

Unit creation is parsed only as attribution state and is not emitted. Movement,
health updates, and every raw frame are intentionally excluded.

Schema v18 separates deterministic mechanical context from attacker attribution.
`buildingCollapse` requires the exact active unit generation to still occupy the
serialized building slot, that building to enter state 3 at the identical raw
event tick, and the unit death to carry the exact synthetic terminal direction
generated by the building/container fatal path in `wic_ds.exe`. A
`destroyedWithContainer` context additionally requires an exact active type-2
container relation, valid relation-teardown ordering, and container and child
destructions at the same raw tick with the same killer ID. Any lifecycle reuse,
time, ordering, direction, killer, overlap, or cause conflict abstains. These
contexts explain how the unit was destroyed; they do not by themselves identify
who damaged the building or container.

The server copies both killer ID and damage-source context from a destroyed
container into its occupants' fatal calls. Therefore, when schema v17 independently
proves an exact-target tactical-aid cause for the container, schema v18 propagates
that exact support/team to the child. It does not propagate proximity candidates or
historical ownership.

Schema v17 classifies every destruction as `unit`, `tacticalAid`, or `unknown`.
Tactical-aid classification applies only to killer sentinel `512` when a preceding
`ProjectileHomingSupportCreate_Unit` names the exact target unit and uninterrupted
lifecycle within 3 seconds, the support ID is a shipped top-level tactical aid, and
all matching `SupportThingSpawnedDelayed` records from the preceding 30 seconds
agree on one issuing team. All qualifying projectiles must agree on one support and
team. The parser emits that team and support identity but leaves `killerPlayerId`
null because the projectile and deployment carry no exact player-slot bridge.
Heavy Air Support serializes one of five child projectile definitions per faction;
the shipped support database explicitly lists those children under their top-level
parent. Their serialized child ID is preserved while `tacticalAidSupportName`
reports the proven Heavy Air Support parent.

Schema v16 separates the replay's individual infantry actors from complete units.
The game creates and destroys riflemen, machine gunners, medics, anti-tank and
anti-air soldiers, snipers, engineers, and airborne riflemen independently from
their squad-parent object. Their exact destruction facts are retained in
`infantrySoldierDeaths`; they are not emitted as `unitDestroyed` timeline events.
The separate `*_Squad_*` destruction remains the complete squad-loss event. The
classification is an explicit catalogue of the 27 shipped US, NATO, and USSR
multiplayer soldier definitions from `units/unittypes_wic.ice/.loc`; unknown unit
types fail open as complete-unit events rather than being hidden.

Schema v15 can recover a removed killer's player/team only when a preceding
`ProjectileHomingUnitCreate` independently serializes the same firing unit as
`UnitDestroy.aKiller` and the destroyed unit as its exact target. The projectile
must precede destruction by at most 0.5 seconds, both units must be active when it
is created, the target lifecycle must remain uninterrupted, and all qualifying
projectiles must agree on one actor. Historical ownership by itself is never used;
`aKiller=512` remains the invalid/no-unit sentinel and is never recovered.

Version 11 adds `participantSessions` for replay slots whose occupant can change.
The existing `participants` array remains a static compatibility directory; it is
not authoritative after a slot lifecycle transition. A structurally valid
`PlayerEntersGame` opens a session with its bounded UTF-16 player name, and
`PlayerLeavesGame` closes that session. Consumers should resolve an event's
`playerId` against the half-open session containing the event's `timeSeconds`.
When a slot has sessions but none covers an event time, its identity is unknown;
consumers must not fill that gap from `participants`. Replays without session data
retain the version-6 static fallback behavior.

Version 9 adds optional `playerId` and `playerAttribution` fields to
`tacticalAidDeployed`. The value is emitted only for the nine faction airborne-
infantry, airdropped-transport, and airdropped-light-tank definitions when a
binary-proven `aSpawnSource=1` `UnitCreate` of the exact shipped unit type has one
candidate owner inside the corpus-validated arrival and position bounds. The basis
is `unitSpawnOwnership`; ambiguous and unmatched deployments remain `null`.

Version 8 added `tacticalAidDeployed` from the top-level
`SupportThingSpawnedDelayed` effect stream. These records expose TA type and faction
for both sides in ordinary and spectator recordings but serialize no player ID.
It also exposes `spectatorViewChanged` with the raw team/LOS values and decoded
one-team/all-team mode, including the last pre-game state at time zero.

Version 7 added raw `tacticalAidMarker` events. Their `playerId` is the serialized
issuing-player slot carried in the field nominally named `aTeam`. Corpus validation
now narrows their coverage to one faction per replay; they resolve exact players for
that faction but are not match-global. A marker is an effect record, not proof of a
selected single/double/triple bundle.
Version 6 adds one sorted `participants` directory covering every player ID
referenced by an event, a chat boundary, or the recorder-scoped tactical-aid
summary. All event IDs resolve through that directory without duplicating names
across event shapes. Version 5 adds the roster-resolved `playerName` to every chat
shape while retaining the serialized `playerId`; those fields remain for backward
compatibility and mirror the participant entry. Version 4 adds chat, explicit
point-of-view coverage, a deterministic recorder TA summary, and separate
post-match chat. Version 3 keeps the `tacticalAidUsed` event added in version 2 but
removes its speculative bundle reconstruction. `ReplayData` is unchanged.

```typescript
interface ChatMessage {
  type: 'chatMessage';
  timeSeconds: number;
  playerId: number;
  playerName: string | null;
  message: string;
  channel: 'all' | 'team';
}

interface PreMatchChatMessage {
  timeSeconds: number;
  secondsBeforeMatch: number;
  playerId: number;
  playerName: string | null;
  message: string;
  channel: 'all' | 'team';
}

interface PostMatchChatMessage {
  timeSeconds: number;
  secondsAfterMatch: number;
  playerId: number;
  playerName: string | null;
  message: string;
  channel: 'all' | 'team';
}

interface RecorderTacticalAidUsage {
  playerId: number | null;
  totalPlacements: number;
  supports: {
    supportId: number;
    supportName: string | null;
    placementCount: number;
    observedCosts: number[];
  }[];
}
```

`PlayerReceiveChat` stores `aPlayer`, a bounded UTF-16LE `aMessage`, and
`aTeamChat`. These records are what the recorder's client received: global chat is
match-wide and team chat is limited to the recorder's team. Messages before the
first gameplay clock sample are returned in `preMatchChat`; messages after the last
valid `TeamWins` envelope are returned in `postMatchChat`. Neither extends nor
collapses onto the gameplay timeline. Both carry two readings of the same moment:
`timeSeconds` places the message on the recording axis alongside every other
timeline timestamp, and `secondsBeforeMatch`/`secondsAfterMatch` give its distance
from the match boundary.

Static participant names are resolved through the canonical roster logic, including
confirmed bilateral lobby swaps and the concatenated-stream fallback for older
replays with incomplete or chunk-split metadata. Chat-only and zero-score slots may
have a name even when absent from `ReplayData.players`. A participant name is
`null` when the slot has no proven identity; player IDs remain the serialized source
facts. Schema-v11 chat-level `playerName` is resolved at the exact chat envelope
offset through `participantSessions`, while older output mirrors the static
participant entry.

The distinct `PlayerReceiveChatPrivate` shape is intentionally excluded. The only
two corpus records are command acknowledgements from allied `Computer:` players
(`I will attack and hold area.` and `Ok - hang in there.`), not evidence of human
private messages.

```typescript
interface TacticalAidUsed {
  type: 'tacticalAidUsed';
  timeSeconds: number;
  supportId: number; // Adler-32 hash of the support definition name
  supportName: string | null; // proven top-level faction-aid definition name
  honorsCost: number; // marginal cost of this placement
  position: [number, number, number];
  playerId: number | null; // the recording player, when the slot is known
}

interface TacticalAidMarker {
  type: 'tacticalAidMarker';
  timeSeconds: number;
  eventId: number;
  supportId: number;
  supportName: string | null;
  position: [number, number, number];
  playerId: number; // serialized issuing-player slot, 0-15
  upgradeLevel: number;
  direction: [number, number, number];
  durationSeconds: number;
}
```

`supportName` is populated only for proven top-level faction-aid definitions
recovered from shipped game data. Internal child effects and unit special-ability
markers retain their raw `supportId` with a `null` name, allowing presentation
layers to distinguish player-selected tactical aids without guessing from timing,
cost, or marker shape.

A tactical aid is charged **per placement**, so a triple nuke that is fully present
in the recording appears as three raw events costing 80, 60 and 40 rather than one
event costing its 180 bundle price:

```text
t= 462.6  TacticalNuke_USSR  cost 80
t= 504.0  TacticalNuke_USSR  cost 60
t= 548.1  TacticalNuke_USSR  cost 40
```

Each event means one observed placement, not one selected single/double/triple call.
The replay does not serialize the selected bundle size, equal-price strikes are
indistinguishable from separate calls, queued strikes may be held indefinitely, and
a recording may begin after the first placement. Bundle grouping is therefore left
to an optional downstream analysis rather than presented as parsed fact.

Three limits are inherent to the format, and consumers
should surface them rather than paper over them:

- **Only the recorder's purchase ledger exists.** `SupportThingUsed` and its paired
  honors deduction are written for the recording player alone. Recorder-visible-
  faction `SupportThingMarker` records separately show effects and their issuing
  players, while the both-faction deployment stream has players only for validated
  unit drops. Neither is a complete purchase ledger or bundled-call record.
- **`honorsCost` does not identify the aid.** The same `supportId` is charged
  different amounts within a single match, and unrelated aids share costs. Never
  map a cost to a name.
- **Bundle selection is not stored.** Count events as observed placements only. Do
  not derive single/double/triple counts from timing, and do not assume the first
  observed discounted placement is the start of a call.

The WASM exports are `parse_replay_timeline_wasm()` for timeline-only output and
`parse_replay_with_timeline_wasm()` for the combined object. The existing
`parse_replay_wasm()` response is unchanged.

The native Rust ground-truth harness passes all 17 saved fixtures. An earlier
schema-18 audit, before the later recorder and roster corrections, parsed 2,865
replays (15 established rejects) and extracted 35,801 pre-match, 23,597 in-match,
and 5,488 post-match messages: 37,384 all-chat and 27,502 team-chat. All 64,886 messages have resolved player names, with
zero conflicts against `ReplayData.players`; 434 messages belong to chat-only or
zero-score slots absent from that match-results roster. The run also found no invalid
chat player IDs, no in-match chat beyond `durationSeconds`, and no TA-summary
mismatch. It emits 198,591 tactical-aid markers from the accepted replays, all with
valid player slots, resolved participant names, and timestamps within the observed
timeline. Of 223,426 both-faction top-level deployments, 91,932 carry the validated
`unitSpawnOwnership` player; none has an invalid player, participant reference,
faction, or timestamp. The two known bot-response records are excluded.
It emits 713,051 complete-unit destructions: 483,184 unit-caused, 16,395
tactical-aid-caused, and 213,472 still unknown. There are 14,723 exact
`buildingCollapse` contexts and 2,779 exact `destroyedWithContainer` contexts, all
on player-owned complete units. Of the container contexts, 67 inherit the exact
tactical-aid support/team proven on their container; the remaining mechanical
contexts retain their independently parsed `unit` or `unknown` cause. Tactical-aid
player identity is never inferred.

Those figures describe the earlier audit snapshot. See [Test results](#test-results)
for the later aggregate results and the
[score-only result recovery](../docs/score-only-results-2026-09-07.md)
for the latest 4,089-file viewer-result comparison.

## How it works

### File format

`.wicdemo` files use **BinTagFormat2** serialization:

- 19-byte raw header
- zlib-compressed 16KB chunks
- First chunk contains metadata (map, server, player names, recorder slot)
- Remaining chunks contain a BinTag event stream (gameplay events, scores, team changes)
- No structured results table — the in-game scoreboard is rendered client-side from the event stream data

### Player name extraction

Players are extracted structurally using the `aSlot` BinTag hash (`04 02 cc 05`) in the metadata chunk. Each aSlot entry has a fixed layout: slot ID (u32) at offset +13, and the player's UTF-16LE name (null-terminated) at offset +47. Older replays (pre-2011) may pad names with U+00A0 (non-breaking space) before the null terminator.

For replays with incomplete metadata (common in 2007-2010 era), a fallback searches
the concatenated decompressed stream for `aSlot` entries matching required player
IDs. The overlap also recovers a valid name record split across the first zlib chunk
boundary.

### Server name extraction

The server/session title is decoded directly from the variable-length UTF-16
`myGameName` BinTag field (`0x158d03e2`) in the metadata chunk. It is not guessed
from words such as `Server`, `Domination`, or `Assault`; server names without those
words and names containing WiC color markup remain exact serialized values.

### Server classification

Server classification is orthogonal rather than a single enum. FPM and bots can
coexist, and FPM also sets the raw Match Mode mechanism flag. The public
`matchMode` property is therefore true only when the Match flag is set and FPM is
clear. `hasBots` uses structurally framed participant `myType=1` records rather
than player names.

All properties are nullable so missing evidence is not reported as false. Ranked
is always `null`: the game has a confirmed Ranked bit in its server-browser
protocol, but the demo header does not serialize it and neither does any message
the client can write, so no future parser version will fill it in. The parser
never guesses Ranked or Unranked from server names, filenames, player counts, or
the absence of incompatible modes. A display layer may treat a classification with
all five other modes positively `false` as ranked — the viewer does — but that
inference is one-directional, since a ranked server may also run Match Mode, clan,
or tournament games.

### Score extraction

This section describes the historical parser/CLI result. Overview and library
summaries now prefer the [replay end-screen projection](#replay-end-screen-projection),
whose score comes from `SetScoreAtGameEnd.aTotalScore`.

Final scores use complete `SetScore` envelopes in the validated primary recording
chain. Each message supplies its own unsigned `aPos` slot and signed
`aPlayerScore`; the last observation before `TeamWins` wins, including zero resets
and negative scores. This captures the final slot-15 update and supports score
messages that do not cycle through all sixteen slots. No score is assigned using
a field from the next message.

If the primary chain never reaches `TeamWins`, or has no usable score messages,
the established counter-based summary scan remains the fallback. Such recordings
can have a readable final summary beyond an incomplete chain; a partial live score
must not replace it. Post-result scores are excluded from the primary path.

### Team detection

Zero-score result rows can receive a narrowly scoped spectator correction when
validated session events establish all-team or one-team spectator status and the
slot has no gameplay role selection, unit creation, or nonzero final statistics.
A sole explicitly named, inactive gameplay session can supersede old lobby roles
with a later spectator event, including during gameplay; observed nonzero stream
scores block this additional exception. This is not a general zero-score filter.
See the [exact gates](SCHEMA.md#result-roster-spectator-correction) and
[replay evidence](../docs/zero-score-spectators-2026-09-07.md). Unknown remains
unknown; the suspected connection to players who have not selected a team is
unverified.

Teams are extracted by finding `PlayerJoinedTeam` events in the BinTag event stream. Each event contains the player's slot ID and faction. `SpectatorJoinedTeam` events are also scanned but always treated as team=0 — their `aTeam` field identifies the observed/previous faction rather than playing membership. For players missing from those events, `UnitCreate` events (which record the owning player and their team) are used as a fallback.

Faction IDs:

| Value | Faction   | Appears on                                           |
| ----- | --------- | ---------------------------------------------------- |
| 0     | Spectator | -                                                    |
| 1     | USA       | American maps (Hometown, Seaside, Farmland, ...)     |
| 2     | NATO      | European maps (Canal, Mauer, Vineyard, Riviera, ...) |
| 3     | USSR      | All maps                                             |

### Lobby slot swap detection

When players swap team slots in the pre-game lobby, the metadata chunk retains the
pre-swap name-to-slot mapping while the event stream records the corrected names.
The parser scans the first 200KB of event data for `aSlot` entries with names that
differ from metadata, then applies only confirmed **bilateral swaps** — slot A's
event-stream name must match slot B's metadata name and vice versa.

Some older replays instead record one player leaving and another entering the same
numeric slot before the match. For scored result rows, a structurally decoded
`PlayerEntersGame` occupant active at the first valid gameplay clock sample supersedes
the lobby metadata name. The score and end-summary role therefore stay attached to
the player who occupied that slot when gameplay began. Later replacements do not
relabel a slot occupied at gameplay start. A separate, constrained correction handles
a scored slot that was vacant at the first gameplay clock: exactly one named entrant
must appear before the first recorded `TeamWins`, that entrant's recorded team joins
must match the existing playing team, and the end summary must have nonzero per-role
scores. Otherwise the established name resolution remains in place. Name-bearing
entry records also discard the U+00A0 padding used by some older replays.

### Role extraction

Player roles (Infantry, Armor, Air, Support) are extracted from end-of-file score
summary blocks using BinTag hashes `aScoreRole0-3`. The primary role is the one with
the highest nonzero signed per-role score, including for players who switched roles. All four
per-role scores are stored. If all four are zero but the player has a
positive score, a recognized `aRoleId` immediately following `aPos` in the same
summary block supplies the recorded role. This fallback does not alter the role
scores or infer a primary role for a role-switcher. Unknown and Few Player Mode
role IDs remain unavailable.

Timeline `playerSetRole` events resolve the role identifier separately, and add a
fifth value: `fewPlayer` (`FPM_ROLE`, `0x0b230275`). Few Player Mode has no roles, so
it assigns this sentinel instead — its presence identifies an FPM match exactly, and
no ordinary match emits it.

### Recorder detection

The POV hash (`ad 07 93 4b`, u32 LE: `0xad07934b`) in the metadata chunk stores the slot ID of the player who recorded the replay. The parser reads this value and resolves it against the player roster. 100% detection rate on valid replays.

### Domination bar

The `aFactor` BinTag field (hash `0x0a6f02c1`, type=float) tracks the domination tug-of-war bar position:

- Starts at **0.5** (neutral center) at game start
- Moves toward **0.0** or **1.0** as one team dominates command points
- Updates ~once per second (800-1100+ events per game)
- The **last value** before `TeamWins` is the final domination state
- Total domination wins reach exactly 0.0 or 1.0; timed wins stay between

The winner always has the larger share: `max(aFactor, 1 - aFactor)`.

### Timeline clock and phases

Every timestamp is **recording-elapsed** seconds, read from the `Event` envelope
each record sits in (see below). Zero is the first envelope in the file, so the
axis covers the pre-match lobby, the match, and the post-`TeamWins` tail.
`TimelineData.durationSeconds` is the length of the recording on that axis and
bounds every event, phase, and sample.

The game-mode countdown is not used as a time axis, because it is not one. A
156-replay scan of the corpus found:

- it starts late — the recording runs for a median of 66.8 s of lobby before the
  first countdown sample, and up to 22 minutes;
- it restarts between Assault rounds, so it is not monotonic within a match; and
- it does not tick at real time — per-sample rates from 0.84 to 1.25 s of clock
  per second of recording are normal, and one corpus replay runs at ~2.4x.

The envelope clock has none of those problems: it starts at zero and advances
monotonically in all 156 replays.

The countdown still supplies what it is sound for: clock readings and phase
boundaries. A positive reset larger than five seconds starts a new phase, which
handles Assault rounds without hard-coding a game mode; small upward jitter is
ignored. Phase bounds are reported on the recording axis, taken from the
envelopes of the samples that opened and closed each countdown run.

`matchDurationSeconds` is the accumulated countdown span — how long the match
ran, not how long the replay is. For a replay recorded after a phase began, it
reports only the captured portion. The initial and final clock values are exposed
so consumers can show that limited coverage without guessing how much of the
match is absent. Negative clock values are retained for overtime. Domination
values are sampled every five seconds, with the final value always retained.

### End-of-match summary

The per-player summary blocks also expose the game's final score categories:
capturing, fortification, transportation, repair, bridge laying, unit damage,
Tactical Aid, and total score. These are serialized score values, not reconstructed
counts of captures, damage hit points, deployments, or honors spent. All are `null`
only when a player has no usable end-summary block.

The replay does not serialize a chosen category leader or a tie-break field. The
original client derives that presentation by sorting its fixed 16 player slots
independently for each score field. The parser therefore exposes the serialized
per-player values without inventing or persisting a leader decision; consumers that
need original-screen parity must reproduce the client ordering as presentation.

A replay does not end at its last envelope. After `TeamWins` the recorder writes a
second complete pass over the match whose envelope timestamps restart at zero and
climb back to the recording length. Every corpus replay contains exactly one such
reset. The parser cuts the chain there: the summary pass sets no timestamps and
contributes no timeline events, chat, or domination samples.

### Event envelopes and tactical aid

Past a short metadata prefix the decompressed stream is a gapless chain of `Event`
envelopes, each carrying its own size, a float gameplay timestamp, and a message
name hash:

```text
[Event 0x05b50203][0x15000000][0x06][totalBytes u32][time f32][messageHash u32][fields...]
```

`totalBytes` counts the header, so the next envelope begins at `offset + totalBytes`.
Tactical-aid extraction uses this framing rather than bare hash scanning, because
the message alone is ambiguous.

`SupportThingUsed` serializes exactly `anId` plus `aPosition` (three floats written
as `aPosition.x/.y/.z`); the sender is at `wic.exe` VA `0x00b830c0`. It carries no
player and no cost. `ChangeHonors` serializes a single `aDelta` float (sender at VA
`0x00b82080`) and is the **recording player's own** honors ledger — a tactical-aid
gift to the recorder produces a matching `+aNumTa` deduction record, while gifts
between two other players produce none.

Two `SupportThingUsed` shapes appear:

- a burst with `aPosition` exactly `(0, 0, 0)` right after `StartGameTime`, which
  enumerates the supports the match makes available and is not an activation; and
- real activations with a world position, each immediately preceded by a
  `ChangeHonors` envelope carrying the negative cost.

The parser emits only the second shape, and only when the negative deduction is
present, so the cost and the activation are always read as one fact.

`SupportThingMarker` separately serializes an event ID, support ID, position, the
issuing player slot (in the field nominally named `aTeam`), upgrade level,
direction, and duration. Binary call-path tracing in both `wic.exe` and
`wic_ds.exe` shows that the server passes the connection's player slot into this
field. The parser emits these recorder-visible-faction records directly as
`tacticalAidMarker`; it does not infer multi-strike bundles.

`SupportThingSpawnedDelayed` separately serializes top-level support ID, position,
faction, upgrade, direction, and age. Corpus validation shows this effect stream
covers both factions even for ordinary players and one-team spectators. Schema v8
emits it as `tacticalAidDeployed`. The message itself contains no player slot.
Schema v9 can separately attach an exact unit-drop owner when the corresponding
`UnitCreate` carries the binary-proven TA spawn source, exact shipped unit type and
faction, a bounded arrival/position match, and only one candidate player. All other
enemy player identities remain unknown unless a player-bearing marker or another
recording supplies them.

`SendTATaunt` is a separate server notification containing the acting player slot,
affected player slot, support-manager index, and upgrade level. The dedicated
server emits it when cumulative tactical-aid damage score against that player
reaches the support definition's taunt threshold. Schema v10 resolves the index
through the ordered zero-position catalogue when available and emits
`tacticalAidDamageThreshold`; an unavailable catalogue leaves the support ID/name
null while preserving the exact players and raw index. This is damage evidence,
not proof that a particular simultaneous `UnitDestroy` was caused by the aid.

### Corrupt file detection

The parser checks for the presence of a `TeamWins` event (hash `0x0db80329`) in the decompressed data. Valid replays contain at least one; a small number contain two complete end-summary segments. Known corrupted files have zero. Returns error: "Corrupt replay file: no TeamWins event found".

### Incomplete match detection

After parsing, the parser checks for three conditions that indicate incomplete match results:

1. **No valid winner** — TeamWins event has aTeam=0 (map-vote interrupted games)
2. **Unknown scored players** — Players with non-zero scores but no team assignment
3. **Missing opposing faction** — Winner exists but no opposing faction visible among players

When any condition is true, `incomplete` is set to `true`. CLI output shows
"Incomplete match results" and suppresses the domination line. The original
2,622-replay result-validation set flagged 35 replays (1.3%); this historical figure
is retained for the classification study rather than presented as a current-corpus
total.

### BinTag format

Each field entry is 17 bytes:

```
[hash: 4 bytes] [0x11000000: 4 bytes] [flag: 1 byte] [0x11000000: 4 bytes] [value: 4 bytes]
```

Field names are hashed using a modified Adler-32 algorithm (mod 65521), found at VMA `0xa03830` in `wic.exe`.

## Test results

Snapshot verified 2026-09-07 from the tracked path-free aggregate baseline. The
baseline digest covers every normalized summary while excluding replay paths.

The current deduplicated linked corpus contains 2,880 replay files:

- **2,865 parsed successfully**, 15 established corrupt/empty fixtures rejected
- **2,865/2,865** emit schema version 18
- **450,126** individual infantry-member deaths are separated from **710,951**
  complete unit/squad destruction events across 2,806 replays, with zero soldier
  death timestamps beyond the recording duration
- **14,693** complete units have an exact occupied-building-collapse context and
  **2,771** have an exact destroyed-with-container context; 67 container children
  inherit an independently exact tactical-aid support/team, with no context overlap
- **2,865/2,865** declare recorder-point-of-view chat and tactical-aid purchase
  coverage plus visible-faction player-marker and
  `bothFactionsWithValidatedUnitDropPlayers` deployment coverage
- **198,495/198,495** emitted tactical-aid markers have valid player slots, resolved
  participant names, and timestamps within the observed timeline
- **223,319/223,319** top-level tactical-aid deployments have valid faction IDs and
  timestamps within the observed timeline
- **91,893/91,893** ownership-attributed deployments have valid player slots,
  resolved participant names, and `unitSpawnOwnership` provenance
- **5,780/5,780** emitted spectator-view changes decode to known LOS modes
- **27,521/27,521** timeline participant entries have resolved names, with 0 duplicate
  IDs or missing player references. Compared with the pre-fix parser, static roster conflict counts
  change in 135 replays for timeline participants and 28 for chat senders; these
  compare a slot directory or historical sender with the corrected result name
- **64,886/64,886** chat messages have resolved sender names
- **2,865/2,865** accepted replays have a structurally decoded server name; the
  corpus contains 664 distinct serialized titles and no `Unknown` fallback
- Mode coverage: 2810 Domination, 43 Assault, and 12 Tug of War replays
- Ground truth regression test: 17/17 passing
- Original lobby-swap ground-truth expectations remain unchanged
- Separate 4,089-file library comparisons: the name correction changes 222 names
  in 173 files (117 distinct contents); the role fallback fills 169 blank roles in
  134 files (66 distinct contents). Each isolated comparison preserves every other
  parsed summary field
- The original demo263 spectator correction changed only team/faction for 192
  zero-score rows across 170 of 4,089 files (92 distinct contents). All names,
  scores, roles, row counts, and non-player summary fields remain unchanged;
  4,074 files parsed and the same 15 failed. That earlier Flatpak scan retained
  37,193 player rows; see the
  [complete evidence and Python parity limitation](../docs/zero-score-spectators-2026-09-07.md)
- The later lobby refinement adds 46 spectator corrections across 40 paths and
  suppresses 18 proven abandoned duplicates across 18 paths in viewer rosters.
  Parser row counts and all scores, roles, names, and non-player summary fields
  stay unchanged. The refreshed Flatpak library has 37,175 visible roster rows;
  all 4,074 accepted summaries match the audit, and the same 15 files reject.
  See the [refinement evidence](../docs/lobby-roster-refinements-2026-09-07.md).
- The opt-in event-backed viewer recovery retains scores observed before departure.
  Its zero-score and last-selected-role fallbacks were withdrawn; roles continue
  using end-summary scores. The earlier 4,089-path audit recovered
  514 departure scores without changing raw results or existing positive scores.
  See the [score-only recovery](../docs/score-only-results-2026-09-07.md)
  and [historical audit](../docs/event-backed-results-2026-09-07.md).
- Signed-score decoding fixes wrapped negative totals and role/category scores.
  The final 4,089-path comparison changes 21 paths (12 distinct replays), including
  seven distinct primary-role corrections where penalties previously outranked
  positive role scores. Identities, teams, hidden duplicates, and all 514 recovered
  departure scores are unchanged. Cache `detail-v42` refreshes the installed
  library. See the [signed-score validation](../docs/signed-scores-2026-09-07.md).
- Ground-truth corrections: demo100 slot 2 expects its explicitly recorded Armor
  role; AIR VS ESL slots 6 and 10 expect Spectator after raw team/view event
  verification. All other fixture expectations remain unchanged
- Max 16 players per replay enforced by slot 0-15 cap

### Rust verification

Run the parser's unit tests and strict lint checks from the `parser/` directory:

```bash
cargo test --locked --all-features --manifest-path rust_parser/Cargo.toml
cargo clippy --locked --all-targets --all-features \
  --manifest-path rust_parser/Cargo.toml -- -D warnings
```

`test_ground_truth.py` additionally requires the private replay corpus at the paths
recorded in `ground_truth.json`; it reports unavailable files as `SKIP` and fails if
zero fixtures execute. It invokes the compiled native Rust parser, not the Python
reference parser. Build the release CLI first. Set `WIC_REPLAY_ROOT` to the
corresponding evidence root without changing `HOME`:

```bash
cargo build --release --locked --all-features \
  --manifest-path rust_parser/Cargo.toml
WIC_REPLAY_ROOT=/path/to/evidence-root python test_ground_truth.py
```

Override the binary path with `WIC_RUST_PARSER` when necessary.
To test a different replay set, create your own JSON expectation file in the same
format and set `WIC_GROUND_TRUTH_FILE` and `WIC_REPLAY_ROOT` to your private files.
The tracked expectations document the original corpus and cannot validate
unrelated replays.

For one release-oriented command that runs the full quality gate, builds the release
CLI and WASM package, and requires all recorded private ground-truth fixtures:

```bash
./scripts/verify-release.sh --corpus-root /path/to/evidence-root
```

Without `--corpus-root` or `WIC_REPLAY_ROOT`, the script completes the public checks
but explicitly reports that ground truth was not run.

### Performance measurements

`scripts/benchmark-parser.sh` builds the optimized native CLI, hashes every measured
JSON result, and reports wall time, CPU time, and peak RSS through GNU `time`. Pass
representative replays explicitly so private evidence paths never enter the
repository; repeat `--jobs` to compare corpus concurrency without changing output
ordering:

```bash
./scripts/benchmark-parser.sh \
  --replay /path/to/median.wicdemo \
  --replay /path/to/largest.wicdemo \
  --repeat 10

./scripts/benchmark-parser.sh \
  --corpus-root /path/to/evidence-root \
  --jobs 1 --jobs 4 --jobs 8
```

The reported SHA-256 is over serialized parser output, not the replay input. Compare
it before and after optimization to detect behavior or ordering changes alongside
the semantic corpus and ground-truth gates.

### Local quality gates

Install the combined repository's tracked native Git hooks once per clone from the
repository root:

```bash
python3 -m pip install -r parser/requirements-dev.txt
./scripts/install-hooks.sh
```

The pre-commit hook runs the combined fast gate; pre-push runs the full parser and
viewer gate. Run the same checks directly with `./scripts/quality.sh fast` or
`./scripts/quality.sh full`. Hooks can be bypassed with Git's `--no-verify`, so they
are a local safeguard rather than server-side enforcement.

### Departed result players

Raw player rows include nullable `leftAtSeconds`, backed by explicit named-session
and leave events before the match result. Statistics are retained. The viewer uses
this in the historical fallback to omit confirmed departed rows from result teams
exceeding eight players; smaller fallback teams remain unchanged. Complete final
screens instead supply their own present-player roster. See the [contract](SCHEMA.md#departed-result-players)
and [demo58 validation](../docs/departed-roster-overflow-2026-09-08.md).

### Replay end-screen projection

The viewer prefers `final_screen_players()` (also available on the Python
reference parser) for complete end-game results. It pairs the final score table
with the present occupants' recorded names and teams, using the same total and
category values shown on the replay end screen. The historical CLI result remains
available, and the viewer retains it as a fallback when the final screen cannot
be decoded completely or the historical roster restores a missing opposing side.
Final-score tables are decoded independently of occupant identity. Skipped compressed
data before the result requires fallback even when the surviving event bytes align;
an unnamed replacement never inherits the departed occupant’s name. Intact score
records therefore do not by themselves qualify a replay for final-screen selection. See [source evidence and validation](../docs/final-screen-results-2026-09-09.md).
