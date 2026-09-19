# Lobby spectator and abandoned-duplicate refinements

Current follow-up: [targeted event results](targeted-event-results-2026-09-08.md)
extends score/name/spectator evidence and advances the cache to `detail-v43`.
The earlier measurements below retain their original baseline.

Baseline: viewer commit `bc3d308`. These are two separate, evidence-gated changes.
The product stays at `0.4.0`; timeline schema 18 and database schema 12 are
unchanged. This audit used viewer cache `detail-v37` for summaries and lazy details.
The [score-only recovery follow-up](score-only-results-2026-09-07.md) used
`detail-v41`; the subsequent [signed-score fix](signed-scores-2026-09-07.md) uses
`detail-v42`. The measurements below remain the historical v37 audit.

## Recorder in 730__demo95

Input: `replays/main/WicTracker/downloads/730__demo95.wicdemo`.
SHA-256: `04b34a91b7ffa57273e37924504c5d494199340e8dade4170a1967aaa55033b1`.
Times are recording seconds, including the lobby.

The recorder `[v]ÅÞÞL£`, slot 8, joins USSR at 2.117 and selects several roles
between 4.409 and 145.129. At 863.750, the primary-chain message at decompressed
offset 1,259,278 explicitly sets spectator view to `aTeam=0`, `aSpectatorLos=2`.
Gameplay starts at 869.391, about 5.6 seconds later. No later team join or role
selection occurs before the result at 2047.660. The slot owns no `UnitCreate`
events, and its final score and every end-summary score are zero. A single named
session spans gameplay.

The previous correction rejected all slots with any role-selection event,
including lobby selections. The refined Rust and Python correction permits those
selections only when the same session subsequently establishes all-team spectator
status before gameplay. A gameplay role selection, a gameplay team join, any
owned unit before the result, or an earlier occupant's lobby role selection blocks
this exception. Existing zero-stat, primary-chain, and identity checks remain.
The recorder's final team/faction becomes `0` / `Spectator`; all other result
fields and historical timeline events remain unchanged.

See the [exact spectator gates](../parser/SCHEMA.md#result-roster-spectator-correction).

## Hunter UK in NoW vs Shiny 3

Input: `replays/main/old/NoW vs Shiny 3.wicdemo`.
SHA-256: `921ced0bb9ea8d04371616ff20c7c4f18aebbd9c545a05600b9e0561ef02c18d`.

The primary envelope chain records:

- Slot 3: `Shiny^Hunter UK` enters at 0, joins USSR at 3.500, and leaves at 22.296.
  A further named entry occurs at 80.739, followed by departure at 80.940
  (message offset 44,886). There is no later occupation, role selection, or owned
  unit in slot 3. All result statistics are zero.
- Slot 8: the same exact name enters at 80.841 (message offset 44,718), joins USSR,
  selects roles, and remains through the match. Gameplay starts at 1025.071;
  this slot owns 87 unit-creation events and finishes with 1,024 points and Air
  as its primary role. `TeamWins` occurs at 2225.019.

The overlapping lobby entry messages do not justify merging arbitrary identities.
The narrower display rule requires an explicitly abandoned, inactive, zero-stat
slot plus exactly one matching named/team result with its own pre-game entry,
match-spanning session, positive score, known role, and gameplay unit ownership.

The opt-in Rust evidence query returns slot 3 for this replay. The viewer removes
that row from its library summary and Overview, retaining slot 8. It does not add
a second Hunter UK to Spectators. Raw parser JSON retains both slots, and the
historical timeline is untouched. Viewer-owned `hiddenPlayerIds` are stored
beside the cached `replay` and `timeline`, preserving the projection on reload.
Absent or ambiguous evidence preserves the original visible rows.

See the [duplicate gates](../parser/SCHEMA.md#viewer-only-abandoned-lobby-duplicates).

## Validation

- Rust and Python spectator regressions each cover 22 cases, including superseded
  lobby roles, a spectator switch only after gameplay starts, subsequent team
  joins, gameplay role selections, lobby units, and the original negative controls.
- The Rust abandoned-duplicate regression covers 19 cases: departure/entry evidence,
  role/unit activity, missing or positive scores, missing summary, slot reuse,
  different names/teams, missing active units, late recordings, early active-slot
  departure, ambiguous active counterparts, missing result, and unframed departure.
- Both actual replays pass the private viewer regression. Hunter UK's raw cached
  roster retains both entries, while fresh and cached detail projections agree
  and retain only the 1,024-point entry in Overview.
- The full portable, coverage, private/game, and strict ground-truth gate passes:
  **17/17 fixtures, zero skipped**. No ground-truth expectations changed.

### Full library comparison

Compared all 4,089 paths against a preserved optimized executable from `bc3d308`.
The direct CLI JSON comparison preserves exact serialized values, avoiding float
precision differences introduced by the temporary audit wrapper's JSON value
conversion.

- 4,074 accepted files and the same 15 rejected files.
- 46 additional spectator result corrections across 40 paths: 30 rows in 26
  distinct SHA-256 contents. Only `team` and `faction` change on zero-stat rows.
- 18 abandoned duplicates hidden across 18 paths: 10 rows in 10 distinct contents.
  These rows remain in parser output.
- Together, 58 paths / 36 distinct contents change. The remaining 4,016 accepted
  documents and their visible rosters are unchanged.
- All 37,193 raw rows, 37,190 known scores, and 30,961 positive scores are retained.
  Viewer roster rows become 37,175 after the 18 duplicate suppressions.
- Names, IDs, roles, every score field, and every non-player summary field match
  the baseline exactly. No positive-score row is hidden or reassigned.
- Independent primary-envelope event checks cover all 36 changed contents with
  zero issues. The Python reference agrees on all 30 distinct spectator corrections.

Local audit artifacts: `/tmp/lobby-refinements-audit.json`,
`/tmp/lobby-refinements-evidence.json`, and `/tmp/lobby-summary-verification.log`.
The comparison scripts are `/tmp/audit-lobby-refinements.py` followed by
`/tmp/verify-lobby-summary-json.py`; raw-event checks use
`/tmp/verify-lobby-evidence.py`.

The local Flatpak was rebuilt and installed with these runtime changes for review.

### Installed Flatpak scan

Installed local Flatpak commit:
`5e9473b2689dff0aa5cef6eef333f725e553cd0f2f2ea802b6c54fe81c45b880`.
The user completed **Scan folders** in this build. Read-only SQLite verification
found all 4,089 cached summaries on `detail-v37`, with 4,074 successful parses,
the same 15 rejects, and 37,175 displayed roster rows. Every accepted summary's
player count, player names, and recorder faction matches the audited output.
Both copies of NoW vs Shiny 3 contain Hunter UK once, and 730__demo95's recorder
faction is Spectator.

Local readback: `/tmp/lobby-flatpak-scan-verification.json`. No lazy detail had yet
been cached after this scan; the fresh/cache detail agreement above comes from
the private regression, not a claim of rendered UI inspection.
