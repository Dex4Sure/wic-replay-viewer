# Does the unlabelled server-mode bucket mean Ranked? — 2026-08-22

## Question

The classifier emits FPM, Match Mode, clan match, tournament match, and bots from
positive replay evidence, and shows no label when none of those fire. The working
hypothesis under test is that this all-clear bucket is exactly the ranked public
games, and that the viewer could therefore label it `Ranked`.

Short answer: **the hypothesis holds.** No replay-side bit says so — the demo header
does not serialize `[RankedFlag]` — but the residual left after the five positively
detected modes is ranked games, and the corpus contradicts that nowhere. The
inference is an elimination argument closed by operator ground truth, so it belongs
in the viewer's display layer, not in the parser's contract: keep
`serverClassification.ranked` null and label the bucket in the viewer.

Sources: `binaries/game/wic_ds.exe` and `binaries/game/wic.exe` (32-bit PE, image
base `0x00400000`), read only, and the 2,880-path linked replay corpus.
Reproduced by `scripts/ranked_bucket_audit.py`; the per-replay rows are in the
ignored `findings/ranked-bucket-audit.json`.

## What Ranked actually is, server-side

`EXD_DedicatedServer` reads the flags into its settings object at these offsets
(config reader `FUN_00405da0`):

| Offset | Setting | Read at |
|---|---|---|
| `0x26c` | `[TournamentFlag]` | `0x0040633c` |
| `0x26d` | `[ClanMatchFlag]` | `0x00406375` |
| `0x26f` | `[ReportToMassgate]` | `0x004062dc` |
| `0x270` | `[RankedFlag]` | `0x0040630c` |

Two rules follow immediately from the reader itself:

- `0x00406384`: when `[ReportToMassgate]` is 0 the server **clears** RankedFlag,
  TournamentFlag, and ClanMatchFlag. An unlisted server cannot be ranked.
- `0x00406365`: a tournament server clears the clan-match flag.

The ranked coercion block begins at `0x0040721f` and runs only when
`RankedFlag != 0` **and** TournamentFlag and ClanMatchFlag are both clear. Inside
it the server rewrites its own configuration:

| Address | Coercion |
|---|---|
| `0x004072e3` | `[ModName]` forced off — "Ranked game, forcing mod to 'no'" |
| `0x00407320` | `[MinPlayers]` raised to 3 |
| `0x0040735d` | `[MinPlayers]` lowered to 5 |
| `0x00407395` | `[TimeLimitMultiplier]` forced to 1.0 |
| `0x004073e4` | `[Password]` forced off |

Bots are refused separately: `Ranked game, forcing bot mode to none` and, at
runtime, `BOTS DETECTED IN RANKED GAME. NO STATS WILL BE REPORTED`.

The start-up validator `FUN_00404cc0` adds the exclusions. FPM "cannot be combined
with tournament or clan match mode or auto-team balancing or be ranked" — the FPM
branch tests the ranked byte at `0x2fd8` of the game settings directly. Clan and
tournament servers each "require match mode" and exclude each other, FPM,
auto-balancing, and bots. Note what the FPM branch does *not* test: bots. That is
the structural reason the FPM-plus-bots replay `demo82.wicdemo` is a legal
configuration rather than a corrupt one.

**Ranked is orthogonal to Match Mode.** No validator forbids the combination, and
the coercion block deliberately steps aside for clan and tournament servers rather
than rejecting them. A ranked clan-match server is a legal configuration, so the
labelled buckets are not "not ranked".

## Why no replay can carry the answer

Two independent enumerations close this from the writer side.

The demo header written at `wic.exe` VA `0x00b87880` is fully enumerated by scanning
the metadata chunk of all 2,880 corpus paths. Every replay carries exactly this
prefix, in this order:

```text
RecordingPlayerSlot, myVersion, myFPMModeFlag, myMatchModeFlag,
myIsTournamentMatchFlag, myIsClanMatchFlag, <float>, FileVersion
```

The unnamed float tracks the recording length (1278.07 against a parsed
`recordingSeconds` of 1277.887 in `replays/main/4600.wicdemo`), not a mode. Across
the whole corpus the metadata chunk contains 98 distinct BinTag field hashes, and
the only per-replay-unique ones are those eight plus the roster records. There is no
spare boolean.

The in-stream side is closed by the complete message inventory
(`findings/bintag-message-inventory-2026-08-22.md`): 169 messages, 0 unresolved,
and none carries a ranked, ladder, Massgate, or report field.

Ranked exists on the wire — `wic_ds.exe:0x005522c0` passes the byte at settings
offset `0x270` into the Massgate registration path, `0x0055e320` serializes it into
server information, and the client keeps it as bit 0 of `0xc6` in server details for
`myGames.ServerDetails_IsRanked` at `0x008605e0` — but the client demo writer copies
none of it.

## What the unlabelled bucket contains

2,880 paths resolve to 2,497 unique replays; 8 are the known empty files.

| Bucket | Replays |
|---|---|
| Match Mode | 937 |
| Clan match | 931 |
| **Unlabelled** | **573** |
| FPM | 31 |
| Bots | 17 |

Every ranked prerequisite that *is* observable holds across the unlabelled bucket:

- No FPM, clan, or tournament flag, and no bot participant, by construction.
- Round length is the default everywhere: 482 of 483 Domination replays at 1200 s
  (one at 1198 s), all 16 Assault at 600 s, all 5 Tug of War at 1200 s. No replay in
  any bucket shows a non-default `[TimeLimitMultiplier]`, so this passes as a
  necessary condition without discriminating.
- The matchups are public-server shaped: 8vs8 is the single largest matchup (308 of
  573) and 345 replays are full 16-player games, against a Match Mode bucket whose
  most common matchup is 1vs1.

The server-name evidence is where the hypothesis gets its strength. Of 661 distinct
server names, exactly **one** appears in both the unlabelled bucket and a labelled
one (`+TNT+ Elite Ranked Server`: 5 Match Mode, 1 unlabelled). No unlabelled replay
comes from a server calling itself unranked; the corpus's two self-declared unranked
servers (`Multiplay.co.uk :: D.N Unranked Practice` and its longer spelling) sit in
the **Match Mode** bucket, all 11 replays.

Split by era, the picture separates sharply:

| Era | Unlabelled replays | Server name says "Ranked" |
|---|---|---|
| 2023 and later | 189 | **189 (100%)** |
| 2007–2012 | 384 | 130 (34%) |

In the revived-Massgate era the hypothesis holds without a single exception: all 189
unlabelled replays come from seven servers, every one of them a ranked public server
(`WiCGate Ranked Public Server` and its two variants, `[US-01] Massgate Ranked …`
×2, `JMann's ARENA Ranked 8v8`, `Winter NEXUS Ranked Public Server`). The same era's
labelled buckets contain no ranked-named server at all.

The legacy 384 carry less self-evidence: they are all ordinary public servers —
map-rotation pubs, clan pubservers, `ngz-server.de` and `Multiplay.co.uk` rentals —
but only a third advertise ranked in the name, so for the rest the conclusion rests on
the operator ground truth recorded below rather than on anything in the file. Four are
community-map games (`do_WakeBeta2`, `do_Airport3` ×2, `do_Countryside` on a "Custom
Maps" server); `AWARNING: Map has been blacklisted from massgate: %s` shows the server
carries a per-map eligibility byte, so those rounds may not have reported stats even
from a ranked server. The exact blacklist condition was not traced.

## Conclusion

Read as an evidence statement, the unlabelled bucket is **public-server games with no
ready-gating**: no FPM, no clan or tournament subtype, no bots. Every enforced ranked
prerequisite is either invisible in the replay or satisfied by every member of the
bucket, so `ranked ⊆ unlabelled` is sound from the binary alone. The hypothesis needs
the converse, and two candidate members of the residual had to be ruled out before it
could be accepted.

**Unranked public servers.** The dedicated server permits the configuration —
`[ReportToMassgate]=1` with `[RankedFlag]=0`, no ready gate, no bots — and it writes
a byte-identical header. It is excluded by operator ground truth rather than by the
binary: in community practice "pub" meant a ranked server game, and unranked public
servers were not a category that was actually run. This is the same class of evidence
the 2026-08-21 pass already relies on for FPM-plus-bots being a legal configuration.

**Client-hosted (listen server) games.** These cannot be ranked — Massgate approves
ranked servers separately (`your server claims that it is ranked but massgate don't
think it is`) — and nothing in the replay distinguishes one from a dedicated server.
The header has no field for it, none of the 169 client-serializable messages carries
one, a message-set diff against dedicated controls returns only map-dependent
differences, and `isAdmin` is not the marker: the corpus's four default-named replays
carry no admin at all, while 77 unlabelled replays carry one or more, on obviously
rented dedicated servers.

The corpus nevertheless contains no evidence of any. The four replays named
`World in Conflict Game` are **not** listen servers: `WicData.myDefaultGameName` in
`directory.loc` resolves to exactly that string, and the dedicated server falls back
to it through the directory loader (`FUN_0044bdd0` at `0x00405ee0`) when `[GameName]`
is unset. They are unnamed dedicated servers, and all four are in the bots bucket
regardless. The client frontend's own default, `myGameText.myMultiplayerFrontend.myDefaultGameName`
= "New game", appears on no replay in the corpus.

With both retired, the residual is ranked games.

## Handling

- Keep `serverClassification.ranked` null. The bit is genuinely absent from the file,
  and the parser reports evidence, not inference.
- Label the residual `Ranked` in the viewer, with this document cited next to the
  rule. The premise is operator ground truth about how servers were run, so it is
  reversible at one call site if a counterexample ever appears.
- The label must key on the classified bucket, never the raw header. The 17 bot
  replays carry the same four cleared header flags and are separated only by
  `myType=1`; a header-keyed rule would call them ranked, and the server refuses to
  report stats for a bot game.
- The inference runs one way only. Ranked coexists with Match Mode, clan, and
  tournament in the dedicated server's configuration, so the labelled buckets must
  not be presented as unranked.
- The route to replacing inference with proof stays external: a Massgate-side record
  of which servers carried `[RankedFlag]=1`. For the WiCGate-era servers that is
  knowable from their own configuration, which would convert 189 of the 573 from
  inference into ground truth.
