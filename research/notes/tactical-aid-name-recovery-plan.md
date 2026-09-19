# Tactical-aid name recovery and both-faction presentation plan

## Progress — 2026-08-17

- Phases 1–4 are complete: the SDF container is decoded, all 200 catalogue names
  round-trip to their replay IDs without collision, and marker player slots agree
  with all 42,340 uniquely linked recorder purchases.
- Phase 5 is complete: 58 top-level faction-aid definitions are named while child
  effects and special abilities remain unnamed.
- Phase 6 is implemented locally in the viewer with both-faction deployment rows,
  friendly categories, explicit player/team evidence, and conservative recorder-
  cost enrichment. The current viewer pins schema v9 and uses
  `timeline-v9/detail-v5` cache invalidation.
- Automated phase-7 gates pass. The remaining gate is the user's playback review
  using `findings/tactical-aid-manual-review-2026-08-17.md`.
- Follow-up spectator coverage work is complete in timeline schema v8. Player-
  bearing markers cover the recorder-visible faction; top-level delayed-spawn
  effects cover both factions but omit the player. The viewer merges only exact
  support/position groups and otherwise labels the player unknown.
- Follow-up opposing-player work is complete in timeline schema v9 for the nine
  faction unit-drop definitions. A binary-proven, zero-contradiction `UnitCreate`
  ownership join supplies exact players; non-unit, ambiguous, and unmatched
  deployments remain unknown. See
  `findings/opposing-player-tactical-aid-attribution-2026-08-18.md`.

## Objective

Recover human-readable names for replay tactical-aid support IDs and present both
factions' deployments, with the correct issuing player only where serialized
evidence supplies one. Preserve the raw replay facts and provenance so that a name
or actor is never inferred from price, timing, or visual resemblance alone.

Manual playback review is the final validation layer. It confirms that recovered
definitions produce the expected visible effect, but it does not replace a proven
definition-name/hash mapping or a serialized player identifier.

## Historical starting point

The bullets below record the evidence boundary before phases 1–6. They are retained
to explain the investigation order; the progress section above is the current
state.

- `SupportThingUsed` contains a support ID and position. When immediately paired
  with a negative `ChangeHonors`, it proves a purchase by the replay recorder and
  supplies the marginal honors cost.
- `SupportThingMarker` contains the support ID, position, marker event ID, direction,
  duration, upgrade level, and the exact issuing player slot. It covers the
  recorder-visible faction, including non-recorder players on that faction.
- Top-level `SupportThingSpawnedDelayed` covers both factions and supplies support,
  faction, position, direction, upgrade, and age, but no player slot.
- The canonical parser emits both facts separately as `tacticalAidUsed` and
  `tacticalAidMarker`. The viewer's general timeline understands markers, but its
  Tactical Aid table is still built only from recorder purchase records.
- Support IDs are Adler-32 hashes of internal support definition names. Four nuclear
  variants are currently proven from executable strings; other IDs remain unnamed.
- Costs, observed effect timing, and single/double/triple placement patterns may be
  used as cross-checks, never as naming or actor-attribution proof.

## Evidence classifications

- `exact`: serialized directly by the replay or recovered from an authoritative
  game definition whose name hashes to the replay ID.
- `corroborated`: agrees with controlled playback, prices, faction availability,
  and effect behavior but is not the source of the canonical mapping.
- `candidate`: plausible from incomplete static or runtime evidence; not shipped as
  a name.
- `unknown`: insufficient evidence; retain the raw support ID.

Every recovered mapping must record the executable/archive identity, source path or
address, internal name, calculated hash, replay support ID, and validation status.

## Phase 1 — replay support-ID inventory

1. Enumerate catalogue IDs, deployed marker IDs, and recorder-purchase IDs across
   the current replay corpus.
2. Rank IDs by deployment frequency and record faction, role, cost, duration,
   direction, upgrade, and marker/spawn behavior as non-authoritative fingerprints.
3. Select a small representative set for the first vertical slice: nuclear strikes,
   common artillery/air support, and IDs with distinctive marker fields.
4. Keep corrupt replay failures and duplicate corpus roots explicit in the report.

Deliverable: an ignored machine-readable inventory plus a tracked findings summary.

## Phase 2 — static SDF and loader investigation

1. Resolve the installed `.sdf` corpus without modifying it; record file sizes,
   SHA-256 hashes, headers, entropy, and repeated signatures.
2. Search both `wic.exe` and `wic_ds.exe` for archive filenames, file-open paths,
   decompression calls, table readers, definition loaders, and the known name-hash
   function.
3. Trace from archive open through entry lookup and decompression to the point where
   support definition names exist as plaintext.
4. Implement a read-only extractor only after the container and compression fields
   are understood well enough to validate bounds and checksums.

Deliverable: a reproducible SDF format note and extractor, or a documented static
boundary if the format remains unresolved.

## Phase 3 — bounded runtime recovery fallback

Use this phase only if static recovery stalls. Attaching to a running game requires
explicit user approval at that time.

1. Confirm the exact executable hash, image base, architecture, and Wine process.
2. Hook the definition-loader or name-registration boundary rather than broadly
   dumping process memory.
3. Log only the internal definition string and resulting support hash needed for
   the mapping.
4. Write captures under `captures/` and derived mappings under `findings/`; never
   modify the supplied executable or archive evidence.

Deliverable: a bounded, reproducible trace with a hash-verifiable name table.

## Phase 4 — mapping construction and validation

1. Hash every recovered internal name and require an exact match to the replay ID.
2. Detect collisions and conflicting definitions rather than selecting one.
3. Normalize faction variants into a separate friendly-display layer while keeping
   the canonical internal name and raw ID intact.
4. Validate prioritized IDs with controlled replays in which one known player uses
   one known aid at a recorded time.
5. Spot-check ordinary replays across factions, roles, Few Player Mode, queued
   placements, and overlapping deployments.

Gate: only exact, conflict-free mappings enter the parser. Playback observations
may corroborate a mapping but may not create one.

## Phase 5 — canonical parser integration

1. Extend the parser's support-name table with proven mappings and source evidence.
2. Preserve `supportId` and the nullable `supportName` fallback for unknown IDs.
3. Add hash round-trip, collision, serialization, malformed-input, and representative
   event tests.
4. Re-run the full parser quality gate and real-corpus validation.

The raw `tacticalAidUsed` and `tacticalAidMarker` records remain separate. Parser
integration must not reconstruct selected multi-strike bundle size or assign costs
to non-recorder players.

## Phase 6 — both-faction viewer presentation

1. Build the Tactical Aid table from `tacticalAidDeployed`, which covers both
   factions, and enrich equal exact support/position groups from
   `tacticalAidMarker`, which carries the issuing player for the visible faction.
2. Join recorder `tacticalAidUsed` purchases to markers only when support ID,
   serialized position, event order, and one-to-one assignment make the link unique.
3. Enrich those recorder rows with the observed honors cost; do not duplicate the
   same deployment and do not synthesize costs for other players.
4. Display friendly names such as `Nuclear Strike` while retaining canonical names
   and raw IDs in the detail/raw layer. Unknowns must show their hexadecimal ID.
5. Surface attribution and enrichment basis where useful for debugging.

Deliverable: one both-faction deployment row per top-level effect, with exact player
identity when serialized and neutral team-only wording otherwise, plus optional
recorder-only purchase cost.

## Phase 7 — final verification and manual review pack

1. Run parser and viewer quality gates and bounded corpus checks.
2. Produce a short manual-review list covering every newly named family and the
   edge cases above.
3. For each review event record replay path/hash, timestamp, player, support ID,
   canonical name, friendly name, visible effect, and `confirmed`, `unclear`, or
   `contradicted` status.
4. Treat contradictions as blockers for that mapping and return them to the
   evidence phase.

## Stop conditions

- Do not name an aid from cost, faction, timing, marker duration, or visual effect
  alone.
- Do not force a nearest-event join when more than one complete assignment exists.
- Do not treat a request, feedback record, delayed spawn, or projectile as proof of
  the issuing player unless a reproducible identifier bridge is established.
- Do not edit binaries, replay evidence, or `.sdf` archives.
- Do not modify the replay viewer until the relevant mappings and derived-event rule
  pass their parser/corpus gates.
