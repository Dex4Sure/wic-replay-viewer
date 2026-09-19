# Opposing-player tactical-aid attribution plan

## Objective

Recover the exact opposing player responsible for a tactical-aid deployment when
the replay evidence supports that conclusion. Preserve team-only deployments when
no reproducible player bridge exists, and never silently promote a plausible actor
to an exact one.

This extends the established schema-v8 result: a single replay exposes exact
tactical-aid type, faction, position, and deployment time for both sides, while
player-bearing `SupportThingMarker` records normally cover only the
recorder-visible faction.

## Current evidence boundary

- `SupportThingMarker` (`wic.exe:0x00b86e70`) contains the exact issuing player
  slot, but follows the recorder-visible faction rather than providing a global
  player ledger.
- `SupportThingSpawnedDelayed` (`wic.exe:0x00b87030`) covers both factions and
  contains support ID, position, faction, upgrade, direction, and age, but no
  player field.
- `SupportThingUsed` plus a preceding `ChangeHonors` is the recorder-only
  purchase/cost ledger.
- `RequestSent` contains the exact requesting player and can contain the requested
  tactical-aid type. A request is not proof that the same player later purchased
  or deployed the aid.
- Support projectile and feedback records identify effects or teams but currently
  expose no issuing player.
- The server-side support-use path still has the player object before serialization
  drops that identity from the globally visible effect record.

Therefore a lone replay does not currently provide a universal exact opposing-player
field. The missing identity may still be recoverable for bounded aid families or
matches through an independently provable join.

## Attribution contract

- `exact`: a serialized player-bearing marker, a matched alternate point of view,
  or a separately validated one-to-one identifier/ownership bridge.
- `likely`: a request or other candidate correlation that narrows the actor but
  does not prove who purchased and deployed the aid. Present the basis explicitly,
  for example `requested by`, rather than `used by`.
- `unknown`: faction and aid are known but no defensible player link exists.

Raw records and provenance remain available underneath the presentation layer.
The viewer must not populate its player field from a `likely` correlation.

## Investigation order

### 1. Unit-drop ownership bridge

Start with Airborne Infantry, Airdropped Light Tank, Airdropped Transport, and any
other proven top-level aid that creates player-controlled units.

1. Correlate each deployment with nearby `UnitCreate` records using exact support
   family behavior, deployment position, replay order, and a bounded time window.
2. Determine whether the created unit owner is consistently the purchasing player,
   the intended recipient, a faction/system owner, or varies by aid.
3. Reject ambiguous many-to-many assignments and overlapping drops.
4. Validate the rule on recorder-visible deployments where `SupportThingMarker`
   already supplies ground-truth player identity.
5. Apply it to opposing deployments only if the visible-faction validation is
   deterministic and collision-free across the corpus.

This is the most promising single-replay path to exact attribution, but it is
limited to unit-spawning aids and remains a hypothesis until the ownership rule is
validated.

### 2. Duplicate and multi-POV match alignment

1. Fingerprint matches using map, server/date metadata, roster, factions, duration,
   and stable shared timeline events.
2. Detect recordings of the same match from different players or spectator views.
3. Align replay clocks with shared deployments using exact type, faction, position,
   and event sequence rather than timestamp proximity alone.
4. Import the opposing POV's player-bearing marker only when the match and event
   mapping are unique.
5. Record both replay hashes and the alignment evidence as attribution provenance.

An alternate POV is the strongest general route because it supplies the missing
serialized marker instead of inferring a player.

### 3. Request-to-deployment correlation

1. Inventory `RequestSent` records with creator slot, request ID, optional support
   type, team state, and time.
2. Measure how often a request has one compatible later deployment and how often
   multiple players or deployments remain possible.
3. Check visible-faction cases against marker ground truth to estimate false links
   and determine whether requester and user commonly differ.
4. Expose successful correlations only as candidate metadata such as
   `requested by`; do not make them exact without a stable shared identifier.

Role eligibility, aid price, unit proximity, score changes, and timing may help
rank candidates, but none is sufficient proof by itself.

### 4. Deeper executable and message tracing

1. Trace backward from the delayed-spawn client receive path near
   `wic.exe:0x0093fcd0` and the serializer at `wic.exe:0x00b87030`.
2. Follow the server support-use and network-dispatch paths while the player object
   and support event still coexist.
3. Look for a stable request ID, event ID, projectile ID, spawned-unit ID, or
   faction-private companion message that survives into replay serialization.
4. Compare both `wic.exe` and `wic_ds.exe`; record executable hashes and addresses
   for every claimed bridge.
5. Test any proposed identifier graph across the corpus before changing the parser
   contract.

This phase may unlock artillery, bombing, and nuclear attribution. Without a hidden
join key, those effect-only aids will generally remain team-level in a lone replay.

### 5. Spectator and view-transition cases

Audit matches where the recorder switches visible faction. A player-bearing marker
emitted while the opposing faction is visible is exact for that interval. Keep the
serialized spectator team/LOS state and marker stream order as provenance; do not
assume visibility outside the proven interval.

## Verification gates

- Establish the exact replay and executable hashes for static or corpus evidence.
- Validate proposed joins first against events whose exact marker player is already
  known.
- Require unique assignments and zero contradictions in the validation set before
  emitting a new `exact` attribution source.
- Preserve separate fields for deployment actor, requester, ownership bridge, and
  confidence/provenance.
- Include ordinary player, one-team spectator, all-team spectator, Few Player Mode,
  overlapping deployments, and known-corrupt fixtures in the corpus audit.
- Finish with manual replay review of representative exact, likely, and unknown
  cases; visual agreement corroborates a bridge but does not create one.

## Expected outcome

The realistic near-term result is exact opposing-player attribution for matches
with an alternate POV and potentially for validated unit-drop families. Some other
events may gain a useful `requested by` candidate. Artillery, bombing, and nuclear
deployments should remain exact-team/player-unknown unless executable tracing finds
a serialized identifier that proves the actor.

## Execution result — 2026-08-18

The single-replay routes above have now been exhausted far enough to establish a
clear boundary.

- The unit-drop path succeeded for the nine USA/NATO/USSR airborne-infantry,
  airdropped-transport, and airdropped-light-tank definitions. The shipped support
  database supplies the exact unit type, `wic_ds.exe` proves that the TA projectile
  creates that unit in the issuing player's container with `aSpawnSource=1`, and
  `UnitCreate` serializes the resulting owner, faction, type, position, and spawn
  source. A fixed, bounded join agreed with 52,875 player-bearing marker deployments
  and contradicted none. Schema v9 emits the `unitSpawnOwnership` player-
  attribution basis only when all compatible units have one owner.
- The same rule attributes 39,361 deployments that have no player-bearing marker in
  the full 2,880-path forensic audit. This includes ordinary opposing-faction,
  one-team spectator, and all-team spectator evidence.
- `RequestSent` is not an actor bridge: only 395 of 81,662 requests name a TA, none
  match a later marker at the exact requested position, and 55 of the 61 same-type
  marker cases exclude the requester.
- Support-projectile IDs do not join to player-owned projectile records: 1,710,727
  support IDs and 26,983,987 normal-projectile IDs have zero per-replay overlap, and
  all 1,713,693 support-projectile records have no trailing payload.
- End-game tactical-aid score is aggregate impact score, not an event or spend
  ledger. The live honors change remains recorder-local; transfers identify only
  the transfer participants. These values cannot solve which teammate spent which
  aid without unsupported assumptions.
- The dedicated server retains the actor and invokes `OnTAExecuted` before replay
  serialization, but that Python callback is server-local. No general multiplayer
  callback record or stable request/event/projectile identifier survives into the
  replay.

The remaining unknowns are therefore real evidence gaps rather than untried parser
joins. Non-unit strikes without a player-bearing marker and ambiguous or unmatched
unit drops remain player-null. Alternate-POV alignment is the next general route,
but it is deliberately deferred until suitable recordings exist. Full addresses,
hashes, corpus counts, ruled-out paths, and reproduction commands are recorded in
`findings/opposing-player-tactical-aid-attribution-2026-08-18.md`.

## Viewer integration verification — 2026-08-18

The first local schema-v9 review build exposed an integration defect rather than a
parser contradiction. The viewer could deserialize and display
`unitSpawnOwnership`, but its `PARSER_CACHE_KEY` still identified the nested parser
as timeline schema v8. Existing SQLite detail rows therefore remained current from
the viewer's perspective and the schema-v9 parser was not invoked for replays that
had already been opened.

The pre-correction local database contained two schema-v8 details, for
`demo398.wicdemo` and `demo400.wicdemo`. Their 76 top-level deployments all lacked
the new deployment-level player fields. Direct schema-v9 parsing of those same
immutable files recovered 27 of 36 and 23 of 40 deployment players respectively:
50 exact unit-spawn owners in total, with 26 raw deployments still player-null.
Recorder-visible marker matching can resolve some remaining presentation rows, but
non-unit effects and ambiguous or unmatched unit drops remain intentionally
unknown.

The correction is now locally committed as a complete three-repository chain:

- parser `7b2756b` emits timeline schema v9 and the conservative unit-drop join;
- viewer `3c86822` pins that parser, uses
  `timeline-v9/detail-v5`, displays the provenance, and automatically invalidates
  schema-v8 details; and
- parent `0d6e836` records both component Gitlinks with the audit tooling and full
  evidence report.

The viewer quality gate now compares the nested parser commit and parser timeline
constant against `PARSER_CACHE_KEY`. A future parser-pin or schema change that omits
cache invalidation fails before commit. The commits remain local until they are
published child-first. This integration correction does not alter the evidentiary
boundary: it makes the proven schema-v9 identities visible, but it cannot supply
players for replay records that never serialize a credible actor bridge.
