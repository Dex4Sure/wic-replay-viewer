# Timeline player-attribution evidence — 2026-08-17

> Historical schema note: this report established the schema-v7 marker-player
> bridge. Schema v8 later added both-faction deployments and schema v9 added
> validated unit-drop owners. The viewer now presents both sources; see
> `opposing-player-tactical-aid-attribution-2026-08-18.md` for the current boundary.

## Scope

This report covers the first tactical-aid vertical slice from
`notes/timeline-player-attribution-plan.md` and its subsequent parser integration.
Replay and binary evidence was not modified, Ghidra annotations were not changed,
and the replay viewer was untouched during this pass. The parser state described
below is the validated schema-v7 marker event that subsequent schema versions build
upon.

The batch first connected the recording player's exact `SupportThingUsed` purchases
to the broader tactical-aid effect stream. Follow-up binary tracing then established
that every `SupportThingMarker` directly serializes the issuing player slot. Marker
actor attribution is therefore exact rather than a candidate inference; spawn,
feedback, and projectile joins remain research output.

## Input identities

| Input | Identity | Purpose |
|---|---|---|
| `binaries/game/wic.exe` | PE32 x86, version 1.0.1.1 b35, SHA-256 `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` | Client serializers and effect-message fields |
| `binaries/game/wic_ds.exe` | PE32 x86, version 1.0.1.1 b35, SHA-256 `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` | Server tactical-aid actor lifecycle |
| `replays/main/4600.wicdemo` | SHA-256 `7c9e8344477afe232eda016a9948d1121489e77bee8ec94f08a871687e2e1f42` | Fixed reference replay and exact recorder ground truth |
| current linked replay corpus | 2,880 parsed files | Candidate coverage and collision measurement |

The corpus count is a live workspace measurement, not a portable constant. Bulk
per-replay evidence is written to the ignored
`findings/timeline-attribution-audit.json` file.

## Correction to the earlier coverage claim

The earlier statement that other players' tactical-aid activations are not in the
file at all was too broad. The precise boundary is:

- positioned `SupportThingUsed`, paired with the local `ChangeHonors` ledger, is
  recorder-only and proves the recorder's purchase;
- the replay also contains recorder-visible-faction tactical-aid markers and
  broader delayed spawns, feedback, and support projectiles from the wider
  simulation;
- marker messages retain the issuing player slot despite naming that serialized
  field `aTeam`; delayed spawns, feedback, and projectile messages still omit it.

The purchase ledger therefore remains point-of-view-local while the effect stream
is broader. The marker player field identifies non-recorder actors directly and the
local purchase ledger provides an independent semantic cross-check.

### Follow-up coverage correction — 2026-08-18

The original report used `match-visible` to describe effects the recorder's client
received; it must not be read as match-global coverage. A dedicated spectator audit
shows
that player-bearing markers follow the recorder-visible faction. Top-level
`SupportThingSpawnedDelayed` effects cover both factions in ordinary, one-team
spectator, and all-team spectator recordings, but omit the player slot. Thus exact
enemy TA type/faction/time is available, while an enemy player remains unknown
unless a player-bearing marker or a second POV recording supplies the identity. See
`findings/spectator-tactical-aid-coverage-2026-08-18.md`.

## Confirmed message evidence

| Message | Confirmed fields relevant here | Actor boundary |
|---|---|---|
| `SupportThingUsed` | support type ID, position | no serialized player; paired local honors ledger identifies recorder |
| `SupportThingMarker` | event ID, support type ID, position, issuing player slot (serialized as `aTeam`), upgrade, direction, duration | exact player |
| `SupportThingMarkerStopped` | event ID | exact through the active marker with the same event ID; absent from current corpus |
| `SupportThingSpawnedDelayed` | support type ID, position, team, upgrade, direction, age | no player |
| `SupportThingFeedback` | support type ID, team | no player |
| support projectile creation | projectile ID, support type ID, trajectory and optional target | no player |
| `RequestSent` | request type/ID and creator player slot, optional TA type | creator is exact for the request only; request is not execution proof |

Relevant `wic.exe` serializers confirmed in Ghidra are
`SupportThingUsed` at `0x00b830c0`, `SupportThingSpawnedDelayed` at `0x00b87030`,
`SupportThingFeedback` at `0x00b86fb0`, and support-projectile senders at
`0x00b86340`, `0x00b86250`, `0x00b86100`, and `0x00b85fc0`. The server receive path
at `wic_ds.exe:0x004aa450` calls the support-use path at `0x004df860`, where the
issuing player is still available before later replay effect messages discard it.

`wic.exe:0x00b86e70` serializes `SupportThingMarker`. Its nominal `aTeam` argument is
the sixth payload field. The receiver at `0x0093fa10` records the message and keeps
the event ID as the key of the active marker. `wic.exe:0x00b86e10` serializes
`SupportThingMarkerStopped`; `0x009373a0` receives it and `0x008b6b30` removes the
active marker with that exact event ID.

The server actor path is equally explicit:

1. `wic_ds.exe:0x004aa450` resolves the requesting connection to its player object
   through `0x004b3d70`, then calls `EXG_Support::Use` at `0x004df860` on that
   player's embedded support object.
2. The `EXG_SupportBarrage` constructor at `0x0051bb30` stores the issuing player
   object at offset `0x28` and generates or accepts the marker event ID at offset
   `0xc0`.
3. `0x004df860` and child-barrage path `0x004decc0` call `0x004f6420` with both the
   connection's player-slot byte and the barrage event ID. That manager record keeps
   the slot and event ID together.
4. Barrage update `0x0051b020` again obtains the slot from the issuing player's
   connection and passes the event ID into support-projectile creation.

The client has the same object layout at `wic.exe:0x00bb5c50`; `0x0070f330` stores
the player slot and event ID together, and `0x0070f1f0` uses that byte to index the
16-player table. This binary path, the 0–15 value domain, and the independent
recorder-ledger comparison jointly establish that marker `aTeam` is a player slot,
not a faction ID.

## Reproducible tooling

`scripts/timeline-attribution-audit.py` builds on `scripts/wic_bintag.py` and emits:

- exact recorder purchases and honors deltas;
- exact marker player IDs plus delayed-spawn, feedback, support-projectile, and
  request evidence;
- raw event times, decompressed offsets, support IDs, positions, player/team fields,
  event IDs, projectile IDs, and request creators where serialized;
- exact marker-stop lifecycle links when a stop record exists;
- purchase-to-marker and purchase-to-spawn candidate components;
- explicit unique, ambiguous, and unmatched results.

Reproduction:

```bash
scripts/timeline-attribution-audit.py replays/main/4600.wicdemo \
  --include-event-rows --json findings/timeline-attribution-4600.json

scripts/timeline-attribution-audit.py \
  replays/main replays/settings replays/wicgate-documents \
  --jobs 4 --json findings/timeline-attribution-audit.json
```

The default rule requires:

1. identical support type;
2. exactly equal serialized world position;
3. the effect to occur no earlier in stream order than the purchase; and
4. exactly one complete one-to-one matching for the entire connected candidate
   component.

No elapsed-time cutoff, nearest-event score, support cost, or support name is used.
Time deltas remain observations. A nonzero position tolerance is available only as
an explicit research option and is recorded in the output; the validated default is
zero.

## Reference replay result

`4600.wicdemo` contains:

- 16 positioned recorder purchases, all paired with a negative `ChangeHonors`;
- 114 support markers;
- 131 delayed support spawns;
- 107 feedback records;
- 906 support-projectile creation records; and
- 10 player-authored request records.

All 16 purchases have a unique exact-position marker candidate and a unique delayed-
spawn candidate. Marker links are effectively immediate. Delayed-spawn links range
from 0 to about 5.041 seconds in this replay, showing why delay must be reported
rather than guessed from one fixed family-wide threshold.

## Full-corpus result

| Measurement | Result |
|---|---:|
| Replays parsed | 2,880 |
| Positioned recorder purchases | 42,344 |
| Purchases with exact recorder-ledger evidence | 42,344 |
| Match-visible support markers | 199,451 |
| Markers with a valid serialized player slot (0–15) | 199,451 |
| Markers with an invalid player slot | 0 |
| Independently matched recorder markers agreeing with recorder ID | 42,340 |
| Independently matched recorder markers disagreeing with recorder ID | 0 |
| `SupportThingMarkerStopped` records | 0 |
| Delayed support spawns | 370,971 |
| Support feedback records | 297,854 |
| Support projectiles | 1,713,693 |
| Requests with exact request creator | 81,662 |

The raw research audit includes all 2,880 linked corpus paths. The release parser
correctly rejects 15 corrupt/empty paths before emitting a timeline; those rejected
paths account for 860 markers because the same seven corrupt replays occur through
two linked corpus roots.

Purchase-to-marker results:

| Classification | Count | Share |
|---|---:|---:|
| Unique global assignment | 42,340 | 99.9906% |
| Ambiguous | 4 | 0.0094% |
| Unmatched | 0 | 0% |

All 42,340 selected marker links have exactly equal serialized positions, and every
marker's direct player field equals the independently known recorder ID. Their raw
event-time deltas range from 0 to approximately 0.065 seconds. This purchase join is
now validation of the direct marker field, not the source of marker attribution.

Purchase-to-delayed-spawn results:

| Classification | Count | Share |
|---|---:|---:|
| Unique global assignment | 42,255 | 99.7898% |
| Ambiguous | 10 | 0.0236% |
| Unmatched | 79 | 0.1866% |

All selected spawn links also have exactly equal positions. Their observed raw time
deltas range from 0 to approximately 18.188 seconds. The 79 unmatched purchases are
not failures of the broader effect graph: every one has a marker candidate, and some
support families use markers without `SupportThingSpawnedDelayed`.

## Remaining candidate ambiguity

The four purchase-to-marker graph ambiguities are deliberately retained:

- three purchases of support type `0x3aff068f` in
  `750__supporthometown-usa.wicdemo` share four compatible exact-position markers;
- one purchase of the same type in
  `old/Mixed vs BTR Hometown US Double Inf.wicdemo` has two compatible markers.

The global one-to-one graph has multiple complete solutions in both components.
Choosing the closest marker would resolve them cosmetically but would add an
unvalidated timing heuristic. This no longer creates actor ambiguity: each marker
already carries its exact player slot. Only the question of which recorder purchase
corresponds to which same-position marker remains ambiguous.

## Acceptance decision and next work

This batch establishes exact player attribution for every recorded
`SupportThingMarker` and
preserves its event ID for later lifecycle joins. The rule is implemented as the
raw `tacticalAidMarker` event in timeline schema v7. On all 2,865 accepted corpus
replays, the parser emitted 198,591 markers with zero invalid slots, unresolved
participant names, missing participant references, or timestamps beyond the
observed timeline. Proposed spawn attribution remains `researchCandidate`; marker
actor attribution does not.

Next research should:

1. determine whether the in-memory marker event ID can be joined reproducibly to
   feedback, projectile, or damage messages after serialization drops that ID;
2. locate multiple POV recordings of the same match to fill opposing-faction player
   identities that the team-only deployment stream omits; and
3. build participant team/role intervals before general combat attribution.
