# Replay end-screen results

Overview and library summaries now prefer the replay's final results screen over
reconstructed historical participation. This intentionally preserves the screen's
slightly different totals rather than substituting the final live `SetScore` value.
Player Performance and the roster therefore use the same `SetScoreAtGameEnd`
statistics. This is replay-screen fidelity, not server-result reconstruction.

Match formats count every selected player with a playing team, regardless of score
or server mode; spectators remain excluded and unknown teams leave the format
unknown. This counting change uses cache `detail-v48`. The `detail-v47` references
below describe the earlier final-screen implementation and installed-build checks.
See [library search](library-search.md) for refresh instructions.

## Source and boundaries

Read-only inspection of the original PE32 x86 client, image base `0x00400000`,
version `1.0.1.1 (b35)`, SHA-256
`41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc`, confirms:

- `0x00b82a30` serializes `SetScoreAtGameEnd`: slot, role ID, total, seven category
  scores and four role scores. It contains no name or team.
- `0x00935510` forwards that table to the screen; `0x008bb660` stores its twelve
  score values per slot. `0x008bc220` selects total-score ordering (`EAX=0` at
  `0x008bc50c`) for the team lists and fills at most eight positions per side.
- `0x008bc050` gets name and faction from the current player state. It requires
  the slot to be present, excludes type 2, and excludes the spectator flag.
- `0x00936bf0` clears presence when a player leaves. `0x00936f90` establishes the
  new occupant; `0x00b86b50` records the entry's team, name and type.
- `0x00939be0` and `0x0093f2a0` update spectator status and playing membership.

The parser API `final_screen_players()` validates a final-score table independently,
then walks the validated primary chain,
tracking explicit entries, leaves and team/spectator changes until `TeamWins`.
It reads complete, correctly typed, 259-byte end-score envelopes. Every result
row needs a named, present occupant. Duplicate or malformed rows, unexplained
active slots without a summary, roster changes during the summary, missing primary
results, or a playing team exceeding eight cause the entire candidate to abstain.
An earlier unknown occupant can be discarded by an explicit leave or superseded
by a complete later entry; it cannot supply a final name.

For a supported final screen, Overview normally uses these rows and their total scores.
Departed people are absent even when their earlier scores were nonzero. Current
spectators and unassigned team-zero players remain names-only in the viewer's
spectator group. Zero-score playing members are retained if the final screen's
state includes them. Roles remain derived from per-role scores, with the existing
same-summary role-ID fallback for a positive total and zero role scores.

The existing historical parser JSON and playback/event identities are unchanged.
`roster::prepare_results` selects the final-screen rows for both import summaries
and detail generation before caching. Selected final-screen results bypass
abandoned-duplicate suppression and departure-score recovery; no last-known-score
asterisk is added. The old behavior remains when final-screen extraction abstains or its roster
lacks an opposing side that the filtered historical fallback can restore. This
exception selects the entire fallback, including its existing score recovery and
departure filtering. All players on both sides use fallback scores, including
supported last-known scores with their asterisk; no final-screen scores are mixed
in. Cached and fresh details have the same projected rows. Cache `detail-v47`
refreshes stored summaries/details; product, timeline and database versions do not
change. The historical fallback remains part of result selection.

## What is authoritative

`SetScoreAtGameEnd` is the authoritative source for the replay end screen's
serialized total, category, and per-role scores. It is not a source of player
names or teams, and it is not a claim about the values reported to the server.
Names and membership are resolved from recorded player state at the result.
A valid score table alone is insufficient to select the new roster projection.

## Corpus comparison

The optimized native probe ran inside Flatpak over 4,089 paths / 2,499 distinct
contents. Acceptance is unchanged: 2,491 accepted and eight rejected contents.
In the original comparison, the final-screen decoder succeeds for 2,483 contents.
Eight accepted contents require the fallback because extraction abstains: seven lack a complete primary result, and one has incomplete final
occupant identity data. No candidate playing team exceeds eight rows.

### The eight fallback recordings

| Replay                       | Reason for fallback                                                             |
| ---------------------------- | ------------------------------------------------------------------------------- |
| `demo192.wicdemo`            | Final table has 16 rows, but slot 12’s later entry omits its name.              |
| `1050__LOLATCOOLGUY.wicdemo` | An earlier zlib checksum failure stops traversal before the intact final table. |
| `1077__demo217.wicdemo`      | An earlier zlib checksum failure stops traversal before the intact final table. |
| `262__demo35.wicdemo`        | An earlier zlib checksum failure stops traversal before the intact final table. |
| `348__demo01.wicdemo`        | An earlier zlib checksum failure stops traversal before the intact final table. |
| `277__demo23.wicdemo`        | An earlier zlib checksum failure stops traversal before the intact final table. |
| `373__demo95.wicdemo`        | An earlier zlib checksum failure stops traversal before the intact final table. |
| `313__demo419.wicdemo`       | An earlier zlib checksum failure stops traversal before the intact final table. |

`demo192.wicdemo` is the recording in `Replay Old` / the recovered `replays/old`
folder. It has usable final statistics; the all-or-nothing roster guard abstains
because slot 12's identity is incomplete. The other seven are WicTracker downloads.
Their primary chain does not reach the result because earlier compressed data is
damaged. The focused follow-up below confirms intact final tables in all seven;
it does not establish complete final identities or in-game playback.

All eight retain the existing viewer behavior. The count is by distinct file
content: demo192 has two library copies. Exact SHA-256 identities and paths are
recorded in the research directory's `research/findings/final-screen-fallbacks-2026-09-09.md`.

Against the preceding `detail-v44` viewer audit, 2,479 contents change. Across
shared slots, 18,502 scores, 1,468 names, 665 teams and 83 roles differ; there are
713 removed rows and 1,037 added rows. These are projection differences, not a
count of previously incorrect results. The new names describe end-screen
occupants, and final totals can differ from live totals. Every candidate score,
category value, identity and team was checked against the separately decoded
messages/state; input identities retain SHA-256 hashes.

Examples:

- The offending demo58 (`7ef6f598e6229210d958387435fa25b8f6501102bdaccae8c10efd59e3986dae`)
  has no final-score row for departed SkorpioPlayer. Its team lists are 7v8 without
  the overflow-departure rule.
- `799__demo03` (`5d31785e013fe563439e9922d83520dc13f7a77f141510b9724179e5399fa5d6`)
  shows Weed Queen at the screen's -12 rather than live -11. Departed Real-Escobar
  and Xxplosive are absent instead of receiving recovered earlier scores.

The independent Python decoder matches native output on 32 controls, including
all eight fallback recordings and the previously reported roster examples.
Synthetic tests cover replacements, missing records, departures, signed totals,
spectators, zero totals, duplicate/malformed summaries, overflow and post-result
activity. Viewer tests cover bypassing recovery and cached/fresh equivalence. The full
product quality gate passed (parser coverage 91.92%, viewer core 84.79%, Tauri
command core 100%). A private demo58 test also passed through actual summary
import, detail generation, and database cache reload. All 17 saved parser ground-
truth fixtures passed without skips; those validate the unchanged historical
parser contract, while the separate corpus comparison validates the new projection.

This investigation does not resolve the previously reported Flatpak scan freeze.
Corpus probes do not exercise the installed GUI. End-screen row layout and exact
ordering have not been independently screenshot-matched in the original client.

A separate full-corpus audit found no primary `SetPlayerLANName` messages. That
additional name-update path is not implemented by this candidate; recordings
using it remain an unverified case outside these corpus results.

## Earlier local review build

Flatpak deployment `6dcaecbdd6ce10b9b9cc90a36d202261c62b28ba32ae72af7044ac2dd82f1e04`
was installed locally. Its executable matches the built package and includes
`detail-v45`. This is the earlier end-screen build; it does not include the
one-sided match exception or empty team cards described below (`detail-v46`).
At that stage those changes had not yet been installed locally, and the GUI had
not been rescanned as part of that validation. The updated installation is recorded
below. Every recorded owner remains present
by name in the original final-screen corpus projections.

## One-sided match exception

A subsequent Flatpak corpus comparison found 43 distinct recordings where the
historical fallback restores the missing opponent. USA and NATO count as one
allied side, opposed to USSR; spectators and unknown teams do not count. The
exception checks the fallback after abandoned-duplicate suppression.

Two recordings remain one-sided: `Kide Spec Dexter vs Silver.wicdemo` (also
`Kide vs Silver 2.wicdemo`) and `798__demo02.wicdemo`. Overview keeps the normal
opposing team card with an empty player list. Allied names are taken from result
evidence or unambiguous recorded timeline factions, without a generic team name.
The original extraction counts above remain decoder results; that audit selected
2,440 final-screen rosters and 51 historical fallbacks.

The comparison covers 2,499 distinct contents across 4,089 paths: 2,491 remain
accepted and eight rejected. After applying the existing detail score-recovery
projection, there are no unexpected roster/statistic differences and no playing
teams above eight players. The full local quality gate passed. Browser rendering
confirmed normal faction headers with zero player rows in empty cards.

## Follow-up: intact tables and missing identity evidence

A focused recheck of all eight extraction failures found intact, contiguous
`SetScoreAtGameEnd` tables immediately followed by `TeamWins`. Each table and
result occupies one checksum-valid compressed chunk. The seven WicTracker files
have earlier zlib checksum failures that break primary-chain traversal; their
final score data is present. In demo192, slot 12 leaves and subsequently enters
without a name field. Its type field is present, but that alone cannot identify
the new occupant. The earlier name is not transferred across that departure.

The parser now decodes the final score table independently, then requires reliable
occupant evidence before selecting it. Skipped compressed data before the result
forces fallback even when the concatenated event bytes happen to remain aligned.
An intact table cannot override an incomplete chain or an unresolved occupant.
All eight investigated recordings retain their fallback; the one-sided exception
is unchanged. Cache `detail-v47` refreshes projections for this stricter check.
No parser JSON or timeline schema change is involved.

See [the focused findings](../research/findings/final-screen-fallbacks-2026-09-09.md)
for input identities and the distinction between readable scores and reliable
final-player attribution.

## Current validation and local installation

The updated Rust parser and Python reference agree with the saved candidate baseline
on 32 SHA-256-verified controls: all eight extraction failures retain fallback, and
the other 24 retain the same final-screen players and statistics. The full portable
gate, private regressions, Quarry playback control, and all 17 ground-truth fixtures
passed. This focused check does not replace the earlier full-corpus audit.

The updated Flatpak was installed locally as deployment
`22f5172e35b69f1908537a712e706feace34a7f59c36177c143b9e3482fa85a1`.
The installed executable matches the built package byte-for-byte (SHA-256
`c68109128bca2390e81ee05a6fe2f8ef61d63788f4cefe43e150a395b0cad80f`)
and contains cache `detail-v47`. It includes the one-sided exception and the
compressed-data continuity checks. The user reports that the updated app works
well so far; this is initial feedback, not a claim that the earlier freeze has
been reproduced and resolved.
