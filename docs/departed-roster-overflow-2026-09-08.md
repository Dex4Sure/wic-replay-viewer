# Departed players and eight-player result teams

Current viewer policy: [replay end-screen results](final-screen-results-2026-09-09.md)
now take precedence when complete. The corrections below remain the fallback;
their corpus comparisons describe the earlier presentation.

The parser preserves every result row and its statistics. Each player now includes
nullable `leftAtSeconds`, on the recording clock. This is independent of
`scoreBeforeLeave`: a player can leave with a valid final score.

A marker requires a primary gameplay clock and result, exactly one gameplay
session matching the result-row name, an explicit matching named entry, and an
explicit leave ending that session before the result. Reconnect ambiguity,
unknown gameplay occupants, implicit replacement boundaries, missing entry/leave
messages, and post-result leaves do not produce markers. A recording may start
late if it still contains the required named-entry and departure evidence.

The viewer applies one shared rule to fresh library summaries and both typed and
cached Overview details. After existing abandoned-duplicate suppression, count
rows per playing faction. Only when the count exceeds eight, omit confirmed
departed rows in departure order until eight remain. Equal timestamps use slot ID
as a stable tie-break. Teams of eight or fewer retain all rows, including departed
players. Spectator and unknown-team rows are unaffected. If evidence is
insufficient, the viewer keeps the unexplained rows rather than guessing.

The filtered rows drive Overview statistics, team totals, library names/counts,
and matchup labels. Raw cached parser JSON and historical playback sessions/events
retain the omitted participants. No new departure UI or spectator reassignment is
introduced. Cache `detail-v44` refreshes existing results; product 0.4.0, timeline
18, and database 12 remain unchanged. Missing departure fields in older documents
default to null.

## demo58 evidence

Source SHA-256:
`7ef6f598e6229210d958387435fa25b8f6501102bdaccae8c10efd59e3986dae`.
This is the demo58 from the recovered game's Documents replay folder, not either
of the other distinct demo58 recordings in the library.

Its historical result roster contains seven NATO and nine USSR participants.
SkorpioPlayer (slot 8) explicitly leaves at recording time 247.43896484375 seconds;
kucsapapa joins USSR afterward. The parser retains SkorpioPlayer and his 115 points,
adding the departure marker. The viewer omits only that overflowing row and displays
7v8. This fixes a pre-existing cumulative-participant count, not a team-attribution
regression introduced by the explicit score fix.

## Verification

The parent workspace's `compare-departure-rosters-2026-09-08.rs` runs the current
parser and the production viewer overflow helper inside Flatpak.
`check-departure-rosters-2026-09-08.py` compares it with the full `detail-v43`
baseline, asserting unchanged raw values apart from the new marker, unchanged
historical evidence, unchanged retained rows, and removals only from overflowing
teams with explicit departures. `verify-departure-rosters-2026-09-08.py` checks
changed recordings independently with the Python reference parser and input hashes.

Portable tests cover explicit departures, reconnects, reused slots, missing and
post-result events, late recordings, small teams, multiple departures, stable ties,
insufficient evidence, and cached projection. A private demo58 check compares fresh
typed and JSON detail projections.

### Full-library results

The optimized Flatpak run covered 4,089 paths / 2,499 distinct contents against
the pre-departure `detail-v43` baseline. Acceptance remains 4,074 paths / 2,491
contents accepted, with 15 paths / 8 contents rejected.

The parser adds 1,546 non-null departure markers across distinct contents, with no
changes to existing raw fields. The viewer omits 17 departed rows in 15 distinct
replays / 24 file copies. All displayed playing teams in this corpus now contain
at most eight rows. Teams already at or below eight retain all players; no
retained row changes its name, role, score, or category statistics. Historical
session/event evidence and existing departure-score evidence are unchanged.

The full product gate passed, including coverage floors; all 17 saved private
ground-truth fixtures passed without skips. Demo58's fresh typed detail and cached
JSON projection matched. Independent Python verification checks every row's
identity, team, role, score, and departure time in all 15 affected contents.

The tested build was installed locally as Flatpak deployment `74c172d857a893d88bf79a0874840e7d8668ab8fee8035830f6ca5ce87b6cded`.
Its executable matches the built package and contains `detail-v44`. Reopen any
already-running viewer to use the new deployment. The source changes are recorded separately from the targeted result corrections.

## Unresolved installed-app freeze

During the subsequent library refresh, the user reported that the Flatpak frontend
froze. Read-only inspection found all 4,089 summary rows on `detail-v44`, and both
the native app and WebKit renderer remained alive. No crash dump or out-of-memory
record was found. A renderer stack sample showed its main event loop waiting in
poll; this does not identify the cause. The corpus checks above validate result
data, not completion of the visible scan UI. No freeze fix was made, and the app
was not restarted during diagnosis. Reproduce and investigate this separately
before claiming the installed scan is regression-free.

September 10 follow-up: [bulk import responsiveness](bulk-import-responsiveness.md)
records event batching, recovery, and yielding frontend preparation added after this
investigation. The original installed-Flatpak freeze is still awaiting a runtime
retest; the historical observations above remain unchanged.
