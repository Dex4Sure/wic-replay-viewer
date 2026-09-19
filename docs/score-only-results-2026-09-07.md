# Score-only result recovery — 2026-09-07

Current follow-up: [targeted event results](targeted-event-results-2026-09-08.md)
extends score/name/spectator evidence and advances the cache to `detail-v43`.
The earlier measurements below retain their original baseline.

This report records the `detail-v41` role-recovery withdrawal. The subsequent
[signed-score fix](signed-scores-2026-09-07.md) advances the cache to `detail-v42`
and corrects pre-existing unsigned score values.

Roles retain the pre-existing parser calculation. The experimental zero-score
summary/selection fallback (including HOTWINGS) and last-selected-role fallback
for departed players are removed. A selected role is not the score-based primary
role shown elsewhere in Overview. The proposed initial-entry-role extension was
not implemented.

The separate departure-score recovery remains unchanged, including its session,
identity, unit ownership, explicit departure, and zero-reset guards. Recovered
scores retain their asterisks and hover explanation. Unavailable category statistics
stay null; existing roles, teams, identities, and raw parser documents are preserved.
Role events still belong in the playback timeline; they are not Overview roles.

`PlayerResultEvidence` no longer has a role field; `ScoreBeforeLeave` no longer
contains a last role, and the `RecordedRole` type is removed. Both summary and
detail projection apply scores only. Legacy role evidence is ignored on decode.
Cache `detail-v41` refreshed the installed library after withdrawing these fields.
No product, timeline, raw parser JSON, or database version change is required.

Regression controls preserve every original role in all seven private review
fixtures, including HOTWINGS remaining without a score-derived role. Legacy cache
controls supply the old role metadata and verify it cannot restore either fallback.
Synthetic score/identity controls and the score asterisk/hover checks remain.

## Corpus comparison

The complete 4,089-path optimized native comparison against `70b1681` preserves
all raw parser results and duplicate-suppression decisions: 4,074 accepted and the
same 15 rejected. All 37,193 raw rows, 37,175 visible rows, and 30,961 original
positive-score rows remain unchanged. Score evidence also matches the prior v39
score audit exactly: **514 recovered departure scores** across **404 paths**
(**245 distinct replay contents**). The role-only corrections are gone; no role
observations are emitted. Artifact: `/tmp/score-only-audit.json`.

## Validation

`./scripts/quality.sh private` passed, including the portable suite, frontend/Rust
coverage gates, Clippy, all configured private/game checks, and **17/17 strict
ground-truth fixtures with zero skipped**. The seven private review fixtures keep
every role identical to the raw parser result while preserving recovered scores.
The legacy-cache regression proves both removed role fields are ignored.
Log: `/tmp/remove-role-quality.log`.

The Flatpak importer refreshed all 4,089 library paths with cache `detail-v41`:
4,074 imported, the same 15 rejected, and no removed paths. Independent read-only
comparison found zero summary/evidence differences against the score-only audit.
All seven selected fresh/cache details agree and preserve raw player rows.

The installed Flatpak deployment is
`3e25631c0df3bb694889e8ccadfa1106744b1e2bd21f9659ae48b8263390cf03`.
Its projected `333__demo17` and `358__demo85` details retain all original roles;
HOTWINGS and the previously recovered last-role players now have no role icon.
A rendered browser check of the actual `333__demo17` projection confirmed
HOTWINGS at zero without Air, and the 150*/408* departure scores still present.
This was a browser preview, not a screenshot of the native app window.
Temporary preview sources were removed. Logs/results:
`/tmp/score-only-flatpak-scan.log`, `/tmp/score-only-flatpak-verification.json`.
