# Targeted player-name, role, and spectator-display corrections

Current follow-up: [targeted event results](targeted-event-results-2026-09-08.md)
extends score/name/spectator evidence and advances the cache to `detail-v43`.
The earlier measurements below retain their original baseline.

Verified 2026-09-07 against the pre-change parser at
`b59654d38f34bd89db5a3c1943c1a8704448fe89`.

## Name correction

Preserve the existing match-start roster and score, team, recorder, and result
resolution. Correct only a stale lobby name on a positive-score slot that was vacant
at the first gameplay clock. Exactly one named session must enter before the first
recorded `TeamWins`, its recorded team joins must agree with the existing playing
team, and the end summary must already contain nonzero per-role scores. Ambiguous sessions,
missing roles/results, active match-start occupants, and conflicting teams retain
the original output. Slots still have one result row each.

The earlier experimental session expansion, final-occupant naming, team reassignment,
and event-label changes have been removed. The name correction does not infer a
role from scoring. The separate recorded-role fallback is documented below.

Separately, the frontend lists unknown-team players under Overview spectators,
showing names only and excluding their scores and roles from match leaders.
The Replay scoreboard allows only known playing factions. Unknown parser
values are retained. The final detail cache key for these changes was `detail-v35`, invalidating older
parser output, including the local experimental builds. Product, database, and
timeline schema versions are unchanged.

The subsequent [zero-score spectator correction](zero-score-spectators-2026-09-07.md)
advanced the cache to `detail-v36`. The later
[lobby spectator and duplicate refinements](lobby-roster-refinements-2026-09-07.md)
used `detail-v37`; the [event-backed result follow-up](score-only-results-2026-09-07.md)
used `detail-v41`. The subsequent [signed-score fix](signed-scores-2026-09-07.md)
uses `detail-v42` and documents the latest full-library comparison. It retains
negative-score participation and negative role activity for name correction.
The comparisons below describe this earlier name/role/display change in isolation.

## Reproducer

`744__demo08.wicdemo`, SHA-256
`7e0b67507afdf001b9df502d83784edc8f0501859ad94c4e599bc3c6b918202f`:

- Slot 8's lobby occupant leaves at 7.966 seconds, before gameplay at 30.8302.
  Todesengel22 enters at 124.728 and joins USSR at 142.019. The result name now
  reflects Todesengel22; score 247, Support, and USSR are unchanged.
- Slot 11's result name becomes bTd^westurkey; score 673, Armor, and USSR are
  unchanged.

## Name-correction library comparison

The optimized native CLI parsed the same 4,089 paths with the original and patched
parser. Input SHA-256 hashes and both complete summary documents were retained in
local audit artifacts; no private replay files are checked in.

- 4,074 accepted and the same 15 rejected in both runs.
- 222 name corrections across 173 files, representing 117 distinct replay contents.
- The other 3,901 accepted files have identical complete summary documents.
- All 37,193 player rows retain the same slot, score, role, team, and every other
  non-name field. All non-player summary fields also match exactly.
- Known scores remain 37,190; positive scores remain 30,961; positive scores on
  known playing teams remain 30,897. No added, removed, or duplicate slot rows.
- The independent Python reference agrees with Rust on names, teams, scores, and
  roles for all 117 distinct corrected replay contents.

These are summary-output comparisons, not a claim that every historical timeline
name equals its slot's final result name.

## Regression gates and aggregate baseline

The name-only patch passed `./scripts/quality.sh private`, including portable
checks, coverage, private and installed-game regressions, and all 17 ground-truth
fixtures without expectation changes. The final role fallback changes one fixture
expectation, documented below. Added synthetic cases exercise the accepted correction and exclusions;
private demo08 assertions preserve its existing scores, roles, and teams.

The separate 2,880-file aggregate run still accepts 2,865 and rejects 15. Compared
with the pre-fix parser, only static name-conflict diagnostics change: timeline
participant comparisons in 135 files and chat sender comparisons in 28. These
compare historical/static slot names with the corrected result name. Every other
aggregate summary field is identical.

The tracked aggregate baseline also needed to catch up with the earlier committed
recorder fix: spectator-view events were already 5,780 at `b59654d`, down from the
old baseline's 8,662. This patch does not change that count. The reviewed new digest
is `b9ba5869d6a6e05b22330c228d779b2d83a3d6ef3b5e67a4eb74b0a92826a963`.

## Initial name-fix Flatpak verification

The local Flatpak was rebuilt from this patch and its full 4,089-entry library was
rescanned with `detail-v34`. SQLite contains 4,074 successful entries and the same
15 failures, totaling 37,193 player rows. All 173 corrected rosters match the native
comparison output exactly, including their row counts and complete name lists.

Installed Flatpak commit:
`d470e1a10480d8ed52e783b82659375b46039044f291fd0001e40c5576873d67`.
This verifies the installed importer and persisted rosters; individual rendered
replay views remain for user review.

## Explicit-role fallback: demo186

`demo186.wicdemo`, SHA-256
`bb918d1929086f9fbe616e1471073b358aa61717dbe5d49e98cda04a3b3ea849`,
shows `-=chica=-smallisland` in slot 8 with 930 points. Both the original parser and
the name-only patch leave the role blank. Its raw end-summary block at decompressed
offset 25,647,804 contains zero in every `aScoreRole0` through `aScoreRole3` field,
but explicitly records `aRoleId = 0x1abf03df` (Support) at offset 25,647,821.
Capturing 160, fortification 71, repair 185, and unit damage 514 sum to 930.

The parser now retains that explicitly framed role ID separately from its existing
primary-role calculation. Only positive-score players whose calculated primary role
is absent use this fallback. Unknown IDs and the FPM role ID remain unavailable;
existing nonzero per-role scores keep their original precedence. Numeric fields,
identity correction eligibility, and team resolution are unchanged. The fallback
reports the recorded role, not a claim about which role earned the most points.
The detail cache advances to `detail-v35` for this follow-up.

The follow-up comparison covers all 4,089 library paths against the preceding
name-only patch: 169 blank roles are filled across 134 files (66 distinct contents).
Every previously populated role and every non-role summary field is unchanged;
4,074 files still parse and 15 still reject. Rust and Python role-fallback parity
is checked separately from existing roster differences, including Python omitting
an unscored spectator in 1166__demo07. The synthetic regression includes
known-score precedence,
zero-score players, unknown role IDs, and FPM exclusion.

One original ground-truth expectation changes: demo100 slot 2, Donald Duck, from
blank to Armor. SHA-256
`77b8936c078ff7d712276b338361bb616fde8db45b248e0337c059747027aa2d`;
its summary at decompressed offset 6,586,159 has explicit `aRoleId = 0x10e00313`,
934 points, and zero per-role totals. No other ground-truth expectations change.

The full/private gate passes with 17/17 ground-truth fixtures after that one
evidence-backed expectation correction.

Python and Rust agree on all 83 corrected roles across the 66 distinct affected
contents. The user reviewed the role-fallback build and reported that demo186 looked
better. The local Flatpak was subsequently rebuilt and installed with the final
names-only spectator grouping. All 35 targeted frontend tests and the fast quality
gate pass for that display-only correction. Parser data is unchanged by the grouping.
