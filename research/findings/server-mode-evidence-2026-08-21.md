# Replay server-mode evidence — 2026-08-21

## Scope

This investigation distinguishes the map/objective `gameMode` (Domination,
Assault, or Tug of War) from the server-level `serverMode` requested for Ranked,
Match Mode, Few Player Mode (FPM), and Bots. Source replays and binaries were read
only. `scripts/server_mode_audit.py` records the reproducible replay-side evidence.

## User-confirmed server behavior

The following game behavior is empirical ground truth supplied by an experienced
server operator/player. It guides interpretation but remains distinct from the
serialized and binary-traced facts below:

- FPM and bots are compatible: an FPM game may contain only players or may also
  contain bots.
- FPM servers cannot be Ranked.
- Ranked servers have enforced prerequisites, including no FPM and no bots. Once
  the configured minimum player requirement is met, the game starts immediately.
- Match Mode is used for organized higher-level games and requires every player to
  press ready before the game starts.
- Clan-match servers use the same all-players-ready start behavior.

These rules explain why FPM plus bot-participant evidence is valid rather than a
corrupt configuration. They also show that `myMatchModeFlag` is at least partly a
mechanism flag for ready-gated starts, not by itself a complete user-facing server
category.

## Confirmed demo-header fields

The replay writer at `wic.exe` VA `0x00b87880` writes these fields into
`DemoHeader`, in order:

| Field | Meaning observed in labelled candidates |
|---|---|
| `myFPMModeFlag` | Set in known FPM recordings |
| `myMatchModeFlag` | Set in known Match Mode recordings and also in FPM |
| `myIsTournamentMatchFlag` | Tournament subtype flag |
| `myIsClanMatchFlag` | Clan-match subtype flag |

The corresponding reader is at VA `0x00bd38d0`. Known FPM candidate
`1217__1_vs_1fpm.wicdemo` has FPM=1 and MatchMode=1. Historical Match Mode
candidates such as `4v4 MM Riviera 2010 #3 Armor Rape.wicdemo` have FPM=0 and
MatchMode=1. Ordinary candidates such as `demo01.wicdemo` have both clear.

The independent `FPM_ROLE` event remains corroborating evidence, but the header
flag is preferable for top-level classification because it is present from the
start of the recording.

## Bot evidence

The `PlayerEntersGame` writer at `wic.exe` VA `0x00b86b50` serializes `aSlot`,
team, player name, `readyFlag`, `myType`, role, and `isAdmin`. Human participants
observed in ordinary recordings use `myType=0`. The known bot fixtures
`421__demo01.wicdemo` and `422__demo01.wicdemo` introduce eight players named
`Computer: Balanced Armor` with `myType=1` at the same event timestamp.

The audit therefore reports a Bots signal when any structurally framed roster or
`PlayerEntersGame` record carries `myType=1`. This is positive participant evidence
and does not depend on names or player counts.

The full 2,880-path linked-corpus scan found only player types 0 and 1. It also
found one unique counterexample to a mutually exclusive server-mode enum:
`demo82.wicdemo` (SHA-256
`868a27582f1b3fc82677965eeccb9ff80bf1fce1938a8652757af7ebb6b4fff4`) has both
FPM=1/MatchMode=1 and three `myType=1` participants named by the replay as
Computer players. FPM and Bots must therefore be representable together.

## Ranked boundary

The dedicated-server configuration has two relevant but distinct settings:

- `[ReportToMassgate]=1` enables reporting/advertising the server to Massgate; and
- `[RankedFlag]=1` requests a Massgate ranked game whose statistics may be reported
  to the ladders by an approved server.

`wic_ds.exe` reads both settings in the configuration reader at VA `0x00405da0`.
The recovered canonical `wic_ds.ini` documents them separately. The client demo
writer at VA `0x00b87880`, however, copies neither setting into `DemoHeader`; it
copies only FPM, Match Mode, tournament, and clan-match flags. Direct BinTag searches
for the obvious ReportToMassgate/Ranked field-name variants also found no framed
field in representative ranked-candidate, public-unranked, or Match Mode replays.

The server does propagate Ranked outside the replay stream. The dedicated-server
game-controller update at VA `0x005522c0` passes the byte at configuration offset
`0x270` into its Massgate/server-registration path, and the server-information
writer at VA `0x0055e320` serializes that same byte as a boolean. On the client,
multiplayer server details retain Ranked as bit 0 at offset `0xc6`; the UI reader at
VA `0x008605e0` uses that bit for `myGames.ServerDetails_IsRanked`. This establishes
that Ranked is a real network/server-browser property, but not that it is present
in `.wicdemo`. It also rules out treating `[ReportToMassgate]` alone as Ranked.

An equivalent value could still be propagated under a differently named in-stream
message, so this is an evidence boundary rather than proof that recovery is
impossible. At present, though, all-clear FPM/Match Mode flags cannot safely mean
Ranked: public unranked recordings can have the same replay-visible header. Server
names and filenames are not acceptable substitutes.

Current conservative replay signals are therefore:

```text
myFPMModeFlag=1                       -> fewPlayerMode
myMatchModeFlag=1 and FPM flag clear  -> matchMode
myType=1                              -> bots (independent; can coexist with FPM)
no positive signal                    -> unknown
```

Do not add the requested public `serverMode` as a single enum
`ranked | matchMode | fewPlayerMode | bots | unknown`: Ranked lacks a positive
serialized signal, and the FPM-plus-Bots counterexample proves the known values are
not mutually exclusive. Prefer an object of independently evidenced properties,
for example `ranked`, `matchMode`, `fewPlayerMode`, `hasBots`, and `clanMatch`, with
unknown retained where replay evidence cannot decide a property. Validate the
known incompatibilities (`ranked` with FPM/bots) without using their absence as
positive proof that a replay is Ranked.

## Classifier shape for implementation

Implemented in parser commit `5035295` and viewer commit `a60aa78`. The parser
exposes orthogonal evidence rather than a single server-mode label:

| Property | Replay rule | Confidence when emitted |
|---|---|---|
| `fewPlayerMode` | `DemoHeader.myFPMModeFlag` | exact boolean |
| `matchMode` | Match flag set while FPM flag is clear | exact classification |
| `hasBots` | any participant has `myType=1` | exact positive; false only after the complete roster is parsed |
| `clanMatch` | `DemoHeader.myIsClanMatchFlag` | exact boolean |
| `tournamentMatch` | `DemoHeader.myIsTournamentMatchFlag` | exact boolean |
| `ranked` | no replay-side positive signal yet | `unknown`, never inferred from exclusions |

Preserve the raw Match Mode flag as well as the classified `matchMode` value:
FPM deliberately sets the mechanism flag, so renaming the raw bit to the public
classification would discard useful evidence. The eventual viewer can present
the positive labels together (for example, `FPM` and `Bots`) and omit Unknown;
the parser contract should retain `ranked: null` so absence is not confused with
confirmed unranked status.

The post-implementation audit covered all 2,880 linked paths with zero evidence
conflicts: 2,107 Match Mode paths, 30 FPM-only paths, 28 Bots-only paths, two paths
representing the same unique FPM-plus-Bots replay, and 712 paths without a positive
display classification. The one audit error remains the known zero-byte fixture.
All 16 saved parser ground-truth fixtures passed without skips.
