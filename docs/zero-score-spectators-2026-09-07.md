# Zero-score spectator results: demo263

Current follow-up: [targeted event results](targeted-event-results-2026-09-08.md)
extends score/name/spectator evidence and advances the cache to `detail-v43`.
The earlier measurements below retain their original baseline.

## Evidence

Input: `replays/main/old/demo263.wicdemo` (private, immutable).
SHA-256: `96a4dec09c046aefd3c7fe2a9bc077f1e9c214c5bde79c3a1afe6b2b899b225b`.
Baseline: viewer commit `7c0bb87`.
All times below are recording seconds, including the pre-match lobby.

- Gameplay begins at 105.338; `TeamWins` occurs at 1305.692.
- Default, slot 1, begins in all-team spectator view and leaves at approximately
  202.448 (end of the leave envelope). The later entrant in that slot is
  ReichSSkorps[RHEIN], first entering at 229.942. The USSR joins at 255.427 and
  290.147 belong to this later occupant, not Default.
- JMann9, slot 7, begins in all-team spectator view, joins USSR at 273.034,
  spectates at 280.591, joins USSR at 292.467, and explicitly returns to all-team
  spectator view at 332.961. There is no subsequent playing-team join before result.
- The raw spectator records at decompressed offsets 42,574 (Default) and 1,665,445
  (JMann9's last switch) contain `aTeam=0`, `aSpectatorLos=2`.
- Both slots have zero final score and zero end-summary role/category totals.
  Neither has a recorded `PlayerSetRole` or owned `UnitCreate` in this recording.
  Nevertheless, their end-summary `aRoleId` values are Armor (slot 1) and Air
  (slot 7), demonstrating why a summary role ID alone is not a participation test.
- The competing result rows remain Kruelgor, USSR, Armor, 2968 points, and
  kickapoo149, USA, Infantry, 2640 points. All eight result names are preserved.

The legacy resolver keeps nonzero team joins in preference to spectator events.
It also resolves those events by slot across occupant changes. This explains the
incorrect USSR result rows in both Rust and Python; it does not establish which
internal game bug caused the user's observed in-game results screen.

## Targeted correction

Apply only the [documented spectator gates](../parser/SCHEMA.md#result-roster-spectator-correction)
to final roster fields. Preserve the general team resolver and timeline events.
The viewer lists the corrected rows under Spectators, with no score or role.
The playback scoreboard continues following actual team events at playback time;
the brief USSR joins remain real recorded history.

Unknown teams retain their existing parser representation and remain grouped under
Spectators in the viewer. No AFK label or additional team is introduced. Detail
cache `detail-v36` accompanied this original correction. The later
[lobby refinements](lobby-roster-refinements-2026-09-07.md) used `detail-v37` and
allowed specifically superseded lobby role selections. The subsequent
[score-only result recovery](score-only-results-2026-09-07.md) used
`detail-v41`; the latest [signed-score fix](signed-scores-2026-09-07.md) uses
`detail-v42`. Product version stays `0.4.0`.

## Unverified hypothesis: unknown may include unselected players

The user reports that the game offers USA/NATO (map-dependent), USSR, and Spectator
as team choices, with no displayed Unknown class. Their hypothesis is that some
unknown entries represent connected players who have not yet selected a team,
often because they are AFK.

This is plausible but not established by demo263: its two problematic rows have
explicit spectator history, and JMann9 actively sends chat messages. The parser's
unknown value currently also covers missing or unresolved team evidence. Lack of
a known team cannot distinguish unselected status, incomplete recording, and
parser limitations, and being unselected does not itself prove absence from the
keyboard. Keep the hypothesis separate from serialized facts. A future test would
need controlled recordings of an unselected client, a selected spectator, and a
playing client, ideally with the corresponding game-state/serialization evidence.

## Validation

This section records the original demo263 correction against `7c0bb87`. The
[later refinement report](lobby-roster-refinements-2026-09-07.md#validation)
contains the current validation against `bc3d308`, including the additional
spectator corrections and viewer-only duplicate suppressions.

Synthetic Rust and Python regressions cover the correction, slot replacement,
nonzero scores/totals, role selection, owned units, a later playing-team join,
post-result spectating, absent spectator evidence/result/score/summary, late
recording, repeated same-name sessions, unframed records, and unknown teams.
The private viewer gate includes demo263 with its unchanged competitive scores,
roles, names, and eight-row roster.

The full portable gate, coverage, and private/game regressions passed, including
new demo263 assertions. Ground truth initially flagged two corrected spectator
expectations in AIR VS ESL; after the raw-record review below, the strict runner
passes 17/17 with zero skipped fixtures. Final static checks also pass.

### Full library comparison

The optimized native CLI compared all 4,089 library paths with a preserved
pre-change executable. Input hashes and complete before/after summaries are in
local `/tmp/zero-score-spectator-audit.json`; the reproducible comparison script is
`/tmp/audit-zero-score-spectators.py`.

- Both accept 4,074 and reject the same 15 with identical errors.
- Only `team` / `faction` change: 192 zero-score rows across 170 paths,
  representing 105 rows across 92 distinct replay contents.
- All 37,193 player rows preserve their names, IDs, scores, roles, and every other
  statistic. No added, removed, or duplicate rows.
- Known scores remain 37,190; positive scores remain 30,961; positive scores on
  known playing teams remain 30,897. All non-player summary fields are identical.
- The other 3,904 accepted replay summaries are entirely unchanged.
- Independent Python parsing agrees on all 104 corrected rows present in both
  parsers across those 92 distinct contents. One existing reference-parser
  limitation remains: slot 12 in `1043__demo06.wicdemo` has an embedded NBSP in
  `ƒail^ Vikoz`. Python's printable-string check omits this unscored row both
  before and after the patch, while Rust preserves it. The replay explicitly
  records its all-team spectator state at decompressed offset 44,976; this is not
  a full-roster parity claim. See `/tmp/zero-score-spectator-parity.json`.

### Installed Flatpak scan

The Flatpak built for this original correction was installed and opened. Its
completed scan stored all 4,089
library entries under `detail-v36`: 4,074 successes, the same 15 failures, and
37,193 player rows. All 170 changed-file stored roster names and row counts match
the native comparison. Summary storage does not contain individual player teams;
team corrections are verified by the parser comparison and private detail test.
The installed app also cached demo263 under the new key: Default and JMann9 have
`team=0`, while Kruelgor remains USSR/2968 and kickapoo149 USA/2640.
No development server was used for the corpus work.

Flatpak commit used for this historical scan:
`3ca7b55630a68411fb5b31e530b1aa91efcf08515081b7bd7194bee59370e791`.
Local scan readback: `/tmp/zero-score-spectator-flatpak-scan.json`.

### Ground-truth correction verified from raw records

`AIR VS ESL.wicdemo`, SHA-256
`08950fbf43209a795bef83b8cf01388d376c60b88750ebcafb5f63a9d239354d`,
previously expected two zero-stat players on playing teams. The primary envelope
chain proves both selected all-team spectator view before gameplay at 340.068:

- Slot 6, STEVEN2.1: USSR join at 13.7583, then spectator at 27.5327,
  decompressed spectator-message offset 42,969.
- Slot 10, Scarrow: NATO join at 8.8669, then spectator at 15.3567,
  decompressed spectator-message offset 41,166.

Both spectator messages carry `aTeam=0`, `aSpectatorLos=2`. Neither slot has a
role-selection or unit-creation event before `TeamWins` at 781.1606, and both have
zero final and end-summary scores. Only their two `team` and two `faction`
expectations are corrected; every other fixture expectation is preserved.
