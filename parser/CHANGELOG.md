# Replay Parser Changelog (Historical)

This preserves parser-specific history through its import into the combined viewer
repository. New parser and viewer changes are recorded together in `../CHANGELOG.md`.
Timeline schema history and compatibility rules remain documented in `SCHEMA.md`.

## Unreleased

### Added

- Expose the eight serialized per-player post-match score categories alongside
  the existing role scores: capturing, fortification, transportation, repair,
  bridge laying, unit damage, Tactical Aid, and total score.

- **Breaking (timeline schema v18).** Add an exact `destructionContext` to complete
  unit and infantry-member deaths. `buildingCollapse` requires the exact active
  unit generation to occupy the serialized building slot, a same-raw-tick
  `BuildingDamaged` state-3 transition for that building, and the executable's
  exact synthetic terminal-direction signature. `destroyedWithContainer` requires
  an exact type-2 container relation between both active unit generations,
  same-raw-tick destruction with the same killer ID, valid relation teardown
  ordering, and that same signature on the child. Reuse, lifecycle, timing,
  signature, killer, overlap, or tactical-aid conflicts abstain. The context does
  not invent an attacker. When a destroyed container has an independently proven
  schema-v17 tactical-aid cause, its child inherits that exact support/team because
  the server copies the container's killer and damage-source context into the
  child fatal-damage call.

- **Breaking (timeline schema v17).** Classify unit and infantry-member deaths as
  `unit`, `tacticalAid`, or `unknown`. A sentinel-512 death is attributed to
  tactical aid only when a preceding homing support projectile names the exact
  target lifecycle, falls within 3 seconds, resolves through one faction-TA support
  definition, and all matching deployments agree on one issuing team. The event
  then carries that `killerTeam` plus `tacticalAidSupportId`/`Name`; player identity
  remains null because the support projectile carries no player slot. Heavy Air
  Support child projectile IDs are mapped to their explicitly serialized top-level
  parent relationship from the shipped support database.

- **Breaking (timeline schema v16).** Separate the replay's 27 shipped multiplayer
  infantry-member types into `infantrySoldierDeaths`. `events` now retains only
  complete unit and squad-parent `unitDestroyed` records, so consumers no longer
  present each soldier casualty as a complete player-unit loss. Unknown types fail
  open into `unitDestroyed`; killer attribution evidence is preserved in both sets.

- **Breaking (timeline schema v15).** Recover `killerPlayerId` and `killerTeam`
  for the bounded historical-killer subset where an earlier
  `ProjectileHomingUnitCreate` names both the serialized `UnitDestroy.aKiller`
  as its active firing unit and the destroyed unit as its exact target. The join
  is limited to 0.5 seconds, preserves projectile-time ownership, requires the
  target's uninterrupted unit lifecycle, rejects conflicting actors, and never
  applies to killer sentinel `512`. `UnitRemove` now closes active unit state so
  removed or reused IDs cannot supply ordinary killer attribution.

- `preMatchChat` and `postMatchChat` messages now carry `timeSeconds`, their own
  position on the recording axis, alongside the existing
  `secondsBeforeMatch`/`secondsAfterMatch` offsets from the match boundary
  (timeline schema v14). The envelope timestamp was already in hand where those
  offsets are computed and was being discarded, leaving a consumer unable to place
  a pre- or post-match message on the same clock as every other timeline
  timestamp without re-deriving the match boundary itself. Both readings are now
  serialized; the existing offsets are unchanged.

### Documentation

- Recorded the exact serialized message names alongside the hash constants, now
  that walking the callers of `wic.exe:0x009240a0` has enumerated all 169
  messages the client can write. All 19 message-hash constants resolve with no
  mismatch. Three `IndexedTag` variants keep Rust casing and carry their wire name
  in a doc comment: `SetGameModeData_Float` (with an underscore — the literal
  `SetGameModeDataFloat` hashes to a value present in no replay),
  `ShowPlayerGiveTANotification`, and `SendTATaunt`. `aFactor` is noted as the
  single field of `UpdateBalanceFactor`. No parsing behaviour changes.

### Changed

- Keep the established community map-name table as the standalone fallback,
  correct `as_AirBase`, and add the missing `tw_Radar`, `tw_Highway`, and
  `tw_Wasteland` names from game/localization evidence. Preserve the additional
  community-compatible paths while normalizing Airport and Wake revisions to
  `do_Airport` and `do_Wake` rather than inventing numbered public names or
  omitting the Domination prefix.

- **Breaking (timeline schema v13).** Every timeline `timeSeconds`, phase bound,
  and sample time is now recording-elapsed seconds read from the record's own
  `Event` envelope, with zero at the first envelope in the file. Schema versions
  through 12 interpolated event positions between countdown samples, which pinned
  zero to the first countdown record and collapsed all pre-match lobby activity
  onto it. A 156-replay scan found the countdown unusable as a time axis: it
  starts a median of 66.8 s into the recording (up to 22 minutes), restarts
  between Assault rounds, and ticks at 0.84-1.25 s of clock per second of
  recording, with one replay at ~2.4x. The envelope clock starts at zero and
  advances monotonically in all 156.
- **Breaking (timeline schema v13).** `TimelineData.durationSeconds` is now the
  length of the recording, so it bounds every other timestamp. The previous
  countdown-derived value moved to the new `matchDurationSeconds`. In 62 corpus
  replays the old value exceeded the recording that observed it — by up to 28
  minutes — which the new bound makes impossible.
- **Breaking.** `ReplayData.timing.recordedSeconds` is renamed
  `observedGameplaySeconds`. It was countdown-derived despite its name, and the
  new `recordingSeconds` alongside it needed an unambiguous neighbour.
- The CLI now prints `Match length` and `Replay length` rather than `Duration`
  and `Recording`.

### Added

- Added `ReplayData.timing.recordingSeconds`: the real length of the replay file,
  measured from the `Event` envelope chain and counting pre-match lobby and
  post-`TeamWins` tail. It exceeds `observedGameplaySeconds` by a corpus median of
  76 s and by up to 22 minutes.
- Added `TimelineData.matchDurationSeconds`, carrying the countdown-derived
  gameplay span that `durationSeconds` used to hold.

### Fixed

- Resolve the player occupying each scored numeric slot at the first valid gameplay
  clock sample before attaching final score and role summaries. This corrects older
  replays where lobby metadata still names a player who left before the match and a
  different player entered the same slot. Structurally decoded entry names supersede
  metadata only when active at gameplay start; later replacements, unscored slots,
  unresolved or vacant slots, and replays without a gameplay clock retain the prior
  behavior. Trailing U+00A0 padding in older `PlayerEntersGame` names is removed.

- Fixed the end-of-match summary pass leaking into the timeline. After `TeamWins`
  a replay repeats the whole match with its envelope clock restarting at zero;
  those records are now excluded from events, chat, and domination samples rather
  than being clamped onto the end of the countdown axis.
- Fixed `durationSeconds` reporting the length of the recording rather than the
  length of the match. A recording that joined mid-match understated the match by
  up to 9.9 minutes across the corpus; match time is now recovered from the
  countdown, with the recorded span, join point, and round length exposed
  separately on `timing`.
- Fixed the final domination split being attributed from the recorded outcome
  rather than from the data. `aFactor` is POV-relative and mirrors when the
  recording player changes side, so the anchor is now resolved at the final
  sample and the split is attributed to concrete factions.
- Fixed timeline `dominationSamples` retaining POV frame changes, which made the
  curve teleport across the centreline mid-match. Frame changes are now detected
  from the recorder's actual team-change offsets and confirmed against the value,
  so a genuine discrete front-line move across the centre is left alone.
- Fixed Assault replays publishing a domination percentage. Its `aFactor` tracks
  attacker progress rather than a two-sided split.
- Fixed the final bar sample being read without a fallback, so one malformed or
  out-of-range trailing record suppressed the whole result.

### Added

- Added `timing`, `matchEnding`, `dominationShares`, and `dominationAnchor` to
  `ReplayData`, and timeline schema v12 `dominationAnchorFaction`.

- Added nullable `rawServerFlags` and an orthogonal `serverClassification` for
  FPM, Match Mode, bots, clan matches, and tournament matches. Ranked remains
  explicitly unknown because its confirmed server-browser bit is not serialized
  in the replay header.
- Added timeline schema v11 `participantSessions` with half-open identity intervals
  reconstructed from bounded, name-bearing `PlayerEntersGame` records and exact
  `PlayerLeavesGame` boundaries. Numeric slot reuse now opens a new occupant session;
  damaged names and post-leave gaps remain explicitly unknown instead of inheriting
  a stale static participant.
- Added timeline schema v10 `tacticalAidDamageThreshold` events from the server's
  `SendTATaunt` record. They retain the exact acting and affected player slots,
  support-manager index, and upgrade level, resolve the support through the replay
  catalogue when available, and explicitly do not claim that a unit was killed.
- Added timeline schema v9 optional player attribution for the nine faction unit-
  drop aids. A deployment receives `playerId` and
  `playerAttribution: "unitSpawnOwnership"` only when its shipped unit type,
  faction, binary-proven `aSpawnSource=1`, arrival band, and position yield one
  candidate owner; ambiguous and unmatched drops remain `null`.
- Added timeline schema v8 `tacticalAidDeployed` events from top-level
  `SupportThingSpawnedDelayed` effects. They expose both factions' exact TA type,
  faction, position, upgrade, direction, and age without inventing a player ID.
- Added `spectatorViewChanged` events with serialized team and `aSpectatorLos`,
  decoded one-team/all-team view, and the final pre-game state preserved at time
  zero.
- Added exact internal names for the 58 top-level faction tactical-aid definitions
  recovered from shipped support localization, while deliberately leaving child
  effects and special-ability markers unnamed.
- Added timeline schema v7 `tacticalAidMarker` events from recorder-visible-faction
  `SupportThingMarker` records, retaining the serialized issuing-player slot,
  event/support IDs, position, upgrade, direction, and duration without inferring
  purchases or multi-strike bundles.
- Added explicit tactical-aid marker coverage and corpus validation
  counters for player-slot, participant-name, and timeline-duration invariants.

### Changed

- Reconciled the current README, architecture, schema, TODO, and corpus-validation
  figures with timeline schema v11, temporal identities, and the validated unit-drop
  ownership boundary.
- Corrected tactical-aid coverage after the spectator-mode corpus audit: player-
  bearing markers cover the recorder-visible faction, while top-level deployment
  effects cover both factions in ordinary, one-team spectator, and all-team
  spectator recordings. Schema v9 additionally attributes the nine proven
  unit-drop definitions when the ownership join has one candidate player.
- Replaced repeated whole-stream BinTag searches with one byte-ordered event index,
  and reused clock reconstruction, recorder identity, and base player-name state
  between match-result and timeline parsing before the schema-v7 marker addition.
- Added a release-mode benchmark command that records replay/corpus wall time, CPU
  time, peak RSS, and deterministic output hashes.
- Locked the `ReplayData` and current timeline-schema-v11 JSON shapes with exact
  serialization tests covering every timeline event variant.
- Extended the full local quality gate and pre-push hook to run locked offline Rust
  tests before Clippy.
- Added one release verification command for the full gate, optimized native build,
  WASM package, and optional private ground truth, with helper state redirected into
  an ignored project-local directory for read-only-home environments.
- Replaced obsolete environment documentation with standalone
  parser architecture, compatibility, resource-bound, and validation guidance.

### Fixed

- Replaced keyword-based server-name guessing with structural UTF-16
  `myGameName` decoding, added server/date ground-truth comparisons, and validated
  non-`Unknown` server names across all 2,865 accepted corpus replays.
- Made private ground-truth validation fail when every fixture is skipped, and made
  release verification require every recorded fixture.
- Made native parse commands exit nonzero when any requested input fails, including
  partially successful batches, with an integration test preserving valid partial
  JSON output alongside the failing status.
- Serialized WASM error responses through `serde_json`, preserving valid JSON for
  quotes, backslashes, and control characters.

### Security

- Added 64 MiB compressed-input, 16,384-candidate, and 256 MiB cumulative inflate
  work limits alongside the existing decompressed-size and accepted-chunk limits.
