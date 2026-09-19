# Parser Output Contract

The canonical Rust parser exposes two JSON surfaces:

- `ReplayData`, the established match-result summary returned by `--json` and
  `parse_replay_wasm()`; and
- `TimelineData`, the independently versioned lightweight timeline returned by
  `--timeline-json`, `parse_replay_timeline_wasm()`, and
  `parse_replay_with_timeline_wasm()`.

Rust field names are serialized as camelCase. Timeline events are tagged unions
whose discriminator is the camelCase `type` field. Optional values are serialized
as explicit JSON `null`; they are not omitted.

## Result-roster spectator correction

The legacy team resolver remains the source for ordinary results and historical
match/POV inference. A narrow final roster correction changes only `team` and
`faction` to `0` / `Spectator` when all of these conditions hold:

- The existing team is USA, NATO, or USSR; final score and every parsed end-summary
  role/category/total score are present and zero.
- The recording captures the pre-match zero clock and a gameplay clock, followed
  by `TeamWins` in the primary validated envelope chain.
- Exactly one session bearing the result row's name overlaps gameplay before that
  result, and that session was already present at the first gameplay clock.
- Its last team/view event before leaving or the result is explicitly
  `SpectatorJoinedTeam` with `aSpectatorLos=2` and `aTeam` in 0–3 (all teams),
  or `aSpectatorLos=1` and `aTeam` in 1–3 (one team). The observed faction is
  not playing membership.
- No owned `UnitCreate` is recorded for that slot before the result, and no
  `PlayerSetRole` occurs during gameplay, including other occupants of the slot.
- Lobby role selections are allowed only within the same match-start session,
  followed by explicit all-team spectator status before the first gameplay clock.
  A subsequent lobby team join requires another pre-game spectator event; any
  gameplay team join blocks this original lobby-role exception.
- An additional exception allows superseded lobby roles when the entire slot has
  exactly one gameplay session, that session has a matching explicit named entry,
  the result has no role, and no nonzero stream score was recorded. Every role
  selection in that session must precede a recognized spectator event. This can
  occur after gameplay starts; unit creation and gameplay role selection still
  block the correction.

The end-summary role ID alone does not establish participation: demo263 records
role IDs for its zero-stat spectators too. Post-result events and other occupants'
team/view events cannot establish spectator status for the named person. Missing
scores/summaries, reconnect ambiguity, late recordings, unknown teams, and other
unrecognized spectator-view modes retain the existing result. Names, statistics, timeline
history, and historical match/POV calculations are unchanged. The viewer already
lists spectator result rows by name only. Its cache key is advanced for this output
correction; the JSON shape and timeline schema remain unchanged.

Unknown team data remains unknown. It does not establish AFK behavior; see the
[demo263 investigation](../docs/zero-score-spectators-2026-09-07.md).

## Viewer-only abandoned lobby duplicates

The opt-in Rust `abandoned_lobby_duplicate_slots(players)` evidence query returns
slot IDs without changing `ReplayData` or timeline JSON. The viewer suppresses a
result row only when every score is present and zero, its role is absent, and:

- The primary envelope chain captures match start and a later result.
- Explicit named entry and leave records establish that all of the old slot's
  sessions ended before gameplay; every session has the same name, and there is
  no later occupation or recorded role selection or unit creation in that slot.
- Exactly one other scored, role-bearing result row has the same exact name and
  playing team, an explicit pre-game entry, a single matching session spanning
  the match, and owned units created during gameplay.

The viewer applies this to summary counts/names and Overview rows. Cached detail
stores the IDs separately as viewer-owned `hiddenPlayerIds`; the embedded parser
roster and historical timeline remain intact. Existing detail without that field
keeps all rows. Matching names or zero score alone never trigger suppression.
See the [lobby refinement evidence](../docs/lobby-roster-refinements-2026-09-07.md).

## Opt-in event-backed result evidence

`WicReplayParser::player_result_evidence(players)` returns score evidence without
changing `parse()`, `ReplayData`, CLI/WASM JSON, or the Python reference output.
Candidates must have a known playing team, no role, and every parsed final score
field present and zero. The query does not recover or change roles.

Recovery requires a captured match start and later result in the validated primary
envelope chain, exactly one gameplay session bearing the result-row name, and an
explicit named entry in that session. Unnamed gameplay occupants, owned units
outside that session or on a conflicting team, and another occupant's gameplay
role selections block score attribution. At least one owned gameplay unit must
establish participation. No player rows or team assignments are inferred.

An explicit `PlayerLeavesGame` must end the session before the result. Its last
complete, primary-chain `SetScore` observation must be positive, followed by a zero
reset before another entry or the result. Any nonzero score outside that session
during gameplay blocks recovery. Signed values and both field type flags are
validated. This uses the last observation, never a maximum. Missing/negative/zero
last observations, absent resets, implicit departures, reconnect ambiguity, and
post-result events do not justify recovery.

Evidence contains `playerId` and optional `scoreBeforeLeave`, with `score`,
`observedAtSeconds`, and `leftAtSeconds` on the recording clock. It contains no
role observations. The zero-score role and last-selected-role fallbacks were
withdrawn; roles come from the parser's signed end-summary score calculation,
not event-backed role recovery.

### Viewer projection

When final-screen extraction abstains, or the historical fallback restores an
opposing side absent from the final screen, the viewer stores `playerResultEvidence`
alongside `replay`, `timeline`, and `hiddenPlayerIds`, preserving the historical
parser result. Otherwise, a supported final screen supplies the cached result rows
and uses empty recovery/duplicate evidence. Missing
evidence defaults to no recovery. Legacy `role` and `lastRecordedRole` evidence
fields are ignored, so loading an old document cannot restore the withdrawn roles.

The projected `PlayerView.score` uses the recovered score and exposes
`scoreBeforeLeave`, an asterisk, and a hover explanation. No repeated explanation
is shown below the cards. Per-role/category/total summary statistics become null
for recovered departure scores; they cannot be reconstructed from one score
observation. Team totals include displayed scores, while category leaders require
actual category statistics. Library matchup calculation uses the same recovery.

Current cache `detail-v47` refreshes summaries/details for final-screen projection;
`detail-v44` previously introduced the departure-overflow fallback.
Departure alone does not establish spectator status. Product, parser JSON,
timeline, and database versions are unchanged. See the
[score-only recovery](../docs/score-only-results-2026-09-07.md),
[current signed-score behavior](../docs/signed-scores-2026-09-07.md), and the
[historical investigation](../docs/event-backed-results-2026-09-07.md).

## Compatibility policy

`ReplayData` predates explicit schema versioning and is treated as a stable public
contract. Removing or renaming fields, changing nullability, or changing a field's
meaning requires an explicit compatibility plan.

`ReplayData.durationSeconds` is **match** time elapsed, not the length of the
recording. The two differ when the recorder joined mid-match; `ReplayData.timing`
carries both, along with the join point and whether the round length was observed
or inferred.

`ReplayData.timing` separates three distinct spans:

| Field                     | Meaning                                                                                                                 |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `recordingSeconds`        | Real length of the replay file, from the `Event` envelope clock. Includes pre-match lobby and the post-`TeamWins` tail. |
| `observedGameplaySeconds` | Gameplay between the first and last countdown sample. Excludes lobby and post-match time.                               |
| `matchElapsedSeconds`     | Match time elapsed at the end of the recording, corrected for a late join.                                              |

`recordingSeconds` is the right answer to "how long is this replay" and the right
value for a replay-library duration column. It replaces the previous
`timing.recordedSeconds`, which was countdown-derived despite its name and
understated the file by a corpus median of 76 s.

`ReplayData.rawServerFlags` and `ReplayData.serverClassification` are additive
server-mode surfaces. Their booleans are nullable: `false` means the relevant
replay evidence was parsed and clear, while `null` means unavailable or not
recoverable. In particular, `serverClassification.ranked` is
permanently `null`: the confirmed server-browser Ranked bit is not present in the
replay header, and neither the header nor any of the 169 messages the client can
serialize carries an equivalent. A consumer may infer ranked status from a
classification with every other mode positively `false`, but that is an inference
and does not belong in this contract.

`TimelineData.schemaVersion` is currently `18`. Increment it before making any of
the following changes:

- adding, removing, or renaming a timeline field or event variant;
- changing field types, enum spellings, or nullability;
- changing the meaning or coverage of an existing value;
- changing the gameplay timebase; or
- replacing serialized source facts with inferred values.

Fixes that restore the documented meaning of an existing field may remain in the
same schema version, but must have a regression test and changelog entry. Resource
limits and error handling may be tightened without a schema increment when valid
output is unchanged.

The serialization contract tests in `rust_parser/src/parser.rs` cover every
`ReplayData` field and every schema-v18 timeline field/event variant. A change to their
expected JSON is an API decision, not a mechanical test update.

## Facts and derived values

The parser keeps serialized facts distinct from conservative reconstruction:

| Output                                                                | Basis                                                                                                                                                                                                                                 |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| In-game replay name                                                   | Unique structurally framed UTF-16 `ReplayName` metadata field; `null` when missing or ambiguous                                                                                                                                       |
| Player IDs, score values, chat text/channel, event IDs and raw hashes | Serialized replay fields                                                                                                                                                                                                              |
| Server name                                                           | Structural UTF-16 `myGameName` metadata field                                                                                                                                                                                         |
| Raw server flags                                                      | Exact `myFPMModeFlag`, `myMatchModeFlag`, `myIsTournamentMatchFlag`, and `myIsClanMatchFlag` demo-header values                                                                                                                       |
| FPM classification                                                    | Exact FPM header flag                                                                                                                                                                                                                 |
| Match Mode classification                                             | Match header flag set while the FPM flag is clear; the raw Match flag is retained because FPM sets it too                                                                                                                             |
| Bots classification                                                   | Structurally framed participant `myType=1`; false requires a parsed nonempty set containing only `myType=0`                                                                                                                           |
| Ranked classification                                                 | Always `null` pending a positive replay-side signal; never inferred from exclusions or server names                                                                                                                                   |
| Player names                                                          | Structural `aSlot` metadata, with lobby-swap correction, bounded stream fallback, and the constrained late-entrant correction below                                                                                                   |
| Team/faction                                                          | Latest team events, with unit-ownership fallback for match results                                                                                                                                                                    |
| Primary role                                                          | Highest final per-role score; if all four are zero for a scored player, the same summary block's explicit playing-role ID is the fallback                                                                                             |
| Player post-match scores                                              | Serialized capturing, fortification, transportation, repair, bridge-laying, unit-damage, Tactical Aid, and total values from the same final `aPos` summary block                                                                      |
| Recorder name                                                         | Serialized point-of-view slot resolved through the canonical roster                                                                                                                                                                   |
| Timeline participant name                                             | Static canonical-roster compatibility fallback for a serialized referenced player ID                                                                                                                                                  |
| Timeline participant session                                          | Half-open slot occupancy interval opened by a structurally valid `PlayerEntersGame` name and closed by `PlayerLeavesGame`; damaged names remain `null`                                                                                |
| Tactical-aid marker player ID                                         | Serialized issuing-player slot in the marker field nominally named `aTeam`                                                                                                                                                            |
| Spectator view                                                        | Serialized `SpectatorJoinedTeam.aTeam` and `aSpectatorLos`; LOS 1 is one-team and LOS 2 is all-teams                                                                                                                                  |
| Tactical-aid deployment                                               | Top-level `SupportThingSpawnedDelayed` effect with serialized support, faction, position, direction, upgrade, and age                                                                                                                 |
| Tactical-aid damage threshold                                         | Serialized `SendTATaunt` actor, affected player, support-manager index, and upgrade; support ID/name are a replay-catalogue lookup, and no unit kill is implied                                                                       |
| Unit destruction context                                              | Exact active-generation building occupancy or type-2 container relation, same-raw-tick terminal records, executable-derived synthetic direction, and strict ordering/killer agreement; context never supplies an attacker by itself   |
| Unit-drop deployment player                                           | Exact owner of a binary-proven `aSpawnSource=1` `UnitCreate`, emitted only for the nine shipped unit-drop definitions when the type/faction/arrival/position join has one candidate player; attribution basis is `unitSpawnOwnership` |
| Timeline `timeSeconds`                                                | Serialized `Event` envelope timestamp of the record's own envelope                                                                                                                                                                    |
| Timeline phases                                                       | Countdown resets larger than five seconds                                                                                                                                                                                             |
| Domination samples                                                    | Serialized domination values downsampled to five-second intervals, retaining the final value, re-expressed in one frame anchored to the final sample                                                                                  |
| Domination anchor faction                                             | POV team resolved at the final bar sample; `null` for a spectator recording                                                                                                                                                           |
| Match timing                                                          | Countdown-derived match elapsed time, separated from recorded span; round length is exact only when the recording observed the pre-match countdown                                                                                    |
| Match ending                                                          | Final countdown remaining plus final bar position; never inferred without a `TeamWins` winner                                                                                                                                         |
| Domination shares                                                     | Final bar split attributed by POV team, falling back to the `TeamWins` winner; Domination and Tug of War only                                                                                                                         |
| `incomplete`                                                          | Conservative result-state classification documented in `README.md`                                                                                                                                                                    |
| Recorder tactical-aid summary                                         | Deterministic count of emitted recorder-only placement events                                                                                                                                                                         |
| Tactical-aid marker                                                   | Raw recorder-visible-faction effect marker with exact player; not a reconstructed purchase or bundle                                                                                                                                  |

Unknown identities and attribution remain `null`. Raw IDs are retained rather than
replaced with guessed names.

The canonical result roster retains its existing match-start resolution. A stale
lobby name can additionally be replaced only when its nonzero-score slot is vacant
at the first gameplay clock, exactly one named session enters before the first
recorded `TeamWins`, and that entrant's team joins agree with the existing playing
team. The end summary must also have nonzero per-role scores; the explicit-role
fallback does not expand this name correction. Missing results,
multiple entrants, active match-start occupants, missing roles, or conflicting teams
leave the original resolution unchanged. This correction changes only the roster
name; it does not redistribute scores, derive roles, change teams, or add rows.

## Timeline coverage and invariants

Schema v14 guarantees:

- every `timeSeconds`, phase bound, and sample time is recording-elapsed seconds
  read from the enclosing `Event` envelope, with zero at the first envelope in the
  file, so the axis covers the pre-match lobby and the post-`TeamWins` tail;
- `preMatchChat` and `postMatchChat` each carry a `timeSeconds` on that same axis
  in addition to their offset from the match boundary, so every chat message has a
  position on the recording clock regardless of which stage it falls in;
- `durationSeconds` is the length of the recording on that axis and bounds every
  other timestamp; `matchDurationSeconds` is the separate countdown-derived
  gameplay span and is normally the smaller of the two;
- the end-of-match summary pass, which repeats the whole match with its own
  envelope clock restarting at zero, is excluded from every timeline surface;
- `participants` is sorted by `playerId` and contains one entry for every player ID
  referenced by an event, chat boundary, or recorder tactical-aid summary; it
  remains the static compatibility fallback and is not authoritative after reuse;
- `participantSessions` is sorted by player/session index, uses inclusive starts and
  exclusive ends, and preserves an explicit unknown gap after a leave until another
  valid entry;
- event-time consumers prefer `participantSessions`; they use `participants` only
  when a slot has no session data, and never substitute the static name into a gap;
- serialized player IDs remain authoritative even when `playerName` is `null`;
- chat coverage is `visibleToRecorder`, not match-global;
- tactical-aid activation coverage is `recorderOnly`, not match-global;
- tactical-aid marker coverage is `visibleFactionWithPlayer`, with the serialized
  issuing `playerId` retained and resolved through `participants` when known;
- tactical-aid deployment coverage is
  `bothFactionsWithValidatedUnitDropPlayers`: top-level effects cover both factions;
  the nine proven unit-drop definitions can carry a unique ownership-backed player,
  while ambiguous drops and all other aid families remain player-null;
- each `tacticalAidDamageThreshold` is an exact server-issued actor/target damage
  notification. It is not a kill record, and `supportId`/`supportName` remain
  `null` when the replay has no usable catalogue entry for the raw `taIndex`;
- the last valid pre-game spectator state per player is emitted at `timeSeconds: 0`,
  only if no later lobby team join or session boundary supersedes it, followed by
  any in-game `spectatorViewChanged` events in stream order;
- each `tacticalAidUsed` represents one observed placement, never a reconstructed
  single/double/triple selection;
- each `tacticalAidMarker` represents one raw effect marker and is not asserted to
  be a one-to-one purchase record;
- each `tacticalAidDeployed` represents one top-level simulation deployment with
  exact faction/type and, only when present, an explicitly sourced unit-spawn owner;
  and
- pre-match and post-match chat remain outside the gameplay scrub range.

The parser does not emit unit creation, movement, health updates, or every raw
simulation frame. Unit creation is consumed only as bounded attribution state for
later destruction events and validated unit-drop deployments.

Schema v18 adds nullable `destructionContext` to complete-unit and separated
infantry-member deaths. `buildingCollapse` carries the exact serialized building
ID. `destroyedWithContainer` carries the exact serialized container unit ID. A
container child may inherit a schema-v17 tactical-aid support/team only when the
container itself has that independently exact cause and the context rule has
already proven the same killer/tick relationship. Unknown attacker identity remains
null.

## Error contract

Native CLI diagnostics are written to stderr. A requested parse exits nonzero when
any input fails, including a partially successful batch; successfully parsed batch
items are still written in deterministic order.

WASM exports always return a JSON string. Failures have this shape, with the message
encoded through the JSON serializer:

```json
{ "error": "description" }
```

## Verification

`./scripts/quality.sh full` runs Ruff, Rust formatting, locked offline Rust tests,
and Clippy with warnings denied. `./scripts/verify-release.sh` additionally builds
the release CLI and WASM package. Pass `--corpus-root PATH` (or set
`WIC_REPLAY_ROOT`) to require every recorded private ground-truth fixture to run. The
ordinary ground-truth harness fails when zero fixtures execute, so an all-skipped run
cannot be mistaken for validation.

## Signed player scores

Player `score`, all `score*` breakdown fields, and `playerScores` contain signed
32-bit integers. Negative scores are preserved, not clamped to zero. `playerScores`
includes nonzero scores in descending signed order. Primary roles use the highest
nonzero role score, with the existing explicit summary-role fallback unchanged.
The JSON shape and timeline version are unchanged; viewer cache `detail-v42`
invalidates previously unsigned results. See the [signed-score investigation](../docs/signed-scores-2026-09-07.md).

## Targeted event-backed result names and scores

A complete primary chain with `TeamWins` uses each `SetScore` message's own
unsigned `aPos` and signed `aPlayerScore`, keeping the last value per slot before
the result. Zero resets remain zero; negative scores remain negative. A missing
primary result or absence of usable scores retains the legacy summary extractor.
This avoids replacing final summaries with partial recording scores.

A stale lobby name may be corrected when a captured match start and result bound
exactly one explicitly named gameplay session. This includes pregame replacements
and a sole later entrant into a vacant slot. The slot must have owned units, all
within that session on the existing result team, no earlier nonzero stream score,
and compatible team joins at match start and during gameplay. Shared gameplay
slots, unknown identities, and conflicting unit teams remain unchanged. The
correction does not reconstruct roles or change raw statistics. Existing departure
score evidence can then recover a departed entrant's score under its own guards.

See [validation and examples](../docs/targeted-event-results-2026-09-08.md).

## Departed result players

Every raw `Player` includes `leftAtSeconds: number | null`, a recording-clock time.
The viewer reads this field at the parser's recorded `f32` precision before
widening it for presentation. This preserves exact fresh/cache equivalence for
shortest-decimal JSON timestamps. Existing cache documents need no invalidation.
It requires a matching explicit named entry and explicit leave within the sole
matching gameplay session before the primary result. Reconnects, unknown occupants,
implicit replacements and missing events abstain; late recordings can qualify
when the named entry and leave are present. Raw statistics and rows remain intact.

For historical fallback results, the viewer omits the earliest confirmed departures
only when a playing team has more than eight rows, using one shared policy for summaries and fresh/cached
Overview. Teams of eight or fewer retain their departed rows. This is separate
from score recovery and never assigns spectators. Missing fields in older caches
default to null. Current cache `detail-v47` selects the final-screen projection
when available; the departure policy was introduced in `detail-v44`. See the
[departure and overflow contract](../docs/departed-roster-overflow-2026-09-08.md).

## Viewer end-screen projection

The native `final_screen_players()` API and Python reference method decode a
complete primary `SetScoreAtGameEnd` table against the occupants present at the
result. The viewer prefers this for Overview and library summaries; the existing
`ReplayData.players` / CLI historical roster contract is unchanged. Final-screen
rows use `score == scoreTotal` and do not recover departed players. Their names
and teams come from explicit entry/team state, not category-score inference.
Unavailable or ambiguous final screens retain the historical viewer fallback.
The viewer also selects that entire fallback when it restores an opposing side
missing from the final screen. Both sides use the fallback roster and scores,
including supported last-known-score recovery; no final-screen scores are mixed
in. The decoder itself is unchanged. Empty team cards
retain their normal names and contain no placeholder players.
See the [full contract](../docs/final-screen-results-2026-09-09.md). Cache
`detail-v47` invalidates earlier viewer projections, including screens selected
without preserving skipped compressed-chunk boundaries.

Final-score table decoding is independent of final occupant validation. An intact,
contiguous `SetScoreAtGameEnd` table immediately before the first framed `TeamWins`
only establishes slot statistics. The viewer still requires the primary chain and
resolved final occupants. Failed zlib candidates are retained as decompressed
splice offsets: a skipped chunk before the result requires the historical fallback,
even if the remaining events happen to align. A splice inside the table or result
also invalidates that table candidate. Damage after the result does not invalidate
the earlier screen. An unnamed re-entry never inherits the departed occupant's name.
This is internal extraction state; parser JSON and timeline contracts are unchanged.

## Viewer library search cache

Viewer database schema 13 adds `search_players`, serialized to the frontend as
`searchPlayers: { name: string, faction: string | null }[]`. Import builds these
pairs from the same selected final-screen or historical fallback roster used in
Overview, after duplicate and departure-overflow removal. Unknown factions remain
null. This is a viewer summary contract, not a parser JSON schema change.

Migration marks older summaries stale for the normal explicit library refresh;
it preserves existing names and lazy detail caches. Search never reparses replays
or requests details. See [library search](../docs/library-search.md).
