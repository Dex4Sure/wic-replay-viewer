# Signed player scores

`799__demo03.wicdemo` (SHA-256
`5d31785e013fe563439e9922d83520dc13f7a77f141510b9724179e5399fa5d6`)
records slot 4, `[BL0W]Weed Queen`, ending on a score of -11.
The final live `aPlayerScore` field at decompressed offset `0x1d87ba`
has signed integer flag 0 and bytes `f5 ff ff ff`. The preceding live scores
include -1, -5, -7, -8, and -9. Reading that final value as unsigned produced
4,294,967,285. The end-summary air and total values similarly decode to -12.

The pre-result-recovery baseline executable and current parser both reproduced
this error. It was an existing bug, not introduced by role-fallback removal.
The saved 4,089-path corpus baseline contains wrapped score fields in 21 paths
representing 12 distinct replay contents. Baseline equivalence alone did not
establish that those scores were correct.

Both Rust and Python now decode scores as signed 32-bit values. The viewer detail
projection accepts signed scores; sorting, team totals, and primary-role comparisons
use their actual values. Nonzero negative final scores remain in `playerScores`
and the scored roster. Category negatives are preserved. No score is clamped or
replaced with a guessed value. Zero role totals provide no activity evidence; the highest nonzero signed role
score supplies the primary role. Thus negative-only role evidence is retained,
while a positive score outranks a negative one. Explicit-summary-role rules remain
unchanged. Identity correction tests also cover negative role activity. Event-backed departure recovery is unchanged.
Viewer cache `detail-v42` forces reprocessing of formerly unsigned results.

## Validation

The final optimized-parser comparison covered all 4,089 library paths: 4,074
accepted and the same 15 rejected. Exactly 21 paths (12 distinct replay contents)
changed. Changes were limited to signed score values, their descending score list,
and seven distinct players' primary roles where negative penalties previously
outranked positive role points. Names, teams, other result fields, hidden duplicate
IDs, and all 514 recovered departure scores remained unchanged. Negative-only
role evidence, including Weed Queen's Air role, remains visible.

Full portable and private quality gates passed, including 17/17 ground-truth
fixtures with none skipped. Rust and Python regressions cover negative scores,
negative-only role evidence, and late-entrant identities. The frontend regression
checks descending positive/zero/negative score display.

The installed Flatpak build
`64acac82a99b0193995da2cb2492db5eeddf904d5e535ecc0ceef1084f5b6af2`
refreshed all 4,089 paths under `detail-v42`. Its 4,074 accepted summaries,
15 rejections, 37,175 visible rows, and nine fresh/cached detail checks matched the
expected results. Both copies of demo03 passed; the actual projected player retains
`[BL0W]Weed Queen`, USA, Air, score -11, and Air/total -12.
