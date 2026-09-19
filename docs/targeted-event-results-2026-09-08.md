# Targeted event-backed result corrections — 2026-09-08

Current viewer policy: [replay end-screen results](final-screen-results-2026-09-09.md)
now take precedence when complete. The corrections below remain the fallback;
their corpus comparisons describe the earlier presentation.

Follow-up: [departed-player overflow handling](departed-roster-overflow-2026-09-08.md)
adds a raw departure marker and advances the cache to `detail-v44`. The v43
comparison below remains historical.

## Scope

This extends the established result parser rather than replacing Overview with
an end-of-replay roster. Departed participants remain eligible for Overview,
unknown teams still display as spectators, and roles remain derived from result
scores. Product 0.4.0, timeline 18, and database 12 are unchanged. Cache
`detail-v43` refreshes stored library summaries and details.

Baseline: viewer `1942692544bc84dc4b563f3dbba0fcb9b8cb5dc2`. The parent workspace
holds the comparison tooling and ignored, SHA-256-keyed corpus evidence in
`findings/*roster*2026-09-08*`.

## Score attribution

The old extractor attributed a score using a counter in the following record.
That misses final slot-15 updates and misattributes messages when slots are not
written in the usual sixteen-slot sequence. Complete primary-chain `SetScore`
envelopes instead supply unsigned `aPos` and signed `aPlayerScore` together.
The latest observation before `TeamWins` is the result; negative scores and zero
resets are preserved, and subsequent lobby activity cannot overwrite it.

The primary chain must reach `TeamWins`. Seven corpus recordings have an
incomplete primary chain but readable final summaries. An initial trial exposed
lower partial totals in those files; the final implementation preserves their
established summary extraction unchanged. This is a structural fallback, not a
filename exception. Missing usable score events also retain that fallback.

In `Replay Old/demo96.wicdemo`, Spirit_Gun-77's final message explicitly gives
slot 15 a score of **810**, correcting **809**. In three bot recordings, reading
actual slots also removes nine synthetic `Player N` rows previously created by
misattributed counters. No named player is removed.

## Stale lobby identities

Require a captured start, a primary result, exactly one gameplay-overlapping
identity session for the slot, an explicit named entry, and owned units entirely
inside that session on the existing result team. Earlier nonzero scores or
conflicting gameplay teams block the correction. Pregame team changes are reduced
to the last pregame join; all subsequent joins must agree. A replacement can have
entered before gameplay or later into a slot that was vacant at match start.

These guards resolve all eleven reviewed stale names. No result score or role
is transferred between two gameplay occupants. Seven corrected identities also
qualify for the existing departure-score evidence query; their recovered scores
use the existing asterisk and hover, with unavailable category totals left null.
In `799__demo03`, slot 2 becomes Real-Escobar with **46\***; slot 4 remains
Weed Queen with **−11**, unchanged.

## Spectator membership

`SpectatorJoinedTeam` describes spectator status even when the player observes
one faction. Accept the already decoded one-team and all-team viewing modes;
the observed faction is not playing membership. Existing zero-stat, captured-start,
identity, unit, gameplay-role, and result-boundary guards remain.

A further explicit-session exception permits an old lobby role to be superseded
by a later spectator event when the slot has exactly one gameplay session, a
matching named entry, no result role, no nonzero stream score, no owned units,
and no gameplay role selection. Lobby roles in that session must be superseded.
This handles the three reviewed inactive sessions previously blocked by lobby
roles. Leaving alone never triggers this correction.

All twelve reviewed spectator candidates are corrected. The explicit inactive-session
rule additionally corrects Nelogs in `demo32` and King1991 in `4v4 MM Seaside 2010
#9 Yet Another Unfair Stack`. Their current sessions already began with team zero,
so the comparison probe's segment coalescing retained an `entry` source and did
not classify their later spectator confirmations as candidates. This limitation
was in the research candidate detector, not a new product heuristic.

The Python reference now accepts nonbreaking spaces inside metadata and bounded player-entry
names, matching Rust; this resolves the independent decoder discrepancy for
sheepy. Other invalid/control-containing strings remain rejected.

## Corpus comparison

The optimized native probe ran inside Flatpak across **4,089 paths / 2,499 distinct
contents**. Acceptance stays **4,074 paths / 2,491 contents accepted** and
**15 paths / 8 contents rejected**. All original 11 name, 12 spectator, and 106
score candidates resolve.

Final changes affect **138 distinct contents / 207 file copies**:

- Eleven named rows corrected; seven gain existing departure-score recovery.
- Fourteen zero-stat rows moved to Spectators, displayed by name only.
- 120 existing raw score values corrected: the reported 106 plus twelve further
  slot-15 updates and two bot-slot attribution corrections.
- Nine unnamed synthetic rows removed in three bot recordings. Their false
  unknown-scored rows no longer mark those results incomplete.
- No named rows added or removed; no retained player's role changes. Winners,
  timing, domination values, recorder identity, and historical event data remain
  unchanged. The seven incomplete-chain recordings retain their baseline results.

The known in-game-broken `CG's epic fail` / `333__demo17` remains unchanged. Parser
acceptance is not a claim that every accepted recording loads in the game.

Portable validation covers signed scores, zero resets, result boundaries,
incomplete chains, malformed score fields, explicit one-team spectators,
superseded lobby roles, sole occupants, shared slots, conflicting unit ownership,
missing activity, and late recordings. The full product gate passes, including
frontend tests/build, Rust tests/Clippy, and coverage floors.

The independent Python parser verified all 138 changed contents, checking every
result row's name, team, score, and role against Rust. Its five nonbreaking-space
metadata mismatches were corrected and rechecked; the final parity artifact has
zero differences. All 17 private saved ground-truth fixtures passed, with none
skipped. The complete full-library comparison is separate from the routine gate.

The corpus validation ran a separate optimized probe inside Flatpak without writing
to the library database. After validation, the user requested installation: the
latest worktree was packaged and installed as Flatpak deployment
`a9b52a85edbf8d81d74b948eea3e02589ee716e956b91441449b2f787f940ab9`.
The installed binary matches the built package and contains `detail-v43`. An
already-running viewer must be reopened to use this deployment. This records the initial installation before the follow-up departure change.
