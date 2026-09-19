# Future Improvements

## Timeline — remaining unit-destruction attribution

Schema v17 classifies only the deterministic exact-target tactical-aid subset.
Local schema v18 adds exact building-collapse and destroyed-container contexts and
inherits an independently exact tactical-aid cause from a destroyed container to
its occupants. Keep context separate from attacker identity: most contextual deaths
remain `cause: unknown`.

Future parser attribution requires a new replay-visible bridge. Parent-workspace
Ghidra research bounds the unclassified binary surface to six direct callers of the
server's forced-death helper plus the non-disband callers of the shared blink
scheduler; see `../../../findings/unit-destruction-phase-7-support-clouds-2026-08-24.md`
and `../../../TODO.md`. Do not classify from historical ownership, timing, proximity,
active support clouds, or blink state alone. Any candidate must preserve exact unit
lifecycle generations, reject ambiguity, and pass known-cause corpus controls before
changing this schema.

## Server mode classification

Keep the two game concepts separate in parser and viewer terminology:

- `gameMode` means the map/objective mode: Domination, Assault, or Tug of War.
  It is tied to the map variant (for example, `do_seaside` and `as_hillside`).
- `serverMode` means the server-level mode: Ranked, Match Mode, Few Player Mode
  (FPM), or Bots.

Do not expose `serverMode` as a single enum. The initial orthogonal parser
classification now implements the replay-supported subset. The 2026-08-21
evidence pass found:

- `myFPMModeFlag=1` is an exact FPM header signal and FPM also sets
  `myMatchModeFlag=1`;
- `myMatchModeFlag=1` with FPM clear identifies ordinary Match Mode;
- a `PlayerEntersGame`/roster `myType=1` is positive bot-participant evidence; and
- one unique corpus replay has both FPM flags and bot participants, so FPM and Bots
  are not mutually exclusive replay properties.

User-confirmed server behavior establishes that FPM legitimately supports games
with or without bots, but cannot be Ranked. Ranked servers permit neither bots nor
FPM and start immediately once their minimum-player requirement is met. Organized
Match Mode and clan-match servers instead require every player to press ready. This
suggests `myMatchModeFlag` is partly a ready-gating mechanism flag; FPM also sets it,
so it must not be exposed directly as a mutually exclusive display category.

The dedicated server distinguishes `ReportToMassgate` (Massgate reporting/listing)
from `RankedFlag` (ranked ladder eligibility), but the client demo-header writer
serializes neither. See the parent workspace's
`findings/server-mode-evidence-2026-08-21.md` and `scripts/server_mode_audit.py`.

**Resolved 2026-08-22: stop looking for a serialized Ranked record.** Two
independent enumerations close it. Scanning the metadata chunk of all 2,880 corpus
paths shows every demo header carries exactly `RecordingPlayerSlot`, `myVersion`,
the four mode flags, a recording-length float, and `FileVersion` — no spare
boolean. The complete message inventory (169 messages, 0 unresolved) contains no
ranked, ladder, Massgate, or report field. Ranked reaches the server browser
through the registration path rather than the replay.

The parser therefore keeps `serverClassification.ranked` null permanently: the bit
is absent from the file, and the parser reports evidence rather than inference.
The residual — all five modes positively false — _is_ ranked games, but that is an
elimination argument closed by operator ground truth, so it belongs in a display
layer. The viewer labels it there. Two constraints travel with it: the inference is
one-directional, because the dedicated server permits Ranked alongside Match Mode,
clan, and tournament servers, so the labelled buckets must never be presented as
unranked; and it needs every mode positively false, never merely unlabelled. See
the parent workspace's `findings/ranked-server-mode-hypothesis-2026-08-22.md` and
`scripts/ranked_bucket_audit.py`.

## Tactical Aid — selected multi-strike size is not serialized

The parser emits one raw `tacticalAidUsed` event per observed placement. It does not
report single/double/triple grouping: equal-cost placements cannot be distinguished
from separate calls, queued strikes may be held indefinitely, and late-start
recordings may omit an opening placement. Few Player Mode additionally uses rising
marginal ladders such as 6, 12, 6.

There is no known exact reconstruction from the current record set. If a future
binary or format pass finds a message carrying the selected bundle size, add it as a
separate parsed fact rather than reviving timing-based inference.

## ~~Timeline — gameplay `Event` envelope timestamps are unused~~ — DONE (schema v13)

Timeline timestamps are now recording-elapsed seconds read from each record's own
`Event` envelope. The two conflated clocks are separated: `durationSeconds` is
replay length, `matchDurationSeconds` is the countdown span, and
`ReplayData.timing` carries `recordingSeconds` alongside
`observedGameplaySeconds`.

The hypothesis was confirmed on 156 replays before the change. The envelope clock
starts at `0.0` and advances monotonically in every one; the countdown starts a
median of 66.8 s later (max 1321 s), restarts between Assault rounds, and ticks at
0.84-1.25 s per second of recording, with one replay at ~2.4x. Sixty-two replays
had a countdown-derived duration longer than the recording that observed it.

Remaining: playback-position verification against the in-game replay player's seek
bar for a long-lobby replay, which needs collaborative in-game observation.

## Timeline — sender team/faction at chat time

Schema version 11 resolves chat identity at the exact envelope offset through
`timeline.participantSessions`, but those sessions intentionally represent occupant
identity rather than mutable team state. A future derived layer could replay
join/team/spectator events up to each chat message and expose the sender's
team/faction at that instant. It must handle lobby chat, mid-game joins, team
changes, and missing state without substituting the final `ReplayData.players` team.

## Code review findings from schema v9 work (HEAD~5..HEAD)

Re-audited 2026-09-03. Items 1 and 3 are resolved with synthetic regressions.
The Enter/Leave boundary remains an evidence-dependent hypothesis. The audit-only
counter remains pending a focused output-contract review.

1. **Resolved 2026-09-03 — `read_unique_u32_field_in_event`** — when scanning an
   event body for the `aSpawnSource` field, a later spurious 4-byte hash match
   whose value bytes spill past the envelope end causes the function to
   `return None` outright, discarding an already-confirmed valid match found
   earlier in the same scan instead of just skipping the bad candidate. Needs
   an exact hash collision plus two `BINTAG_SEP` matches to trigger from
   noise, so unlikely on the current 17-fixture ground truth, but could
   silently drop a legitimate unit-drop attribution on some future replay.
   The bounds check now precedes the value read and skips the bad candidate. A
   regression proves an earlier valid value survives a truncated later collision.

2. **Enter/Leave interval boundary asymmetry (`parser.rs:1609` vs `1626`)** —
   `Enter` closes the previous occupant's session at the new entrant's
   envelope _start_; `Leave` closes its own occupant's session at the leave
   envelope's _end_. Same slot, same half-open-interval model, two different
   conventions. Might be intentional (no explicit Leave to anchor an
   in-place slot reuse to), might not be — needs a same-slot-churn replay
   fixture to pin down the intended semantics before touching it.

3. **Resolved 2026-09-03 — unresolved-name session merge** — the Enter
   dedup guard compares `intervals[active_index].player_name ==
transition.player_name`, and `None == None` is `true`. Two different
   players entering the same slot back-to-back with no decodable name
   between them get silently merged into one continuous "unknown" session,
   contradicting the documented goal that unresolved names should stay
   explicitly unknown rather than inherit a stale participant. The dedup shortcut
   now requires a resolved name, and a regression proves consecutive unresolved
   entries form distinct unknown sessions.

4. **Dead corpus-audit counter (`main.rs:166`, incremented at `370-373`)** —
   `tactical_aid_deployments_with_invalid_player` checks `player_id > 15`,
   but `parse_unit_drop_create` already rejects `player_id > 15` before
   anything reaches `TacticalAidDeployed`, so the counter can never be
   nonzero. Not validating anything; candidate for deletion.

## ~~Score Overflows~~ — NOT A BUG

7 replays show scores like `4294967294` (u32 wrap of small negative values like -2). Verified from actual game results — WiC genuinely allows negative scores (team damage penalties etc.). The parser reads them correctly as unsigned; displaying as signed is a presentation choice, not a parser issue.

## ~~Corrupted Replay Detection~~ — DONE

7 replays — all crash the game on playback. Corrupted files with no TeamWins event in the data stream.

**Detection:** Check for TeamWins hash (`0x0db80329`) in decompressed data. Valid
replays have at least one; six corpus files contain two complete end-summary segments.
Corrupted replays have 0.

**Implemented:** Parser rejects these at construction time with error "Corrupt
replay file: no TeamWins event found". The standalone Rust native/WASM parser and
Python reference parser are updated. At the time of that fix the then-current ground
truth was 16/16; the current manifest and gate supersede that historical count with
17/17.

- `1112__demo05.wicdemo`
- `1115__1COOLNOOB.wicdemo`
- `148__demo44.wicdemo`
- `149__demo43.wicdemo`
- `244__demo02.wicdemo`
- `499__GreatComeback.wicdemo`
- `49__demo11.wicdemo`

## ~~Incomplete Match Detection~~ — DONE

All three categories below ("Loser" as faction, "Spectator" as faction, [Unknown] with scores) are now handled uniformly as "incomplete match results" via `incomplete: bool` on `ReplayData`.

**Detection criteria** (any one triggers incomplete=true):

1. No valid winner — `extract_winner()` returns None when TeamWins aTeam=0 (was previously returning "Spectator")
2. Players with scores but no team assignment — `faction` is None for scored players
3. Missing opposing faction — winner exists but no non-Unknown/non-Spectator opposing faction among players

**Implementation:** CLI shows "Incomplete match results" and suppresses the domination
line. JSON/WASM output includes `incomplete: boolean` for frontend handling. The
standalone Rust native/WASM parser and Python reference parser are updated. The original
2,622-replay result-validation set flagged 35 replays (1.3%), including the 12
WicTracker replays below plus 23 from the original corpus with the same patterns;
this is a historical classification figure, not a current-corpus total.

### "Loser" as faction (2 WicTracker replays)

- `1321__demo55.wicdemo` — recorder joined in the last ~30 seconds (also has [Unknown] team with real scores)
- `798__demo02.wicdemo` — recorder's team joined a game where the opponent had already left, no one on opposing side

### "Spectator" as faction (5 WicTracker replays)

- `662__demo06.wicdemo` — custom map `bllack_forest` not installed locally, replay is valid (has TeamWins=1)
- `1038__demo05.wicdemo` — map vote interrupted game before natural end
- `1219__SeasideBUG1Test.wicdemo` — map vote interrupted game before natural end
- `1220__SeasideBUG2Test.wicdemo` — map vote interrupted game before natural end
- `979__demo01.wicdemo` — map vote interrupted game before natural end

### [Unknown] team with scores (6 WicTracker replays)

- `421__demo01.wicdemo` — bots game, bots join late (duplicate of 422)
- `422__demo01.wicdemo` — same game as 421
- `926__CIAxmasstatpad.wicdemo` — stat padding, recorder joins at very end (duplicate of 1053)
- `1053__CIAxmasstatpad.wicdemo` — same game as 926
- `961__demo05.wicdemo` — recorder joins near very end
- `1321__demo55.wicdemo` — recorder joins in last ~30 seconds (also "Loser" faction issue)
