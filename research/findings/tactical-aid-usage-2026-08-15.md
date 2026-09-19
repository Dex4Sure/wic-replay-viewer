# Tactical-aid usage records — 2026-08-15

> Historical scope: this report established recorder purchase/cost semantics and
> schema v3 placement behavior. Later work decoded the SDF support catalogue,
> added visible-faction marker actors in schema v7, both-faction deployments in
> schema v8, and validated unit-drop owners in schema v9. Current coverage is
> summarized in `opposing-player-tactical-aid-attribution-2026-08-18.md`.

## Scope

A read-only evidence pass over the `SupportThingUsed` / `ChangeHonors` record pair,
followed by an evidence-backed parser change in `components/replay-parser`.
Everything under `binaries/` and `replays/` was read only; nothing was patched and
no Ghidra project annotations were made.

The prior investigation had a working hypothesis that 80/60/40 deductions are
marginal nuke prices. That price ladder is confirmed below against player ground
truth and corpus evidence. The selected single/double/triple size is not serialized,
so timeline schema version 3 deliberately does not reconstruct bundles.

## Input identities

| Input | SHA-256 | Purpose |
|---|---|---|
| `binaries/game/wic.exe` | `41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc` | Message senders, field names, support definition names |
| `binaries/game/wic_ds.exe` | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` | Identifier string cross-check |
| `binaries/game/wic_online.exe` | `7bb41478f3070641b7639707d72a976a2ae5c8f9ad9356ff5733610979c0b7d0` | Identifier string cross-check |
| `replays/main/4600.wicdemo` | `7c9e8344477afe232eda016a9948d1121489e77bee8ec94f08a871687e2e1f42` | Record layout, repeat-use pricing, gifts between other players |
| `replays/main/demo01.wicdemo` | `03dbab783aad67b0db4b3f5fffb222cb37d981997370010737462dbdef5eb14b` | Gift-to-recorder attribution proof |

`wic.exe` is a 32-bit PE with `ImageBase = 0x00400000`; all VAs below are in its
`.text` section.

## Reproduction

```bash
scripts/ta-usage-audit.py replays/main replays/settings replays/wicgate-documents \
  --json findings/ta-usage-audit.json
scripts/timeline-corpus-check.py replays/main replays/settings replays/wicgate-documents \
  --json findings/timeline-corpus-check.json
```

`scripts/wic_bintag.py` holds the shared reader. The previous throwaway probes in
`/tmp` are superseded and are no longer referenced by anything.

## Observation 1 — the event stream is a self-describing envelope chain

Past a short metadata prefix, the decompressed image is a *gapless* chain of `Event`
envelopes rather than a soup of tags to be pattern-matched:

```text
+0   4  0x05b50203   adler32("Event")
+4   4  0x15000000   envelope type marker
+8   1  0x06         BinTag flag 6 (array/blob)
+9   4  total envelope size in bytes, header included
+13  4  float32 gameplay timestamp, seconds
+17  4  adler32(message name)
+21  …  message body: a run of 17-byte fields
```

The next envelope starts at `offset + total`. Measured on `4600.wicdemo`, walking
the chain from its anchor at byte 416 covers 25,674,217 of 25,674,234 remaining
bytes — 100.00%, with one resynchronisation — across 399,027 envelopes. `demo01`
behaves the same way. Envelope walking is therefore exact, not heuristic.

Message and field names are stock Adler-32 over the ASCII identifier, so
`zlib.adler32` reproduces them and identifier strings mined from the shipped
binaries invert the mapping.

## Observation 2 — the record layout, confirmed against the binary

`SupportThingUsed`, sender at VA `0x00b830c0`:

```asm
00b830c0  push edi / push 0 / call 0xb80880   ; acquire the event writer
00b830d6  push 0xcfd848      ; "SupportThingUsed"
00b830dc  call 0x9240a0      ; begin message
00b830fe  push 0xcfdf18      ; "anId", type 1 (uint32), size 4
00b83103  call 0x989e40      ; write field
00b83121  mov  eax, 0xcfdf0c ; "aPosition"
00b83126  call 0x923db0      ; write vec3
00b8312d  call 0x923a70      ; end message
```

`ChangeHonors`, sender at VA `0x00b82080`:

```asm
00b82094  push 0xcfdc08      ; "ChangeHonors"
00b820ba  push 0xcfea9c      ; "aDelta", type 2 (float32), size 4
00b820c5  call 0x989e40
00b820ca  call 0x923a70
```

So on the wire:

| Message | Fields | Notes |
|---|---|---|
| `SupportThingUsed` | `anId` u32, `aPosition` vec3 | **no player, no cost, no bundle count** |
| `ChangeHonors` | `aDelta` f32 | **no player** |

The vector writer at `0x923db0` emits three derived field names. Confirmed by hash:
`adler32("aPosition.x") = 0x1a53045d`, `.y = 0x1a54045e`, `.z = 0x1a55045f` — exactly
the three previously unresolved hashes following `anId`.

Concrete layout, `4600.wicdemo` at decompressed offset 10,361,869:

```text
10361869  03 02 b5 05 15 00 00 00 06 | 59 00 00 00 | f2 65 01 44 | 89 06 12 38
          Event envelope header        len=89        t=517.593s    SupportThingUsed
10361890  7d 01 c8 03 11 00 00 00 01 11 00 00 00 | 65 06 07 3b   anId  = 0x3b070665
10361907  5d 04 53 1a … df 35 df 43                              aPosition.x = 446.42
10361924  5e 04 54 1a … f2 5e 02 42                              aPosition.y =  32.59
10361941  5f 04 55 1a … a4 7b e2 43                              aPosition.z = 452.96
```

immediately preceded by a 38-byte `ChangeHonors` envelope at 10,361,831 carrying
`aDelta = -80.0`.

Because a `ChangeHonors` envelope is always exactly 38 bytes (21-byte header plus
one field), the pairing is a fixed-offset check from the `SupportThingUsed` message
hash — no backward chain walking is needed in the parser.

## Observation 3 — two distinct shapes of `SupportThingUsed`

| Shape | `aPosition` | Preceded by | Meaning |
|---|---|---|---|
| Catalogue | exactly `(0, 0, 0)` | `StartGameTime`, then itself | Enumerates the supports the match makes available |
| Activation | real world position | `ChangeHonors` with a negative `aDelta` | A real tactical-aid use |

In `4600.wicdemo`: 196 `SupportThingUsed` envelopes = 180 catalogue entries emitted
in one burst at t=54.816 s, plus 16 activations.

The two classifications are independent — one looks at the position, the other at
whether a deduction precedes the record — and across 2,879 replays they agree
without a single exception:

- 42,344 of 42,344 activations were preceded by a `ChangeHonors`, and the census of
  preceding messages contains **only** `ChangeHonors` (`0x1d8204c0`). Nothing else
  ever immediately precedes a positioned `SupportThingUsed`.
- **0** catalogue entries were ever priced.

Catalogue size is per-match rather than fixed, and the distribution is itself a
consistency check:

| Catalogue entries | Replays | Reading |
|---|---|---|
| 0 | 427 | Recording began after the start-of-match burst |
| 180 | 697 | One match |
| 201 | 1,714 | One match, larger support set |
| 360 | 18 | Two bursts — 2 × 180 |
| 402 | 23 | Two bursts — 2 × 201 |

The two-burst counts being exact doubles is what one expects from a file holding two
matches, and is further evidence the burst is a per-match enumeration rather than
anything activation-like. The union across the corpus is 200 distinct support IDs.

## Observation 4 — player attribution is provable, but only for the recorder

The acting player is not serialized and is not recoverable from a sender or object
context: both senders acquire the same writer channel (`push 0; call 0xb80880`), and
so does `ShowPlayerGiveTANotification`, so the channel argument carries no identity.

Attribution instead follows from `ChangeHonors` being the **recording player's own
honors ledger**. `ShowPlayerGiveTANotification` does carry `aFromSlot`, `aToSlot` and
`aNumTa`, which makes this directly testable:

`demo01.wicdemo`, recorder slot 8 — every gift where the recorder is the receiver is
immediately preceded by a `ChangeHonors` of exactly `+aNumTa`:

```text
t=199.23  give from=5 to=8 num=1  [receiver]  nearby ChangeHonors = [(-1, +1.0)]
t=366.77  give from=5 to=8 num=5  [receiver]  nearby ChangeHonors = [(-1, +5.0)]
t=473.35  give from=4 to=9 num=5  [other]     nearby ChangeHonors = []
```

`4600.wicdemo`, recorder slot 15 — all 12 gifts are between other players, and not
one has any nearby `ChangeHonors`.

Run corpus-wide over 99,960 gift notifications, the separation is unambiguous:

| Gift role relative to the recorder | Gifts | Matching `ChangeHonors` within ±4 envelopes |
|---|---|---|
| Recorder is the receiver | 37,447 | 37,447 — **100%** |
| Recorder is the sender | 18,217 | 18,217 — **100%** |
| Between two other players | 44,296 | 254 — **0.57%**, consistent with coincidence |

Two consequences:

1. A negative `ChangeHonors` immediately followed by a positioned `SupportThingUsed`
   is **the recorder's own purchase**. Attribution is proven, not guessed.
2. Other players' `SupportThingUsed` purchase records are not present. This limits
   the exact purchase ledger to the recorder, but it does **not** mean other tactical-
   aid effects are absent: later research found visible-faction `SupportThingMarker`,
   `SupportThingSpawnedDelayed`, feedback, and support-projectile records. Those
   effects retain support, position, team, and object identifiers; critically,
   `SupportThingMarker` also serializes the issuing player slot in its nominal
   `aTeam` field. See
   `findings/timeline-player-attribution-2026-08-17.md` for the correction and corpus
   measurements.

The 100% match on the *sender* row also shows why the parser cannot key on the
deduction alone: giving tactical aid away produces a **negative** `ChangeHonors`
that is not a purchase. 18,217 of them exist in this corpus. Requiring a positioned
`SupportThingUsed` to follow is what separates the two, and that guard is load-
bearing rather than defensive.

## Observation 5 — costs are marginal multi-strike prices; bundle selection is absent

`4600.wicdemo`, all 16 activations by slot 15 (`[-->].CrEativE.`):

| t (s) | supportId | cost | position |
|---|---|---|---|
| 244.5 | `8ae20a16` | 14 | 483, 33, 465 |
| 462.6 | `3b070665` (`TacticalNuke_USSR`) | 80 | 446, 33, 453 |
| 504.0 | `3b070665` (`TacticalNuke_USSR`) | 60 | 341, 41, 421 |
| 631.6 | `2fe505d0` | 6 | 258, 35, 787 |
| 667.0 | `8ae20a16` | 14 | 482, 37, 398 |
| 669.4 | `8ae20a16` | 12 | 358, 40, 364 |
| 738.2 | `526b07c3` | 25 | 706, 27, 622 |
| 778.3 | `2fe505d0` | 6 | 784, 30, 485 |
| 965.7 | `3b070665` (`TacticalNuke_USSR`) | 80 | 340, 40, 395 |
| 1031.4 | `8ae20a16` | 14 | 306, 39, 316 |
| 1060.0 | `2fe505d0` | 6 | 500, 32, 379 |
| 1098.6 | `8ae20a16` | 14 | 454, 37, 373 |
| 1147.9 | `353d060f` | 10 | 513, 31, 637 |
| 1149.8 | `353d060f` | 8 | 332, 38, 296 |
| 1159.7 | `2fe505d0` | 6 | 510, 38, 410 |
| 1189.2 | `8a8e0a11` | 5 | 453, 28, 443 |

**Cost cannot name a support.** The same `3b070665` is charged 80, then 60, then 80
again. `8ae20a16` is charged 14, 14, 12, 14, 14. Any cost-to-name table would be
wrong on its own data. This part stands.

**The costs are marginal multi-strike prices.** Player ground truth, supplied by the
user from 15+ years of play, is that a single nuke costs 80 TA, a double 140 and a
triple 180; a single carpet bombing costs 45, a double 75 and a triple 95; and that
the large tactical aids get a reduction on the second and third strike while small
ones such as light artillery (5/10/15) and tank buster (6/12/18) stay linear. The
corpus cost families reproduce that exactly:

| Family | Marginal costs observed | Cumulative | Player-stated bundle price |
|---|---|---|---|
| Nuke | 80, 60, 40 | 80 / 140 / 180 | 80 / 140 / 180 |
| Carpet bombing | 45, 30, 20 | 45 / 75 / 95 | 45 / 75 / 95 |

The deduction happens **per placement**, not once per purchase, which is why one
multi-strike appears as two or three `SupportThingUsed` records with descending
costs rather than a single record at the bundle price.

> An earlier revision of this report claimed the bundle hypothesis was *refuted*,
> on the grounds that a nuke charged 80 and one charged 60 were 41.4 s apart at
> different map positions. That reasoning was wrong: separate placements at
> separate positions are exactly what a multi-strike produces, and the 41.4 s gap
> is well inside the observed distribution. The claim is retracted.

Three independent checks confirm the structure.

**The ladder is never skipped.** Across the corpus, every reduced-price nuke or
carpet use directly follows the rung above it — 281 of 281 nuke steps and 75 of 75
carpet steps. A 60 never appears without an 80 before it; a 40 never without a 60.

**Bundle counts are internally consistent.** Nukes: 555 first strikes, 175 seconds,
106 thirds — so 380 singles, 69 doubles, 106 triples. Charging those at the player-
stated bundle prices gives `380×80 + 69×140 + 106×180 = 59,140` TA, which equals the
sum of the marginal deductions actually recorded, `555×80 + 175×60 + 106×40`.

**Nothing ever exceeds a triple.** Grouping consecutive same-support uses whose cost
strictly decreases yields 39,091 singles, 1,418 doubles and 139 triples across
40,648 bundles, and **zero** bundles of four or more. That is the falsifiable form
of the player-stated max-triple rule, and it holds without exception.

These are corpus-level consistency checks for the supplied price rules. They do not
make bundle identity a parsed replay fact: partial recordings can omit an opener,
equal-cost aids remain ambiguous, and a selected remainder may never be placed.

### Why timing cannot recover linear-priced multi-strikes

For light artillery, tank buster and similar, all strikes in a bundle cost the same,
so cost cannot separate a triple from three singles. Gaps between consecutive
equal-cost uses of the same support are sharply bimodal in this corpus:

```text
p30 =   1.0s      0-1s  ########################################   8221
p40 =   1.6s      1-2s  ########################################   4565
p50 =   2.8s      2-3s  #######################                    1788
p60 =  89.1s      3-4s  ###########                                 840
p70 = 113.6s      4-5s  #####                                       387
p80 = 155.8s      5-6s  ##                                          204
                  6-8s  #                                           132
                 8-20s                                               44
                20-89s                                                1
```

That distribution describes player behaviour, not a serialization rule. A queued
second or third placement can be held indefinitely, while two independent calls can
land close together. Timing therefore cannot establish a bundle and is not used by
the parser. Replays containing repeated recorded segments and late-start recordings
introduce further ambiguity.

### Why prices differ: role pricing, and Few Player Mode

Three pricing shapes exist, and the user supplied the rules for all of them.

**Bulk discount.** The large aids get cheaper per extra strike, and their prices do
not depend on role — nuke and carpet opened at a single price in **all 664**
replay/support pairs where they appear (499 nuke, 165 carpet).

| Aid | Cumulative | Marginal | Corpus evidence |
|---|---|---|---|
| Nuke | 80 / 140 / 180 | 80, 60, 40 | 4 IDs, names proven from `wic.exe` |
| Carpet bombing | 45 / 75 / 95 | 45, 30, 20 | 3 IDs |
| Daisy cutter (USA/NATO) and fuel air bomb (USSR) | 30 / 50 / 70 | 30, **20, 20** | `0x34f6061e` and `0x290c0579`, which open only at 30 and never at 20 |

The daisy cutter ladder goes **flat** after the first step. That pattern is useful
research evidence, but it is not enough for a canonical bundle field: a replay may
begin after an opening placement, and the parser does not have an authoritative
per-role price table for most raw support IDs.

**Linear, priced per role.** Most aids simply cost double for a double and triple
for a triple, but many charge a different base depending on the player's current
role. Unit drops are the clearest case, and the pattern is not simply "armor is
cheapest" — each aid has its own table:

| Aid | Infantry | Armor | Air | Support |
|---|---|---|---|---|
| Airborne infantry drop | 5 / 10 / 15 | 8 / 16 / 24 | 10 / 20 / 30 | 10 / 20 / 30 |
| Light tank drop | 10 / 20 / 30 | 8 / 16 / 24 | 10 / 20 / 30 | 10 / 20 / 30 |
| Air-to-air strike | 12 | 15 | 10 | 10 |
| Bridge repair | — | — | — | cheaper than others |

Role pricing is therefore not confined to unit drops. It is another reason raw costs
must be preserved without trying to group placements in the parser.

This is what produced the "unexplained" mid-match price changes. `2a640597` is a
light tank drop, and in `1168__demo216.wicdemo` the recorder used it twice at 10
while playing **air**, switched to **armor** at t=319.3, and paid 8 for every use
afterwards:

```text
t=  297.4  2a640597  cost 10          role = air
t=  298.0  2a640597  cost 10
t=  319.3  ---- role change -> armor ----
t=  452.6  2a640597  cost 8           role = armor
t=  453.3  2a640597  cost 8
t=  608.8  2a640597  cost 8
```

All three corpus cases that broke the earlier model are role changes. `368a063c` is
an airborne infantry drop, which is exactly why it appears at 5, 8 and 10 across
replays — those are the infantry, armor and air/support prices.

**Few Player Mode.** FPM has no roles, so prices are uniform, but select aids get
*more* expensive in bulk: airborne drops cost 6 / 18 / 24 cumulative, i.e. marginal
**6, 12, 6**. That is precisely the cycle seen in `1217__1_vs_1fpm.wicdemo`, where
`368a063c` repeats 6, 12, 6 while nukes in the same replay keep their ordinary
80/60/40.

**FPM is detectable exactly, from the replay itself.** Because the mode has no roles
it assigns a sentinel one, `FPM_ROLE` = `adler32("FPM_ROLE")` = `0x0b230275`, and
`PlayerSetRole` carries it. In `1217__1_vs_1fpm.wicdemo` that is the *only* role ever
set; `1168__demo216.wicdemo` and `4600.wicdemo` use exclusively the four ordinary
roles and never emit it. The parser now names it `fewPlayer`, so a consumer can
identify an FPM match without guessing from a filename.

A rising marginal price also defeats any generic descending-cost grouping rule.
Schema version 3 avoids the problem by emitting the FPM placements exactly as
recorded, without bundle metadata.

Ruled out along the way: role *levels*. A player's role never levels up and tactical
aid never becomes cheaper over time — only units gain experience, which improves
their accuracy and rate of fire, not prices.

### Cost attribution is sound

Because these anomalies could equally have come from mis-reading the cost, the
pairing was re-checked directly: a placement is preceded by **exactly one**
`ChangeHonors` envelope, never two. No purchase splits its deduction across several
records, so the parser is not under-reading any cost.

## Observation 6 — support IDs are name hashes; four names are proven

`anId` is Adler-32 of the support definition's name. Four resolve directly against
`wic.exe`'s string table:

| ID | Name |
|---|---|
| `0x2e8f05c0` | `TacticalNuke_US` |
| `0x3b070665` | `TacticalNuke_USSR` |
| `0x3ab4064a` | `TacticalNuke_NATO` |
| `0x7adc097e` | `TacticalNuke_NATO_British` |

These are exactly the IDs the earlier corpus scan grouped at cost 80, which
independently corroborates both the hash relationship and the user-supplied
"one nuke costs 80 TA".

The remaining catalogue IDs do **not** resolve. Their names are not present as
plaintext in any of the three executables, nor in the packed `.sdf` archives
(`grep` for `TacticalNuke` across all 26 `.sdf` files returns nothing, so the
archives are compressed). Substring-level hashing of every identifier run in the
binaries produced only coincidental collisions. **Naming the other supports
requires unpacking the `.sdf` container format** — a separate piece of work. Until
then those IDs stay raw.

## Parser change

`TimelineEvent::TacticalAidUsed` was introduced in timeline schema version 2.
Version 3 removes the inferred bundle fields and retains only the parsed placement:

```typescript
{
  type: 'tacticalAidUsed'
  timeSeconds: number
  supportId: number
  supportName: string | null   // only the four proven names
  honorsCost: number           // marginal cost of this placement
  position: [number, number, number]
  playerId: number | null      // the recorder's slot, when known
}
```

Deliberately conservative:

- The event is emitted only when the paired negative `ChangeHonors` is present, so
  cost and activation are always one fact rather than two correlated guesses.
- Unpriced zero-position catalogue entries are dropped by the deduction requirement;
  a priced activation at `(0, 0, 0)` remains valid.
- A positive `ChangeHonors` never produces an event, so a tactical-aid gift to the
  recorder cannot be misread as a purchase.
- No name is derived from cost.
- `playerId` is the recorder slot, which is evidence-backed, and is `null` for the
  ~2% of replays whose POV slot is not recoverable.
- No single/double/triple grouping is emitted. Each event means exactly one observed
  placement.

`ReplayData`, `parse()`, `parse_replay_wasm()` and every schema-version-1 event are
unchanged. Version 2 moved for the new event variant; version 3 moves because it
removes bundle fields whose apparent precision was not supported by the format.

### Why bundle reconstruction was removed

A player selects single, double or triple before launching the first strike and may
hold later placements indefinitely. The replay does not serialize that selection.
Equal-cost placements cannot be grouped at all, and even a discounted sequence can
start before a late recording begins. Schema version 2 used timing and price
heuristics; schema version 3 removes them from the canonical parser output. The raw
cost sequence remains available for explicitly derived research outside the parser.

## Corpus results

### Record-level audit

`scripts/ta-usage-audit.py` reads the envelope chain directly and does not apply the
parser's acceptance rules, so it measures the format rather than the parser:

| | |
|---|---|
| Replays analysed | 2,879 |
| Replays failed | 1 (zero-byte file) |
| Activations found | 42,344 |
| Activations preceded by `ChangeHonors` | **42,344 — 100%** |
| Other messages ever seen preceding an activation | **none** |
| Catalogue entries priced | **0** |
| Activations with a non-negative deduction | **0** |

### Parser-level run

`scripts/timeline-corpus-check.py` over every deduplicated replay under
`replays/main`, `replays/settings` and `replays/wicgate-documents` — 2,880 files:

| | |
|---|---|
| Replays parsed | 2,865 |
| Replays rejected | 15 (established corrupt fixtures plus a zero-byte file in a read-only document mirror) |
| Timeline schema version | 3 on all 2,865 |
| Tactical-aid placements emitted | 42,288 |
| Activations with no resolved `playerId` | **0** |
| Activations timestamped at or before zero | **0** |
| Activations past the observed duration | **0** |
| Priced activations at `(0, 0, 0)` | **0** in this corpus; parser accepts them |
| Distinct support IDs activated | 54 |
| Support IDs charged more than one cost | **42 of 54** |

The placement total is unchanged from schema version 2; only the unsupported bundle
metadata was removed. Consumers can display and count observed placements without
mistaking a heuristic grouping for replay data.

Coverage by mode — the corpus is Domination-dominated, so Assault and Tug of War
are reported separately rather than folded in:

| Mode | Replays | Replays with activations | Activations |
|---|---|---|---|
| Domination | 2,784 | 2,411 | 41,505 |
| Assault | 43 | 39 | 478 |
| Tug of War | 12 | 9 | 70 |
| Unknown map prefix | 26 | 21 | 235 |

That 42-of-54 figure is the corpus-scale form of the cost argument: for most
supports, cost is not a function of identity, so no cost-to-name mapping can be
correct.

The two runs reconcile exactly. The audit's 42,344 minus the parser's 42,288 is 56,
and all 56 sit in the 14 replays the parser rejects as corrupt but the deliberately
lenient audit still reads. No activation was dropped by the parser's gameplay-start
filter, and none was dropped for a non-negative deduction.

Nuke family, corpus-wide, corroborating the binary-recovered names and the
user-supplied "one nuke costs 80":

| ID | Name | cost 80 | cost 60 | cost 40 |
|---|---|---|---|---|
| `0x3b070665` | `TacticalNuke_USSR` | 292 | 96 | 62 |
| `0x2e8f05c0` | `TacticalNuke_US` | 206 | 63 | 34 |
| `0x7adc097e` | `TacticalNuke_NATO_British` | 53 | 14 | 10 |

`0x3ab4064a` (`TacticalNuke_NATO`) appears in match catalogues but was never
activated in this corpus, which is why the earlier scan found only three IDs.

## Validation

- Rust unit tests: 23 passed, 0 failed. Tactical-aid coverage includes the paired
  deduction, unpriced catalogue records, a priced world-origin activation, recorder
  attribution, missing/positive deductions, malformed envelopes and fields,
  per-placement marginal costs, partial discounted sequences, arbitrary gaps
  between equal-cost placements, and unproven IDs staying unnamed.
- Clippy, all targets and features, warnings denied: passed.
- `cargo fmt --check`: clean.
- Feature-gated builds (`--no-default-features` with `cli`, then `wasm`): passed.
- `wasm-pack build --target web`: passed.
- Python ground-truth suite against its real private corpus root: 16 passed,
  0 failed, 0 errors, 0 skipped.
- Ruff lint and format on the three new scripts: clean.
- Full timeline corpus check: 2,865 accepted schema-version-3 replays, 15 established
  rejects, and 42,288 raw tactical-aid placements retained.

## Remaining limits and open work

- **The purchase ledger covers one player, but markers cover the match.**
  `SupportThingUsed` still shows only the recorder's purchases. Later binary and
  corpus validation established that visible-faction `SupportThingMarker` records
  carry the issuing player slot, so per-player marker totals can be derived from one
  replay. Spawn, feedback, and projectile ownership still needs a validated join.
- **Support-name gap — resolved later.** At the time of this pass, 195 of roughly
  200 support IDs were unnamed. The subsequent bounded SDF extractor recovered all
  200 catalogue definitions with no missing hashes or collisions; 58 proven
  top-level faction aids now receive parser names while child effects and special
  abilities remain deliberately unnamed.
- **Selected bundle size remains unknown.** The parser emits observed placements,
  not single/double/triple calls. This is correct for linear prices, held strikes,
  partial recordings, role-priced aids, and Few Player Mode, but it means call-level
  totals cannot be derived reliably from the current record set.
- Timeline timestamps remain countdown-derived observed gameplay time. The `Event`
  envelope's own float timestamp was discovered during this pass and is a plausible
  future upgrade, but was deliberately not adopted here to avoid mixing two time
  bases inside one schema.
- The Ghidra project at `ghidra/WiCProject.gpr` was locked by a `pyghidra-mcp`
  process left running from a prior session (started 2026-08-14 18:32), so binary
  work was done with `pefile`/`capstone` static disassembly instead. No Ghidra
  writer was started and the lock was not disturbed.
