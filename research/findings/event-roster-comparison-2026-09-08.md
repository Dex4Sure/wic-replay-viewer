# Event-driven Overview roster comparison — 2026-09-08

> Follow-up: the targeted fixes have now been implemented in the
> viewer component. See [implementation and validation](../components/replay-viewer/docs/targeted-event-results-2026-09-08.md).
> The follow-up [departure policy and unresolved Flatpak freeze](../components/replay-viewer/docs/departed-roster-overflow-2026-09-08.md) are documented separately.
> The comparison-only procedure and baseline measurements below remain historical.

## Subsequent viewer policy

The viewer now prefers the replay's complete end-screen table and current
occupants, rather than promoting a generic last-live-roster snapshot. The original
comparison below remains historical. See the [implemented end-screen policy](../components/replay-viewer/docs/final-screen-results-2026-09-09.md)
and [eight fallback recordings](final-screen-fallbacks-2026-09-09.md).

## Original recommendation

Keep the current Overview as the default. Event history exposes useful remaining
corrections, but replacing Overview with the final live roster loses departed
participants. A whole-match event roster introduces slot reuse and faction-switch
ambiguities that cannot be displayed under the one-person-per-slot policy without
an additional, evidence-backed attribution rule.

The most promising follow-up is direct `SetScore.aPos` score attribution, followed
by reviewing the small set of remaining name and spectator candidates. No candidate
in this report has been applied to the product.

## Baseline and scope

- Viewer/parser commit: `1942692544bc84dc4b563f3dbba0fcb9b8cb5dc2`.
- Installed Flatpak: `64acac82a99b0193995da2cb2492db5eeddf904d5e535ecc0ceef1084f5b6af2`.
- Cache: `detail-v42`; product 0.4.0, timeline 18, database 12.
- Read-only inventory: **4,089 paths / 2,499 distinct SHA-256 contents**.
- Same acceptance: **4,074 paths / 2,491 contents accepted**, **15 paths / 8 contents rejected**.
- Seven accepted contents lacked a usable gameplay clock/primary-chain result pair;
  the comparison abstained on those. Another 93 comparable contents did not capture
  match start; conservative correction candidates abstain on those too.
- `CG's epic fail` / `333__demo17` is one content with three copies, reported broken
  in-game. It was processed as a diagnostic control and excluded from the accuracy
  comparison below. None of the 23 roster candidates comes from it.
- The remaining **2,483 contents** are comparable and not known broken. This does
  not certify them all as playable in-game.

The native probe was compiled from unchanged production parser source in the
Flatpak SDK build cache and executed inside the installed Flatpak runtime with
four workers. Identical input bytes were processed once and results mapped back to
every alias. No dev server, replay edits, library database writes, application
reinstall, cache-key change, or production parser/viewer changes were made.

## What was compared

1. **Current Overview:** canonical parser rows, existing abandoned-duplicate
   suppression, and the current departure-score projection. Unknown teams are
   grouped as spectators using the existing viewer policy.
2. **End snapshot:** named sessions still present immediately before the first
   primary-chain `TeamWins`, grouped by their latest session-local team event.
   This reflects the live scoreboard's question: who is here now?
3. **Gameplay participants:** named sessions with a recorded playing-team interval
   and observed gameplay activity (owned unit creation, recognized role selection,
   or a nonzero score observation). Departed sessions remain present. Multiple
   occupants/factions stay explicit in the diagnostic output; they are not merged.
4. **Conservative review candidates:** a full captured start, a unique named
   gameplay session with explicit entry, and either consistent unit/team evidence
   for an alternate name or explicit spectating with zero final statistics and no
   observed gameplay activity. These are review hypotheses, not automatic fixes.

The event model is deliberately strict about missing team evidence. Unit team
observations are recorded separately rather than silently filling missing team
joins. Initial names use the same resolved metadata seed as playback; this is not
an entirely independent source of identity truth. Byte offsets preserve ordering
where projected timestamps could tie. Final statistics and score-derived roles are
not reconstructed from role-selection events.

## Comparison results

Counts below exclude the known-broken fixture and seven uncomparable contents.
A difference is not automatically an improvement.

| Alternative           | Distinct replays differing from Overview | File copies affected | Main drawback                                                                            |
| --------------------- | ---------------------------------------: | -------------------: | ---------------------------------------------------------------------------------------- |
| End snapshot          |                              747 / 2,483 |                1,182 | Omits departed players and uses final presence/team state                                |
| Gameplay participants |                              613 / 2,483 |                  939 | Adds historical occupants and faction memberships that cannot all occupy one result slot |

The snapshot removes **1,243 rows with nonzero displayed scores**. Some rows can
also be replaced or regrouped, so this count is not a claim that every removal is
incorrect. It demonstrates that copying the end snapshot would discard substantial
existing result information.

The participant alternative creates a one-slot display conflict in **407 distinct
replays**: **335** contain multiple named people in the same slot and **220** contain
one person on multiple playing factions. These sets overlap. They cannot be added.
No identities were merged by matching names, and no score totals were summed across
occupants or faction switches.

Examples:

- `358__demo85`: the snapshot omits i.s.a., HUSSAR001, and Neuro, whose departed
  scores the current Overview deliberately preserves. Whole-match participation
  also exposes faction changes; simply selecting the last team would change what
  Overview means.
- `799__demo03`: the current slot-4 Weed Queen result remains USA/Air/-11.
  Separately, slot 2 has a named `[BL0W]Real-Escobar` gameplay session with six unit
  creations and a last score of 46 before departure; Overview still labels that
  zero-stat slot `[BL0W]Weed Queen`. This is a name/score-attribution review candidate,
  not permission to transfer either occupant's statistics.
- `1269__sh.esvsNoWrivieraNATO`: Balrog and Siderk remain zero-stat faction rows,
  but their sessions include explicit spectator events and no observed gameplay
  activity. Both are candidates for a targeted spectator-display review.

## A separate score extraction finding

For matching named players still present at the result, the final explicit live
score differs from Overview in **106 rows across 105 distinct replays**:
**102 rows use slot 15**, two slot 14, and one each slots 0 and 8. The latter two
occur in the same bot-containing replay, `421__demo01`.

The Python reference decoder independently confirmed all 106 last observations.
For `Replay Old/demo96.wicdemo`, `[CSiE]Spirit_Gun-77` has Overview score **809**, but
a complete `SetScore` envelope at decompressed offset **15,966,707** explicitly
records **aPos=15, aPlayerScore=810**. The legacy final-score scan fails to include
that last value because it derives the slot from a following counter field, rather
than that same message's `aPos`. Its extracted counter-0 result remains 809.

This is a concrete extraction gap in that example, not merely a different roster
policy. It merits a separate targeted patch and corpus comparison. This experiment
does not replace final scores wholesale with the last live sample: departures,
resets, post-result events, and occupant boundaries still require checks.

## Remaining roster candidates

**23 rows across 21 distinct replays / 31 file copies:** 11 name reviews and 12
spectator reviews. No playing-faction replacement passed the conservative unit/team
candidate rule. All 23 have a captured match start. Independent Python inspection
found no pregame unit creation in their slots. That absence alone is not proof of
spectating or nonparticipation.

Name candidates require final-statistics attribution review before any rename.
Spectator candidates require checking observer modes, lobby roles, entry identity,
and game-time boundaries; they must not become a blanket zero-score filter. One
candidate (`Early 2010 MM #1`, sheepy) has a name decoded by Rust but omitted by the
Python reference's printable-string rule; its identity needs an explicit byte-level
check before promotion.

| Replay                                     | Slot | Current name          | Current score | Candidate to review          |
| ------------------------------------------ | ---: | --------------------- | ------------: | ---------------------------- |
| 799__demo03.wicdemo                        |    2 | [BL0W]Weed Queen      |             0 | Name → [BL0W]Real-Escobar    |
| 101__demo67.wicdemo                        |    2 | Shiny^Hej Hej         |             0 | Name → [+Eng+]Shaq           |
| 101__demo67.wicdemo                        |   14 | sh.es^WereK           |             0 | Spectators, names only       |
| 1063__demo23.wicdemo                       |   11 | -=LCDA=-nadal         |             0 | Name → [NOTB]Wiki            |
| 1269__sh.esvsNoWrivieraNATO.wicdemo        |    9 | NoW√^°Balrog°         |             0 | Spectators, names only       |
| 1269__sh.esvsNoWrivieraNATO.wicdemo        |   10 | sh.es^Siderk          |             0 | Spectators, names only       |
| 16__Cr4ck vs dc.wicdemo                    |    6 | [^BOB^]»HIM«          |             0 | Spectators, names only       |
| 259__demo249.wicdemo                       |    1 | Shiny^MeinHarenz      |             0 | Name → -=WICC=-KeeperOfPeace |
| 319__USAAirport.wicdemo                    |    5 | -=kado=-Storyteller   |             0 | Spectators, names only       |
| 349__WakeBetaTesting5v5.wicdemo            |    2 | [§]Mujahid            |             0 | Spectators, names only       |
| 415__demo02.wicdemo                        |   14 | TrooperSwe[GRB]       |             0 | Name → Bloodbeard[TACE]      |
| 658__demo118.wicdemo                       |    8 | -=TBP=-.#lan          |             0 | Name → Lee999                |
| 861__demo04.wicdemo                        |    6 | [SAS!]harbringer388   |             0 | Name → Lapsior               |
| 993__demo01.wicdemo                        |    6 | [RKKA]PH1D3L C@STR0   |             0 | Spectators, names only       |
| demo45.wicdemo                             |    7 | yellow                |             0 | Name → BKnight3              |
| demo14.wicdemo                             |    7 | [•S•]Sgt.Pepper       |             0 | Spectators, names only       |
| 1v1 Dexter vs GeneralX Seaside #17.wicdemo |    2 | ?^ chuchu             |             0 | Spectators, names only       |
| 4v4 MM Seaside 2010 #2.wicdemo             |    4 | -=NKWD=-Draconotanker |             0 | Name → 龍^Monkey_Nuts        |
| 5v5 MM Hometown 2010 #6.wicdemo            |   11 | voll^Di3T3R           |             0 | Spectators, names only       |
| Early 2010 MM #1.wicdemo                   |    7 |  ^-sheepy-            |             0 | Spectators, names only       |
| PUB Fjord Light Blob Trolling.wicdemo      |    5 | [--->]0ptimus         |             0 | Name → [B&H]noobonceagain    |
| demo158.wicdemo                            |    6 | [FFZ]Seinfeld         |           267 | Name → V V VI VI             |
| demo406.wicdemo                            |    5 | [a.]Blitz             |             0 | Spectators, names only       |

## Validation and artifacts

- Six synthetic controls passed: departures, slot reuse, missing team evidence,
  late captures, zero activity, and absent match results.
- All 4,089 baseline summaries matched the installed library's roster counts,
  names, faction sets, recorder faction, and cache version; zero issues.
- The production timeline and live playback extractors independently reproduced
  all six selected snapshot rosters: demo03, demo17/CG, demo85, demo95, NoW vs
  Shiny 3, and sh.es vs NoW Riviera.
- Independent Python decoding checked every roster-candidate replay and every
  score-discrepancy replay (125 distinct contents, including overlapping cases), with source SHA-256
  verification. The evidence records entries, leaves, team/spectator/role events,
  unit counts, offsets, and score observations.

Sources in this directory:

- `compare-event-rosters-2026-09-08.rs`: bounded native stream/session probe.
- `analyze-event-rosters-2026-09-08.py`: alternatives, differences, and review rules.
- `test-event-roster-comparison-2026-09-08.py`: synthetic policy controls.
- `verify-event-roster-candidates-2026-09-08.py`: independent Python evidence probe.
- `check-live-roster-controls-2026-09-08.rs`: production playback control extractor.

Local generated evidence (ignored; contains private corpus paths):

- `event-roster-inputs-2026-09-08.json`: all hashes and aliases.
- `event-roster-extract-2026-09-08.jsonl`: current results and observed sessions.
- `event-roster-comparison-2026-09-08.json`: all alternative rosters and differences.
- `event-roster-proof-2026-09-08.json`: independent candidate evidence.
- `event-roster-baseline-check-2026-09-08.json`: installed-library comparison.
- `event-roster-live-controls-2026-09-08.jsonl` and
  `event-roster-live-check-2026-09-08.json`: production controls and comparison.

## Reproduction

Use the recorded baseline and a read-only library inventory with SHA-256 grouping.
Each manifest item has `path`, `sha256`, `aliases`, and `expectedErrors` (one error
or null per alias). Verify all source hashes before processing.

Copy the two Rust probes into the parser's `examples/` directory **in the disposable
Flatpak SDK build cache**, as `compare_rosters.rs` and `check_live_roster.rs`.
Their includes resolve the unchanged parser and playback code from that cache.
Verify those sources equal the baseline checkout; do not edit production sources.
Build each example with:

```text
cargo build --locked --offline --release --manifest-path parser/rust_parser/Cargo.toml \
  --example compare_rosters --no-default-features --features cli
```

Repeat with `--example check_live_roster` for controls. Use the configured Flatpak
SDK's Rust PATH and CARGO_HOME. Run the first binary inside Flatpak with the full
input manifest and the second with the six-control manifest. Grant only read access
to private replays and manifests; redirect stdout to the respective JSONL files.
The native probe has four bounded workers and does not open the library database.
From the workspace root:

```bash
python findings/test-event-roster-comparison-2026-09-08.py
python findings/analyze-event-rosters-2026-09-08.py \
  findings/event-roster-extract-2026-09-08.jsonl \
  findings/event-roster-comparison-2026-09-08.json
python findings/verify-event-roster-candidates-2026-09-08.py \
  findings/event-roster-comparison-2026-09-08.json \
  findings/event-roster-extract-2026-09-08.jsonl \
  findings/event-roster-proof-2026-09-08.json
```

Remove temporary example sources from the SDK cache after validation. The product
worktree remains clean and the installed app continues using the published version.
