# Timeline player-attribution plan

## Objective

Identify which replay participant performed each timeline action whenever the
recorded evidence supports that conclusion. Preserve raw identifiers and make the
reason for every attribution auditable. An action remains team-attributed,
ambiguous, or unknown when the available constraints do not identify one player.

This work is deliberately ordered from read-only research to parser integration and
then viewer presentation. The parser and replay viewer must not absorb research
hypotheses before corpus validation establishes their safety.

## Progress — 2026-08-17

- Phases 1–4 are complete for the tactical-aid vertical slice across 2,880 replays.
- Phase 4 found the missing direct bridge: the field serialized as
  `SupportThingMarker.aTeam` carries the issuing player slot, not a faction/team
  value. The server and client retain that slot together with the marker event ID.
- All 199,451 corpus markers contain a slot in 0–15. All 42,340 independently
  matched recorder purchases agree with that serialized slot; there are zero
  mismatches.
- No `SupportThingMarkerStopped` message occurs in the current corpus, although its
  exact event-ID lifecycle is implemented in the research audit for future inputs.
- The tactical-aid marker portion of phase 7 first shipped in schema v7. The current
  local parser is schema v9 and passes the full parser and parent quality gates.
- Follow-up schema-v8 work establishes the coverage boundary: marker actors are
  exact for the recorder-visible faction, while top-level delayed-spawn deployments
  cover both factions but omit the player. One-team and all-team spectator modes
  are now decoded and preserved.
- Follow-up schema-v9 work supplies a validated `derivedUnique` player for the nine
  faction unit-drop definitions when exact shipped type, same faction,
  `aSpawnSource=1`, bounded arrival/position, and a single owner all agree. The
  serialized attribution-basis value is `unitSpawnOwnership`. The rule has 52,875
  agreements and zero contradictions against player-bearing marker ground truth.

This result removes the need for multi-POV inference to identify actors on recorded
markers and on validated unit drops, but not for other enemy-player actions whose
marker is absent. Schema v9 emits 223,426 both-faction top-level deployments across
2,865 accepted replays; 91,932 have a validated unit-spawn owner, with zero invalid
player/faction IDs, unresolved participant names, or out-of-range timestamps. The
viewer presents exact players where markers or the validated ownership bridge exist
and neutral team-only rows otherwise. Phase 5 remains useful for filling non-unit
identities from another recording of the same match.

## Evidence contract

Attribution is classified as:

- `exact`: the player identifier is serialized by the relevant message or by an
  authoritative record that directly names the action.
- `derivedUnique`: a reproducible chain rooted in exact facts leaves exactly one
  possible player.
- `teamOnly`: the action's team is known but no individual player is established.
- `ambiguous`: two or more candidates satisfy the currently validated constraints.
- `unknown`: the replay does not provide enough evidence to construct candidates.

Only `exact` and validated `derivedUnique` results may populate a canonical player
identifier. Candidate lists, timing proximity, cost correlations, and other research
signals must not silently become facts. In particular, tactical-aid cost cannot name
an aid or its user.

Participant state is time-dependent. Joins, departures, team changes, spectator
changes, and role changes form intervals; the final match roster is not a substitute
for state at the time of an event. Replay-local identifiers must not be treated as
stable identities across different replay files.

## Work phases

### 1. Establish the attribution ledger

- Inventory identity-bearing, team-bearing, and object-bearing replay messages.
- Record confirmed field layouts, executable hashes, sender/receiver addresses, and
  representative replay offsets.
- For each event family, document what is exact, what is joinable, and what remains
  unknowable.
- Correct older findings when later evidence narrows an earlier claim. In
  particular, distinguish recorder-only `SupportThingUsed` purchases, visible-
  faction player markers, and both-faction deployments with optional validated
  unit-drop owners.

Deliverable: a reviewed findings report and a machine-readable evidence vocabulary.

### 2. Build a read-only evidence-graph extractor

- Add a reproducible audit script on top of `scripts/wic_bintag.py`.
- Emit nodes for participants, support placements/effects, units, projectiles,
  requests, and other relevant events.
- Emit edges only from explicit fields or named, testable matching rules.
- Preserve replay offset, raw event time, identifiers, position, and attribution
  basis on every result.
- Write bulk output under `findings/*.json`, which is intentionally ignored.

Deliverable: deterministic JSON for one replay or a bounded corpus.

### 3. Validate a tactical-aid vertical slice

- Connect positioned recorder `SupportThingUsed` purchases to
  `SupportThingSpawnedDelayed` and `SupportThingMarker` effects using support type,
  exact serialized position, and observed event ordering. Preserve the marker's
  serialized event ID as a possible bridge into later effect lifecycle records.
- Keep feedback and projectile records as effect evidence even when they cannot yet
  be assigned to one placement.
- Hide the exact recorder attribution and measure whether the join recovers it.
- Report unique, ambiguous, and unmatched cases; never force a nearest match.
- Solve candidate components globally as one-to-one assignments. Promote a component
  only when the validated constraints admit exactly one complete matching; otherwise
  keep every affected purchase ambiguous.
- Cover simultaneous placements, delayed effects, held multistrikes, all game modes,
  and damaged/corrupt inputs.

Gate: no observed false attribution for any rule promoted to `derivedUnique`.
Incomplete coverage is acceptable; incorrect certainty is not.

### 4. Find missing player-to-effect bridges in the binaries

- Trace the tactical-aid actor through the server use path and created support or
  barrage object in `wic_ds.exe`.
- Trace the corresponding senders, receivers, callbacks, score/honors messages,
  requests, markers, projectiles, damage, and spawned objects in `wic.exe`.
- Look specifically for a record that carries both a player slot and an identifier
  retained by the replay effect stream.
- Record observations with binary identity and addresses. Do not bulk-change Ghidra
  annotations without explicit approval.

Deliverable: confirmed bridge fields, or a documented proof of the remaining
information boundary.

### 5. Fuse multiple points of view

- Detect recordings of the same match using map, mode, roster, duration, and shared
  event signatures.
- Align clocks from shared serialized events rather than file timestamps alone.
- Merge each recorder's exact purchase ledger into a match-level evidence graph.
- Surface disagreements and missing perspectives instead of selecting a likely
  answer.

Deliverable: exact multi-recorder attribution where matching POVs exist.

### 6. Generalize the graph to combat and player state

- Execute the focused `UnitDestroy` cause and killer verification in
  `notes/unit-destruction-attribution-verification.md`, beginning with the Quarry
  replay's `killer=512` playback set.
- Resolve participant state intervals at each event time.
- Link `UnitCreate` ownership through unit and projectile lifecycles to damage and
  destruction events.
- Handle missing owners, environmental kills, reused identifiers, joins, and team or
  role changes conservatively.
- Measure coverage and ambiguity separately for every event family.

Deliverable: a corpus attribution report covering tactical aid, unit lifecycle,
combat, chat, votes, transfers, and naturally team-level events.

### 7. Promote validated semantics into the parser

- Specify a new timeline schema version because attribution fields change the public
  JSON contract.
- Keep exact facts distinct from derived attribution and expose a bounded basis enum.
- Keep speculative candidate sets in research output unless a stable consumer and
  uncertainty contract are explicitly approved.
- Add serialization-contract tests, focused replay fixtures, full corpus validation,
  ground truth, Rust tests, formatting, Clippy, and WASM verification.
- Update documentation to state coverage precisely.

Deliverable: a versioned parser release with measured attribution guarantees.

### 8. Integrate the replay viewer

- Advance the viewer's nested parser pin and cache key together.
- Resolve player IDs through the participant directory.
- Present exact and derived attribution without hiding uncertainty.
- Fall back to team or unknown-player wording when individual attribution is absent.
- Run the viewer quality gate on Linux and preserve Windows/macOS portability.

Deliverable: timeline and tactical-aid views backed by the validated parser model.

## Initial implementation batch

The first batch is intentionally limited to:

1. this written evidence contract and plan;
2. a generic read-only tactical-aid evidence extractor;
3. conservative purchase-to-effect candidate generation for delayed spawns and
   markers;
4. focused tests using synthetic events plus `4600.wicdemo` as immutable evidence;
5. a small-corpus and then full-corpus validation report.

It does not change parser output, timeline schema, viewer code, replay evidence, or
Ghidra annotations.

## Acceptance measurements

Every candidate rule must report:

- exact ground-truth actions tested;
- uniquely recovered actions;
- ambiguous actions;
- unmatched actions;
- incorrect attributions;
- candidate collisions;
- position and time deltas without converting them into undocumented thresholds;
- exact-position results by default, with any nonzero research tolerance recorded in
  the output;
- results by replay and game mode where metadata is available.

The research JSON must remain deterministic for identical inputs. A failed or
partially parsed replay must be reported explicitly rather than dropped.
