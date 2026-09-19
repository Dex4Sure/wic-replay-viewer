# Event-backed result recovery — 2026-09-07

Current follow-up: [targeted event results](targeted-event-results-2026-09-08.md)
extends score/name/spectator evidence and advances the cache to `detail-v43`.
The earlier measurements below retain their original baseline.

**Historical v40 investigation.** The role recoveries described here were later
withdrawn in `detail-v41`; see [score-only results](score-only-results-2026-09-07.md).
Current behavior retains only departure-score recovery and uses `detail-v42` after
[correcting signed scores](signed-scores-2026-09-07.md). Measurements
below document the earlier experiment, not the current role presentation.

Baseline: `70b1681`. This is an opt-in viewer projection; the canonical Rust/Python
result JSON remains unchanged. Existing team, identity, primary-role, positive-score,
and abandoned-duplicate behavior stays authoritative. The evidence query only
fills specific zero-stat gaps proven by the same named gameplay session.

## Corrected classification decision

The uncommitted `detail-v38` experiment classified named pre-game departures as
Spectators. That was withdrawn: a departure proves absence, not a spectator
selection. Its query, viewer cache metadata, and grouping tests were removed.
Smallisland in settings/demo34 and Gwynbleidd in settings/demo30 retain their
prior team listings. Sgt. Jonny in 358__demo85 also keeps USA: he chose Support
then left before gameplay, without an explicit spectator switch. Existing fixes
based on actual spectator messages and proven duplicate suppression remain.

## 333__demo17: role and departed scores

Input: `replays/main/WicTracker/downloads/333__demo17.wicdemo`.
SHA-256: `972959c61587f57dd9cce5d7116c4f0e6ac1ed3c3b65fc55ece9b92860ca0447`.
The same replay content also occurs under other names, including CG's epic fail.
Times below are recording seconds; offsets identify decompressed message starts.
Gameplay starts at 379.686 (38,208); the result is at 1316.720 (20,948,232).

| Player                          | Evidence                                                                                                                                                                                                                          | Viewer result                                                                      |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Slot 3, COOL GUY                | 28 owned USSR unit creates; last score 150 at 1232.150 (20,019,769), explicit leave at 1233.447 (20,048,341), followed by zero broadcasts                                                                                         | USSR, 150 marked as score before leaving; role and category statistics unavailable |
| Slot 15, PukinDog               | 146 owned USSR unit creates; last score 408 at 1246.924 (20,298,112), leave at 1247.324 (20,303,586), then zero broadcasts                                                                                                        | USSR, 408 marked as score before leaving; role and category statistics unavailable |
| Slot 6, recorder Xtrme^HOTWINGS | All-team spectator in lobby; joins USSR at 1265.535 (20,609,807), selects Air at 1266.941 (20,620,317), creates four USSR units at 1293.647, remains through result; final summary at 20,946,682 also records Air ID `0x0ae8026e` | USSR, Air, zero score                                                              |

The legacy role fallback requires positive score; HOTWINGS is a real zero-score
participant, established by explicit selection and owned gameplay units. The new
role evidence requires agreement with the final summary and preserves ordinary
primary roles. COOL GUY and PukinDog have no explicit recoverable role selections
in their sessions. Their participation and scores are known; their roles are not
invented from unit types or another occupant's data.

Delta Shorty explicitly spectates and leaves; a different named spectator later
occupies that slot. Both timeline sessions remain intact. The summary continues
using its existing one-row-per-slot roster rather than expanding identities.

## Other reviewed replays

| Input                            | SHA-256                                                            | Result                                                                                          |
| -------------------------------- | ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| settings/demo69                  | `dde531f58b2d7a047f34e6aa454fd249074d8e83c6e3f3c42d59b55e1234b51c` | Donald Duck: 457 before departure, USA retained                                                 |
| settings/demo66                  | `20346131b6fb3fabe7cdf1d00aa65557e4e43fef621a737bbae37df9e88f7312` | He_y6uBauTe: 180 before departure, NATO retained; later spectator occupant does not relabel him |
| settings/demo33                  | `309fbfac35edafe60b290ed7e270df3b44cd86c565b70205406a8f8898f731f3` | Cmdr Trigger: 215 before departure, NATO retained                                               |
| settings/demo34                  | `51a35d0714b482ad2d95c4aa511e773cef9b919c786b5670832245637ace750a` | No new recovery or spectator grouping                                                           |
| settings/demo30                  | `06398cfee9a25bbd7daeb2d7ced2bbfebe58a233eddae574c00a2740c117db2f` | No new recovery or spectator grouping                                                           |
| WicTracker/downloads/358__demo85 | `bea3f6c6e3b697b6adf1d41b2851cb80173290646efdb1c34a1ad626cf8b37a4` | i.s.a. 358, HUSSAR001 167, Neuro 265 before their departures; all teams preserved               |

In 358__demo85, Tor Stenvik deploys four units but never has a positive score;
Levicius_BR joins USA and selects Support during gameplay without observed units.
Their zero scores remain. Jonny's departure does not make him a spectator.

## Final behavior and implementation boundaries

The Rust `player_result_evidence` query returns separate evidence without changing
`parse()`, CLI/WASM result JSON, or the Python reference parser. It uses validated
primary-chain envelopes and byte offsets to scope activity to a named session;
it does not copy the playback scoreboard's time-based projection into Overview.
The [contract](../parser/SCHEMA.md#opt-in-event-backed-result-evidence) defines the
entry, unit ownership, role agreement, departure/reset, and ambiguity guards.

The viewer persists `playerResultEvidence` beside the unchanged parser document.
Fresh and cached details apply the same projection. A recovered score carries its
observation and departure timestamps through `scoreBeforeLeave`. Overview keeps
an asterisk beside that score and explains it on hover. A proven last role uses
the normal role icon with the tooltip "Last recorded role before leaving". There
is no repeated explanation below the cards. Neither marker nor last-role icon
claims complete final statistics.

Per-role, category, and total summary fields become null for recovered departure
scores, because the end-summary zeros no longer describe that player's earlier
activity. Category leaderboards still require recorded category values; team
roster totals sum displayed scores. Library matchup calculation uses the same
recovered scores. Ordinary positive results, roles, names, and teams remain
unchanged. Departure alone never establishes spectator status.

Cache `detail-v40` refreshes summaries and details, superseding the intermediate
v38/v39 experiments. Product version 0.4.0, timeline schema 18, database schema 12,
and the raw parser contracts remain unchanged.

## Last recorded role evidence

A recovered departure score can carry `lastRecordedRole` from the latest explicit
selection after the same player's named entry and before departure. Lobby
selections qualify; later switches replace earlier selections. Unknown/FPM or
malformed latest selections leave the role unknown instead of reviving an older
selection. Existing score-derived primary roles remain authoritative.

- `358__demo85`: i.s.a. last selected Infantry at 11.032 seconds, HUSSAR001 Armor
  at 13.048, and Neuro Support at 7.109.
- `demo69`: Donald Duck last selected Support at 21.026 seconds.
- COOL GUY and PukinDog (`333__demo17`), He_y6uBauTe (`demo66`), and Cmdr Trigger
  (`demo33`) remain without roles: their verified sessions have no recorded role
  selection. HOTWINGS' Air recovery remains based on selection/summary agreement.

## Regression coverage

- Synthetic Rust cases cover declining scores (last observation, not maximum),
  zero/negative last scores, absent or post-result resets, scores after departure,
  missing/implicit/unframed departure, reconnects, spectator and playing slot
  replacements, unknown occupants, missing/contradictory units, entry/start/result
  gaps, positive or incomplete final statistics, and malformed field flags.
- Role controls cover summary agreement, role switches, missing/late selections,
  selections before named entry, unknown/FPM IDs, malformed/truncated latest
  selections, and post-result events.
- Portable cache tests verify provenance, unavailable categories, unchanged teams,
  legacy documents without evidence, and role-only recovery at zero score.
- All seven private review fixtures verify expected corrections, unchanged embedded
  result/timeline JSON, stable names/teams/row counts, and equal fresh/cached views.
  The component regression retains the asterisk and hover text without a footer.

Run `./scripts/quality.sh private` from the viewer repository for the full portable,
coverage, Clippy, private/game, and strict ground-truth gates. Private fixtures are
configured in `tests/private-fixtures.json`; no ground-truth expectations changed.
All **17/17 ground-truth fixtures passed, with zero skipped**. The final visual
refinement also passed all 16 component tests and the fast gate.

## Full corpus comparison

The completed comparison covered all **4,089 paths**, using optimized native
executables rather than a development server. Baseline and current wrappers use
the same serialization. **4,074 accepted and the same 15 rejected**, with no raw
result or duplicate-suppression differences against `70b1681`.

| Measurement                                       | Replay paths, including copies | Deduplicated replay contents  |
| ------------------------------------------------- | ------------------------------ | ----------------------------- |
| Missing roles recovered through summary agreement | 10 player rows                 | 5 player rows                 |
| Scores recovered before departure                 | 514 player rows                | 309 player rows               |
| Last recorded roles accompanying departure scores | 100 player rows                | 63 player rows                |
| Replays with any recovered evidence               | 409 paths                      | 248 distinct SHA-256 contents |

All **37,193 raw rows**, **37,175 visible rows**, and **30,961 original positive
scores** remain. Recovered scores are additions to zero-stat gaps, not replacements
for existing positive results. The five distinct summary-agreement corrections
cover HOTWINGS' replay, demo93, two players in demo49, and 505__demo522.

An independent Python primary-envelope scan verified all **248 affected contents
with zero issues**: named sessions, owned units/teams, explicit departures, exact
last signed score samples, post-departure resets, role/summary agreement, and the
latest role IDs/timestamps after named entry. The final named-entry guard was also
checked across all 409 evidence-bearing paths, preserving the counts above.

## Flatpak and rendered verification

The app's native importer ran inside the installed Flatpak against all existing
library locations, after requiring all 4,089 paths to remain accessible. Its v40
scan imported **4,074**, rejected the same **15**, and removed no paths. A repeated
cache-only scan found all 4,089 summaries current. No helper was bundled with the
application.

Read-only SQLite comparison checked every accepted summary's player count/names,
factions, recorder faction, and matchup against the audit, with zero differences.
All summaries use `detail-v40` and retain 37,175 visible roster rows. The seven
review details have matching fresh/cached projections, unchanged raw player arrays,
and evidence matching the independent scan. Withdrawn `lobbyOnlyPlayerIds` metadata
is absent from those details.

The final UI was installed and reopened at Flatpak deployment
`4ddb8950482c5729e1869d59aaca5e7bc5df11cb02348bbc34349c071df63d13`.
A rendered browser check of the production Overview using its actual Flatpak
projection for `358__demo85` showed Infantry/358*, Support/265*, and Armor/167*
for i.s.a., Neuro, and HUSSAR001 respectively, with hover explanations and no
footer. Ordinary roles and category leaders retain their prior presentation.
This was a browser check, not a native-window screenshot; preview files were removed.

## Local audit artifacts

These are session-local derived files, not shipped fixtures or portable paths:

- Corpus comparison: `/tmp/last-role-audit.json`, `/tmp/last-role-refresh-audit.log`;
  baseline `/tmp/lobby-refinements-audit-wrapper.json`.
- Independent scan: `/tmp/verify-last-role.py`, `/tmp/last-role-proof.json`.
- Installed-library comparison: `/tmp/verify-last-role-flatpak.py`,
  `/tmp/last-role-flatpak-verification.json`.
- Flatpak import/detail checks: `/tmp/last-role-flatpak-scan.log`,
  `/tmp/last-role-flatpak-detail-check.log`.
- Quality and UI checks: `/tmp/last-role-final-quality.log`,
  `/tmp/score-asterisk-test.log`, `/tmp/score-asterisk-fast.log`.

The replay paths and input SHA-256 hashes above, the parser contract, and the
tracked private/synthetic regressions provide the durable reproduction references.

## Pre-commit recheck

The final combined changes passed `./scripts/quality.sh private` again, including
all portable/coverage/Clippy checks, configured private/game regressions, and
17/17 ground-truth fixtures with zero skipped. The installed library was compared
again against the completed corpus audit with zero differences. All six runtime
source files involved match the validated Flatpak build inputs byte-for-byte.
The parser source SHA-256 is
`b59e41c25b7dcd08b49d082e71e665442e274c9a9845a72b24a1998d7eef331f`.
This recheck reused the completed corpus parse and independent envelope audit;
it did not repeat the full corpus parsing. Logs: `/tmp/result-commits-quality.log`
and `/tmp/result-commits-library-check.log`.
