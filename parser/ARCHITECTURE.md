# WiC Replay Parser Architecture

## Scope

This directory is the canonical parser for World in Conflict `.wicdemo` files in
the combined replay-viewer repository. It owns the Rust parser used by the viewer,
native CLI, and optional WASM exports, plus a Python reference parser for
match-result comparison.

Parser development happens here. Changes to its public JSON output require
coordinated schema, viewer, cache, test, and documentation updates.

## Components

| Path                          | Responsibility                                                                              |
| ----------------------------- | ------------------------------------------------------------------------------------------- |
| `rust_parser/src/parser.rs`   | Shared decompression, BinTag extraction, match results, and timeline model                  |
| `rust_parser/src/main.rs`     | Native human-readable, JSON, timeline JSON, and corpus-summary CLI using the library parser |
| `rust_parser/src/lib.rs`      | WASM string-to-JSON entry points and structured error responses                             |
| `wic_replay_parser.py`        | Standalone Python match-result reference parser                                             |
| `test_ground_truth.py`        | Native Rust match-result regression harness over private fixtures                           |
| `scripts/benchmark-parser.sh` | Release-mode replay and corpus timing with deterministic output hashes                      |
| `scripts/quality.sh`          | Fast and full local quality gates                                                           |
| `scripts/verify-release.sh`   | Full gate plus release CLI, WASM, and optional ground-truth verification                    |

The Rust crate has one shared parsing implementation compiled once. Its native and WASM entry
points therefore differ only in input/output plumbing; both use the standalone
built-in map display-name translation.

## Input pipeline

`.wicdemo` files contain a 19-byte header followed by independently compressed zlib
chunks. The first decompressed chunk holds metadata; the concatenated decompressed
stream carries metadata, score blocks, and BinTag gameplay events.

The parser applies five independent resource limits before exposing data:

- 64 MiB compressed input;
- 96 MiB aggregate decompressed output;
- 6,144 accepted zlib chunks;
- 16,384 zlib-header candidates, including failed inflate attempts; and
- 256 MiB of cumulative input consumed by accepted and failed inflate attempts.

Each accepted decompressed chunk is limited to 64 KiB. Oversized, truncated, empty,
or over-budget data fails closed. The compressed-input and candidate limits were set
with headroom over the deduplicated 2,880-file corpus, whose observed maxima are
18,681,134 compressed bytes and 6,873 raw header candidates. The cumulative work
budget prevents a smaller number of deliberately expensive failed candidates from
evading the count limit.

After decompression, the parser requires at least one `TeamWins` record. This rejects
the established corrupt replay class before match or timeline output is created.

The concatenated stream is then scanned once for every four-byte BinTag used by the
match and timeline models. The resulting byte-ordered event index preserves the
offsets and overlapping-match semantics of the earlier independent searches while
avoiding repeated whole-replay scans. Match and timeline parsing reuse the same
index, clock reconstruction, base player-name directory, and recorder slot. Timeline
identity reconstruction additionally replays `PlayerEntersGame` and
`PlayerLeavesGame` in byte order so a numeric slot can have multiple occupants.

## Match-result model

`ReplayData` provides map/server/date/mode metadata, the result roster, final scores,
teams/factions, per-role scores and primary role, match timing, how the match ended,
winner and final domination percentages, incomplete-result classification, and
recorder identity.

The viewer separately selects replay end-screen rows through
`final_screen_players()`. Internal table decoding validates contiguous final-score
records before the first framed `TeamWins`; occupant validation still requires
reliable primary-chain entries, departures, and team changes. Decompression keeps
skipped-chunk offsets so an accidentally aligned stream cannot hide lost identity
evidence. Missing evidence retains the historical result projection.
The viewer caches name/faction pairs from that selected roster for
[structured library search](../docs/library-search.md), without loading timeline
details during searches. This does not
change the CLI result or timeline JSON contract.

Match time and recording time are separate. The countdown reads zero only before it
starts, so a recording that observed a zero sample covers the match from its first
second; without one the recorder joined mid-match, and match time is recovered as
`round length - remaining`. The countdown keeps ticking past zero into the
post-match screen, so it is clamped before subtracting.

`matchEnding` separates total domination (bar pinned to a stop with time still on
the clock) from a timer finish (countdown expired, bar frozen wherever it stood)
and a forfeit (real winner, time left, bar unpinned).

`aFactor` is reported from the recording player's perspective and is mirrored when
that player changes side, so the final split is attributed using the POV team
resolved at the final sample, falling back to the `TeamWins` winner for a spectator
recording. Its semantics are mode-dependent: a continuous lead bar on Domination, a
discrete front line on Tug of War, and attacker progress on Assault — which is why
Assault carries no control split.

Player names come from structural `aSlot` records. The parser corrects only proven
bilateral lobby swaps. For older or split metadata, it performs a bounded fallback
over the concatenated stream for player IDs that the result or timeline actually
requires.

The server/session title comes directly from the variable-length UTF-16
`myGameName` metadata field. No content or date-based heuristic is used to decide
which metadata string is the server name.

There is no serialized scoreboard table. Final result fields are reconstructed from
the same event and score data the game client used. `SCHEMA.md` identifies which
outputs are serialized facts and which are conservative derived values.

## Timeline model

Timeline output is opt-in and independently versioned. Schema version 18 contains:

- observed duration, initial/final clocks, and countdown-derived phases;
- five-second domination samples with the final sample retained, re-expressed in a
  single frame anchored to the final sample, plus the faction that frame belongs to;
- one static compatibility participant directory for every referenced player ID;
- half-open participant sessions that preserve exact entry names, slot reuse, and
  unknown intervals after a player leaves;
- discrete player, spectator-view, command-point, destruction, tactical-aid
  purchase, exact actor/target damage-threshold notification, player-attributed
  marker, both-faction deployment, chat, vote, and win events;
- separate pre-match and post-match chat boundaries;
- explicit point-of-view coverage; and
- a deterministic recorder-only tactical-aid placement summary;
- raw recorder-visible-faction tactical-aid markers with issuing-player slots;
  and
- top-level deployment effects covering both factions, with an optional exact
  `unitSpawnOwnership` player for the nine validated unit-drop definitions and
  `null` for ambiguous drops or other aid families.

Every timeline `timeSeconds` comes from the record's own `Event` envelope clock,
with the repeated end-summary chain excluded when its envelope time resets. The
game-mode countdown remains separate match/phase evidence and never substitutes for
recording time.

Unit creation is parsed as bounded ownership/type state and is used to attribute
later destruction events and validated unit-drop deployments. It is not emitted.
Movement, health updates, and raw frames are outside the lightweight timeline.

## Point-of-view data

### Chat

`PlayerReceiveChat` contains sender slot, UTF-16LE message, and all/team channel.
The replay stores what the recorder's client received: match-wide all-chat and only
the recorder's visible team-chat. It is not a complete match-global chat log.

Pre-game messages are exposed as `preMatchChat`; messages after the final valid
`TeamWins` envelope are exposed as `postMatchChat`. Each carries its own
`timeSeconds` on the recording axis as well as its offset from the match boundary,
so a consumer can present all three chat stages on one clock. The distinct
private-chat shape is excluded because the only corpus examples are allied computer
command acknowledgements, not evidence of human private messages.

Chat text is untrusted content. The parser validates envelope bounds, field lengths,
player slots, and UTF-16 structure. Escaping and moderation remain consumer
responsibilities.

### Tactical aid

`SupportThingUsed` activations are serialized only for the recording player. A
negative recorder-ledger `ChangeHonors` immediately paired with a positioned support
record produces one `tacticalAidUsed` placement containing raw support ID, proven
name when available, marginal honors cost, world position, and recorder slot.

Zero-position catalogue records are not activations. Costs do not identify support
names, and the replay does not serialize selected single/double/triple bundle size.
Queued placements have no time limit, so the canonical parser never groups them.

`SendTATaunt` independently records the acting player, affected player, support-
manager index, and upgrade level after the dedicated server's cumulative tactical-
aid damage score reaches that support's taunt threshold. Schema v10 emits this as
`tacticalAidDamageThreshold`. The index is resolved through the replay's ordered
zero-position support catalogue when present; older or custom replays retain the
raw index with a null support ID/name. The notification is damage attribution, not
a unit-kill record, so the parser does not attach nearby or simultaneous
`UnitDestroy` events.

Timeline schema v15 also maintains exact unit lifecycle generations through
`UnitCreate`, `UnitRemove`, replacement, and `UnitDestroy`. A removed killer's
player/team is recovered only from a preceding, at-most-0.5-second
`ProjectileHomingUnitCreate` whose firing unit equals `UnitDestroy.aKiller`, whose
target equals the destroyed unit, and whose target generation is still active.
The firing owner is captured when the projectile is created; conflicting actors,
future records, generic historical ownership, and killer sentinel `512` abstain.

Timeline schema v16 classifies the 27 explicit multiplayer infantry-member type
hashes recovered from the shipped `units/unittypes_wic.ice/.loc` catalogue.
Individual member destructions retain their exact lifecycle and attribution in
`infantrySoldierDeaths`; only complete units and the separately serialized
`*_Squad_*` parent destructions enter `events` as `unitDestroyed`. Unknown types
remain unit events so an incomplete catalogue cannot suppress evidence.

Timeline schema v17 adds a bounded tactical-aid destruction cause. A homing support
projectile must target the exact active unit generation no more than 3 seconds before
a sentinel-512 destruction. Its shipped faction-TA support ID must resolve through
recent `SupportThingSpawnedDelayed` records to one issuing team, and every qualifying
projectile must agree on the same support/team pair. The parser emits the team and
support identity, never a player inferred from timing; conflicting, reused, stale,
non-TA, or non-sentinel evidence remains `unknown`. The shipped support database
explicitly links Heavy Air Support's child projectile definitions to their top-level
parent; output preserves the child ID and reports that proven parent name.

Timeline schema v18 adds exact mechanical destruction contexts. The dedicated
server's building and container fatal paths both call a helper that generates one
of 10,000 exact normalized X/Z terminal directions before writing `UnitDestroy`.
For an occupied-building collapse, the parser also requires the exact active unit
generation's serialized slot occupancy and a same-raw-tick state-3 building event.
For a destroyed container, it requires the exact type-2 relation generations,
teardown ordering, same-tick parent/child terminals, and identical killer IDs.
Ambiguous overlap or any failed predicate produces no context. The context is
orthogonal to `cause`: it explains the mechanical path without guessing who dealt
the initiating damage. A container child's support/team is inherited only from an
independently exact schema-v17 tactical-aid cause on its proven parent.

Top-level `SupportThingSpawnedDelayed` records independently provide both factions'
support type, position, faction, direction, upgrade, and age. For the nine shipped
unit-drop definitions, schema v9 can attach a player only when an exact-type,
same-faction, `aSpawnSource=1` `UnitCreate` falls inside the validated arrival and
position bounds and all compatible units have one owner. The attribution basis is
serialized as `unitSpawnOwnership`; every other or ambiguous deployment remains
player-null.

## Interfaces and failures

The native CLI supports human-readable match results, `--json`, `--timeline-json`,
and deterministic bounded-worker corpus summaries. A parse command exits nonzero if
any requested input fails.

WASM exports return JSON strings for match results, timeline-only output, or their
combined object. Errors are serialized as `{"error":"..."}` so quotes, backslashes,
and control characters cannot produce invalid JSON.

The public compatibility and schema-version rules are defined in `SCHEMA.md` and
locked by exact serialization tests covering all schema-v18 event variants.

## Validation

Fast checks run Ruff and Rust formatting. Full checks additionally run all-feature
Rust tests offline and locked, then Clippy with warnings denied. Release verification
also builds the optimized CLI and WASM package and can require private ground-truth
fixtures. `wasm-pack` helper state is redirected into ignored project-local `state/`
so release verification also works when the host home directory is read-only.

The private corpus itself is never committed. Current reproducible aggregate results
are recorded in the tracked path-free viewer baseline and dated in `README.md`.
Harness self-tests cover a missing binary, missing fixtures, zero execution, partial
and strict execution, parser failures, and semantic differences. The ordinary
ground-truth harness rejects an all-skipped run; private/release verification is
stricter and requires every one of the 17 recorded fixtures. Rust is canonical;
`wic_replay_parser.py` remains an independently useful secondary reference and has
no numeric coverage floor.

## Desktop live scoreboard

The desktop-only lazy playback projection reads signed absolute `SetScore`
(`aPos` unsigned slot, `aPlayerScore` signed score) observations and
`PlayerEntersGame.team`, `PlayerJoinedTeam` and `SpectatorJoinedTeam` assignments from the primary envelope
chain, including the lobby. Spectator records mean team 0; their `aTeam` identifies
the side being left. The end-summary clock reset is excluded. Each score/team list
is bounded to one million observations.

`DetailView.scoreParticipants` projects the existing participant sessions; it adds
no canonical parser JSON fields. The detail cache key advances for this projection.
Playback observations remain lazy and uncached. The frontend indexes observations
once per replay and selects the latest at or before the playback clock. Session
boundaries prevent prior occupants' scores and teams from carrying into reused
slots. Final roster teams and end-result scores are never used as playback state.
Unknown scores render as an em dash, spectators and players without recorded teams
are excluded from the scoreboard, and team totals are
sums of displayed player scores, not a separate serialized team score.

Focused input evidence: SHA-256
`a5a43de0657fc7ec4f094792c1cdb3be5b09771ed5f2547d3b175ab93c111a18`
contains 26,896 primary-chain score records and 10 lobby team-join assignments (plus player-entry snapshots). Those
assignments are absent from the gameplay-only canonical timeline, which is why
playback reads them directly. This is a focused control, not a full-corpus audit.
