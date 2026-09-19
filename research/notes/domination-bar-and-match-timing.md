# Domination bar and match timing

Empirical findings on the `.wicdemo` domination bar and countdown clock, from a
254-replay corpus spanning `replays/main`, `replays/wicgate-documents`, the
WicTracker downloads, and `Imports/Replay Old`. Observations are separated from
the conclusions drawn from them.

## `aFactor` is the only domination signal

The bar is the `aFactor` BinTag field, hash `0x0a6f02c1`, type float, carried by a
single parent event `0x49160769`. Every occurrence in every sampled replay belongs
to that parent; there is no second stream.

That parent is now named: **`UpdateBalanceFactor`**, and `aFactor` is its only
field. Enumerating the callers of the client's message-begin function confirms it
is the sole message carrying the value, so the "no second stream" observation
holds from the writer side and not only from corpus sampling. See
`findings/bintag-message-inventory-2026-08-22.md`.

Three independent searches for a per-team domination figure came back empty:

- `SetGameModeData_Float` (`0xfb079156`) carries only `aDataType = 1`, the countdown
  clock. No other data type appears. The name carries an underscore; the literal
  `SetGameModeDataFloat` hashes to a value present in no replay. Its sibling
  `SetGameModeData_Int` (`0x47210730`) exists in the writer but is unused here.
- The metadata chunk contains no domination field and no round length.
- A differential scan of the post-`TeamWins` block against a known 100%
  total-domination replay found only per-unit floats, no summary percentage.

## The bar is POV-anchored and mirrors on team change

`aFactor` is reported from the recording player's perspective. Comparing
`sign(aFactor - 0.5)` against "the recorder's team won":

| mode | agreement |
|---|---|
| Domination | 213/214 |
| Tug of War | 9/9 |

The single Domination-mode exception is an Assault map (see below).

The anchor **mirrors** when the recording player changes side. In
`replays/main/demo25.wicdemo` at t=73.5 s the value jumps `0.7003 -> 0.2987` in one
tick while the slope reverses from +0.0010/s to -0.0010/s — the physical bar never
moved. The trigger is adjacent: `PlayerJoinedTeam slot=0 team=USA` at offset
3,003,587, after that slot appeared as `SpectatorJoinedTeam` at offset 269,123.
7 of 249 replays contain such a frame change.

Detecting a mirror from the values alone is **unsound**. A mode that moves a front
line in discrete steps makes a genuine move across the centre its own mirror:
`0.6 -> 0.4` is exactly `1 - 0.6`. Frame changes must be located from the
recorder's team-change offsets and only then confirmed against the value.

## Semantics are mode-dependent

| mode | quantisation | update rate | two-sided split? |
|---|---|---|---|
| Domination | 1/3000 continuous | ~1 Hz | yes |
| Tug of War | 0.5 neutral, then fifths (0.2/0.4/0.6/0.8) | on change only, 3-28 per match | yes |
| Assault | 0.5 neutral, then sixths | on change only | **no** |

Assault tracks attacker progress rather than a split between the two sides, and is
the one replay where POV anchoring fails
(`626__FTLvsTankKilla5v1THYPOONSHITTYMAPLOLNOOBBOONHAX.wicdemo`: bar 0.0 with the
recorder's own team winning).

Mode is inferred from the map display-name prefix (`do_`/`tw_`/`as_`), which is not
always reliable: `replays/wicgate-documents/demo21.wicdemo` carries a `do_` prefix
but its bar moves in discrete fifths with 11 emit-on-change samples.

## The bar is a lead meter, not a control meter

It only moves while one side holds more command points, so a late comeback erases
an accumulated lead. A decisive match can legitimately finish near 0.5:
`Delta.Squad.vs.Team.36.red.Silo.wicdemo` peaked at 0.5797 and returned to 0.5023
while USSR won 5713-3691 on player score.

Because the bar resolves to 1/3000 — steps of 0.0333% — rounding a displayed
percentage to whole numbers destroys real results. `demo335.wicdemo` finished
USA 50.07% / USSR 49.93%, confirmed by watching the replay; whole-percent rounding
showed it as a 50/50 tie.

## `TeamWins` freezes the bar

No `aFactor` record follows `TeamWins` in any of the 249 replays checked, and the
Domination stream runs at ~1 Hz right up to it (largest inter-sample gap 2.1 s). The
frozen value is therefore the true final state.

The countdown plus the frozen bar separate three endings cleanly:

| ending | n | clock remaining |
|---|---|---|
| bar pinned at exactly 0.0/1.0 — total domination | 158 | median 534 s, none under 2 s |
| bar strictly between, clock expired — timer finish | 74 | under 2 s |
| bar strictly between, clock left — forfeit | ~14 | minutes |

## Match time is not recording time

The countdown reads zero only before it starts, so a recording that observed a zero
sample covers the match from its first second. Without one, the recorder joined
mid-match.

14 of 254 replays are late joins. Measuring elapsed time from the first countdown
value *seen in the recording* understates the match by up to 9.9 minutes:

| replay | recording | actual match | round |
|---|---|---|---|
| `1053__CIAxmasstatpad.wicdemo` | 1.2 min | 10.0 min | 600 s |
| `demo09.wicdemo` | 4.6 min | 9.8 min | 600 s |
| `demo335.wicdemo` | 15.4 min | 20.0 min | 1200 s |
| `demo50.wicdemo` | 12.0 min | 15.0 min | 900 s |

Round length is only observable when the recording captured the start. Otherwise it
must be inferred; observed lengths are 1200 s (233/249), 900 s, and 600 s.

The countdown keeps ticking **past zero** into the post-match screen —
`1053__CIAxmasstatpad.wicdemo` reaches -64.1 s — so it must be clamped before
subtracting, or overtime inflates the match beyond its own round length.

A late join costs only history, never the final result: `aFactor` is absolute state,
not an accumulated delta (`demo335`'s first recorded sample is already 0.5120,
mid-match), and 5 of the 14 late joins still read a clean total-domination 0.0/1.0.

## Spectator recordings are not degraded

23 spectator-recorded Domination replays reach 0.0/1.0 in 61% of cases against 66%
for the 217 player-recorded ones, with an identical median bar span of 0.500. A
spectator recorder has no roster team to anchor to, so the split falls back to the
`TeamWins` winner — sound because the winning side of the bar decides the match
(verified 136/136 on total-domination and timer endings).
