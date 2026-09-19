# Workspace TODO

Open orchestration, tooling and research work for the unified replay repository.
Parser implementation items live in `parser/TODO.md`;
proxy items belong in the separate `wicgate` repository.

The parser roadmap includes attribution and server integration questions; consult
the component's TODO for its current scope. The envelope-timestamp question is
resolved: timeline schema v13 times every event from the `Event` envelope clock,
and replay length and match length are now separate fields.

## Pressing

### Finish manual tactical-aid playback review

Review representative player-POV, one-team spectator, all-team spectator, and
view-transition replays using the current viewer. The checklist was prepared
against `timeline-v13/detail-v14`; those are historical cache identifiers. Confirm
both recovered aid names and `unitSpawnOwnership` players, including exact and
intentionally unknown rows. Record `confirmed`, `unclear`, or `contradicted` in
`research/findings/tactical-aid-manual-review-2026-08-17.md`; any player contradiction blocks
the corresponding attribution rule. Automated preparation now verifies all 18 exact
marker rows and projects 47/111 viewer rows for the two review replays; the remaining
gate is collaborative in-game observation.

The reused-slot contradiction found during replay B playback is resolved: schema
v11 identifies `[WHO]LtDan73` as slot 1's later occupant instead of extending
`[-HH-]JonnySky` across the match. Remaining playback review concerns effect labels,
not static player-slot identity.

The seek-timing concern should be gone. Checklist times were countdown-derived,
which is not the axis the in-game player scrubs on; schema v13 puts them on
recording time and the checklist was migrated to it on 2026-08-22. Seek hints
should now land on the effect rather than near it. Confirm that during playback.

### Verify unattributed unit-destruction causes

Phases 1-10 have completed the census, exact-target, blast, projectile-impact,
disband, lifecycle-context, support-cloud, and forced-death screens documented in
`notes/unit-destruction-attribution-verification.md`. Schema v17 promotes only the
exact-target tactical-aid subset. Schema v18 added deterministic
`buildingCollapse` and `destroyedWithContainer` contexts and inherits 67 independently
exact tactical-aid causes through destroyed containers. Broad cloud overlap and all
tested proximity/timing rules remain rejected. Unknown causes must stay unknown.

Phase 8 closed the two bounded binary questions; see
`research/findings/unit-destruction-phase-8-forced-death-callers-2026-08-24.md`. All six
forced-death callers are classified and five cannot produce a multiplayer unit loss.
The self-deletion caller split is complete, and its branch ends in `UnitRemove`
rather than `UnitDestroy`. The terminal-direction signature means "fatal damage with
no direction vector" and is not exclusive to the forced and inherited death helpers.

Phase 9 closed the sentinel-plus-directional hypothesis by refuting it; see
`research/findings/unit-destruction-phase-9-death-explosion-blasts-2026-08-24.md`. Death
explosions provably emit sentinel-killer damage that carries a real direction
(`EXG_Death` `0x00513450` enqueues a blast; `EXG_BlastContainer` `0x004f2b50` applies
it with a literal `0x200` killer), so that class is not a tactical-aid signature and
must not be labelled as one. The class carries the predicted temporal signature with a
clean negative control, but `UnitDestroy` serializes no position, so no per-death
parent can be identified. This population stays unattributed.

Phase 10 rejected the `buildingDamage` mechanical context; see
`research/findings/unit-destruction-phase-10-building-damage-context-2026-08-24.md`. The splash
path `0x00517f10` passes a literal null killer object to `0x004ce830`, which therefore
selects sentinel `0x200` and a synthetic direction, so a genuine splash death must be
sentinel-synthetic. The same-raw-tick residency rule fires 2,476 times on classes that
provably cannot be splash against 1,569 where it could be right, so a majority of its
matches are false by construction. Rejected on determinism and on yield.

Remaining bounded work:

- [x] Publish schema-v18 child changes. Parser `60c1b78` and viewer `dc785b4` are
      pushed, the viewer's parser cache key matches, and the parent
      Gitlinks were advanced in `cb04e7d`.
- [x] Test whether every sentinel-killer death with a directional hit vector is
      support-caused. **Refuted in Phase 9.** The sentinel `512` is `EX_MAX_UNITS` and is
      the engine's own encoding for "no owning unit": `EXG_Unit::Kill` `0x004ccef0`
      asserts `aKillerUnit <= EX_MAX_UNITS` and gates scoring on `killerId < 0x200`.
      `EXG_BlastContainer` `0x004f2b50` passes that literal for death-explosion blasts
      while supplying a real position, so the class provably mixes at least one
      non-support mechanism. Do not promote a `tacticalAid` cause for it.
- [x] Evaluate a `buildingDamage` mechanical context. **Rejected in Phase 10.** The
      attacker is discarded deliberately at the splash call site, so the context could
      never have carried an actor, and the same-tick residency join is not deterministic:
      it fires more often on deaths that provably are not building splash than on ones
      that could be.
- [ ] If a new candidate bridge is found, measure known-killer and known-TA positive
      controls before screening unknowns. Deterministic evidence may enter the parser;
      high-confidence but imperfect evidence must be reported for an explicit product
      decision first.
- [ ] Consider controlled dedicated-server instrumentation only for prospective
      validation. It cannot reconstruct an internal damage-source pointer omitted from
      historical replay files, and attaching to a process still requires user approval.

Closed and not to be reopened without new evidence: the forced-death caller
classification, the caller-specific companion search for those six paths (only the
unserialized bridge kill box remains live), the blink-scheduler reason split, the
sentinel-plus-directional support hypothesis, and the `buildingDamage` context.

Optional, opportunistic:

- [ ] Multi-POV derived attribution layer. Aligning two or more recordings of the same
      match recovers the opposing team's exact tactical-aid players; the join is exact
      (bit-identical `(supportId, position, team)`), and the audit measured 45.9% -> 71.9%
      coverage with 826 cross-confirmations and 0 contradictions. See
      `research/findings/multi-pov-tactical-aid-attribution-2026-08-22.md` and
      `research/scripts/multi_pov_attribution_audit.py`.

  It is rare and cannot be a dependency: only 22 confirmed groups (21 pairs, one
  triple) exist across 2,880 replays, roughly 1.5% of files. It is worth building only
  as a strictly additive bonus for users who merge replay folders from several
  players. Scope limit: it recovers tactical-aid _deployment_ attribution only, and
  does nothing for the sentinel blast population closed in Phase 9.

  Design questions to settle before implementation, in this order:

  1. Provenance. A cross-POV attribution must be visibly marked and traceable to the
     source replay that carries it, never presented as if it came from the replay
     being viewed. Same principle as never inventing an actor.
  2. Layering. Grouping is a library property, not a replay property; it exists only
     once several files are imported. The summary layer should surface "this match has
     N points of view" and the detail layer should mark derived causes as a distinct
     class.
  3. Stability. Decide whether a derived attribution is cached with its source
     recorded, or recomputed and shown as conditional. A viewer whose causes change
     with import order is not trustworthy.

### Fedora AppImage build limitation

The historical Tauri `linuxdeploy` bundle embedded an old `strip` that rejected Fedora
44 RELR sections and its GTK plugin can copy the 32-bit
`/usr/lib/gio/modules/libgiognutls.so` into the 64-bit AppDir. The verified local
workaround uses `NO_STRIP=1` and restricts that plugin step to `/usr/lib64`, but it
was not encoded in the repository. Future Linux releases now use AppImage built
on Ubuntu 22.04, alongside the Windows portable ZIP. Direct Fedora bundling remains
unsupported; this investigation is relevant only if Fedora becomes a supported
build host. Running the Ubuntu-built AppImage on Fedora is supported and was
verified after the [startup packaging correction](../docs/appimage-fedora-startup.md).

## Maintenance

### Decide whether Windows code signing is worth pursuing

The portable executable is unsigned and Windows may display an unknown-publisher
warning. The viewer now has an MIT license in
`LICENSE`. If signing is pursued, verify current provider
eligibility, repository-visibility requirements, and pricing before choosing a
service; the earlier provider estimates are no longer maintained here.

### Completed: publish schema-v10 and the Linux viewer fix child-first

Parser `d51e3a0` and viewer `a7d64fe` were published before their parent Gitlinks.
The chain includes exact tactical-aid actor/target damage notifications, explicit
unknown unit-destruction causes, typed fortification losses, neutral command-point
wording, and the Fedora WebKitGTK white-window fallback. Future changes retain the
unified viewer (including parser) first, then parent publication order. The
separate parser commits above describe the pre-unification delivery.

### Retest native WebKitGTK DMA-BUF rendering after graphics-stack updates

The August 19 Fedora white-window investigation led to an X11 default with
shared-memory transport; see `notes/replay-viewer-webkit-white-window.md`. A
September 18 AppImage launch with native Wayland and shared-memory transport
rendered the library on the affected desktop, so Wayland is now the Linux default.
After a material WebKitGTK, GTK, Mutter, or Mesa update, retest with
`WEBKIT_DMABUF_RENDERER_FORCE_SHM=0`. Confirm the Tauri window, folder picker,
library, and replay detail views on both the 1.0x and 1.5x displays before
removing the shared-memory fallback. A browser-only Vite render is insufficient.

## Research

### Expand decoded definition coverage beyond tactical aids

The RYS/SDF v9/v10 container and its plain/zlib codecs are decoded by
`research/scripts/wic_sdf.py`. The authoritative support catalogue is no longer blocked:
all 200 catalogue names round-trip to replay IDs and 58 top-level faction aids are
named. Continue applying the extractor to the authoritative unit and map definition
files needed to resolve remaining unit-type and command-point hashes.

Do not broaden parser mappings until each decoded name hashes back to its replay ID
without collision and its source archive/hash is recorded.

### Align duplicate or alternate-POV recordings

The format question is settled and the method is validated; what remains is
implementing it as a derived layer. See
`research/findings/multi-pov-tactical-aid-attribution-2026-08-22.md` and
`research/scripts/multi_pov_attribution_audit.py`.

A deployment's serialized `(supportId, position, team)` triple is bit-identical
across recordings of the same match, so the join is exact rather than statistical
and needs no timestamp alignment. Player slot IDs also agree across recordings.
Across 22 confirmed multi-POV groups covering 4,222 deployments, attribution rose
from 45.9% to 71.9%, with 1,099 newly attributed, **0 ambiguous**, **0
contradictions**, and 826 independent cross-confirmations between the schema-v9
unit-drop bridge and serialized markers. One three-POV group reached complete
attribution for its spectator recording.

Remaining work:

- Build the merge as an explicitly derived layer above the raw parser. The raw
  contract must keep emitting player-null deployments; the derived layer imports a
  player only when exactly one is named across the group.
- Record the contributing replay SHA-256 hashes and the matched key as provenance
  on every imported attribution.
- Reject same-POV re-uploads before merging. The corpus has 369 of them, and they
  would otherwise read as independent confirmation of attributions they copy.
- Decide how the viewer presents an imported attribution versus one the replay
  established on its own.

Not recoverable this way: 1,185 of the audited deployments produced no marker in
any point of view, led by `Tankbuster`, `ClusterBomb`, and artillery. Marker
emission is partial even for the recorder's own faction, so this is a
serialization gap rather than a visibility gap. Those stay unknown.

The labelled attributions must not be turned into a single-replay heuristic. A
re-test over 10,812 player-bearing markers found tactical aid is not role-gated:
52 of 53 aid families are called by all four roles, guessing an aid's most common
role is 53.9% accurate, and in 39.0% of cases a teammate shares the actor's role,
so a perfect role oracle would still reach only about 33%. The labelled set is a
test harness for future exact bridges, not training data.

### Completed: Ghidra coverage of the replay writer

Done. Walking the callers of `0x009240a0` enumerated **178 call sites and 169
distinct messages with zero unresolved names**, with complete field lists. See
`research/findings/bintag-message-inventory-2026-08-22.md` and
`research/scripts/bintag_message_inventory.py`, which reads the PE directly and does not
open the Ghidra project.

The BinTag writer entry points are confirmed, and `0x00923db0` takes its base
name in EAX rather than on the stack:

| VA           | Role                                                              |
| ------------ | ----------------------------------------------------------------- |
| `0x00989e40` | write a scalar field — cdecl `(name, typeCode, &value, size)`     |
| `0x00923db0` | write a vector field — base name in EAX, formats `%s.x`/`.y`/`.z` |
| `0x009240a0` | begin a message on a writer                                       |
| `0x00923a70` | end the current message                                           |
| `0x00b80880` | acquire the event writer for a channel                            |

Outcomes:

- **The tactical-aid actor question is closed.** Only three of 169 messages carry
  both a player and a tactical-aid identity, and the parser already consumes all
  three. Every global effect stream, including the four support-projectile
  creators, is player-free at the source.
- **Messages are rate-limited before writing.** `0x009240a0` suppresses a message
  when `now < lastEmit + interval` for its name hash, and every field write is
  guarded by that flag. A throttled event is absent from _every_ recording, which
  bounds what multi-POV alignment can ever recover.
- **The envelope timestamp is elapsed time from a writer origin**, confirming
  timeline schema v13 from the binary side.
- The countdown message is `SetGameModeData_Float`, with an underscore;
  `UpdateBalanceFactor` names the domination parent event; `CameraPosition` opens
  the end-of-match summary pass. All 19 parser message-hash constants resolve to a
  real message with no mismatch.

Possible follow-up: the parser consumes 19 of 169 messages. `SpawnerDeployed` and
`SpawnerSetPosition` carry `aPlayerID`, and `DeployableCreate` and `UnitSetOwner`
carry `playerId`; these may support non-TA attribution work.

## Deliberately not doing

- **Naming tactical aids from their cost.** Cost varies by role and by game mode,
  and unrelated aids share values. Cost ladders may _suggest_ an identity for a
  human to confirm, but the parser must not encode that mapping.
- **Treating the recorder purchase ledger as match-wide.** `SupportThingUsed` and
  its paired honors cost remain recorder-only. Separate delayed-spawn records do
  expose both factions' deployments, while markers and the validated unit-drop
  bridge identify some players; they do not create a complete match-wide purchase
  or cost ledger.
- **Reconstructing TA bundles in the raw parser.** The selected bundle size is not
  serialized, queued strikes have no time limit, and a recording may begin after an
  opening placement. The raw contract therefore emits placements only; any future
  grouping belongs in an explicitly derived analysis layer. The timeline has
  emitted placements rather than a selected bundle size since schema v9.
