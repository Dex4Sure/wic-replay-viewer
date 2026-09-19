# Complete BinTag message inventory — 2026-08-22

Walking the callers of the message-begin function enumerates every message
`wic.exe` can serialize into a replay. This replaces discovering messages by
scanning replays, and closes the question of whether an undiscovered record
carries the tactical-aid actor.

Target: `binaries/game/wic.exe`, SHA-256
`41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc`, matching
`manifests/targets.sha256`. 32-bit PE, image base `0x00400000`.

## Result

**178 call sites, 169 distinct messages, 0 unresolved.** Names, BinTag hashes, and
complete field lists are in the ignored `findings/bintag-message-inventory.json`.

## The writer confirmed

`FUN_009240a0(writer, name)` decompiles to exactly the envelope the parser reads:

```c
local_10 = now - writer->startTime;        // gameplay timestamp
local_c  = (float)FUN_00a03830(name);      // Adler-32 of the message name
FUN_00989e40("Event", 6, &local_10, 8);    // flag 6, 8 bytes
```

That is `[Event][0x15000000][0x06][total][time f32][messageHash u32]`, field for
field. It also independently confirms timeline schema v13: the serialized
timestamp is **elapsed time from a writer origin**, not the game-mode countdown.

Two field writers append the body. `FUN_00989e40(name, type, &value, size)` is
cdecl and writes a scalar. `FUN_00923db0` writes a vector, taking its base name in
**EAX** and formatting `%s.x`, `%s.y`, `%s.z` from `0x00d456e0` — which is why a
stack-only scan misses position and direction fields.

## Messages are rate-limited before they are written

`FUN_009240a0` looks the message name's hash up in a per-writer tree
(`node[2]` key, `node[0]` minimum interval, `node[1]` last emit time). When
`now < lastEmit + interval` it sets the suppression flag at `writer+0x1028` and
returns **without writing**. Every subsequent field write in every caller is
guarded by `if (writer[0x1028] == 0)`. `UnitFrame` is special-cased onto a
per-slot interval array at `writer+0x28`.

This is the structural explanation for partial event coverage. It is a
serialization-side throttle, not a visibility limit, so a deployment dropped by
the throttle is absent from **every** recording and no additional point of view
recovers it. It matches the observed marker sparsity: reference replay `697`
carries 35 markers for 63 deployments.

## The actor question is closed

Of the 169 messages, 24 carry a player-identity field. Exactly **three** carry
both a player and a tactical-aid identity, and the parser already consumes all
three:

| Message | Hash | Fields | Why it is not a general actor bridge |
|---|---|---|---|
| `SupportThingMarker` | `0x461f075a` | `anEventId, anId, aPosition.x/.y/.z, aTeam, aSupportUppgradeLevel, aDirection.x/.y/.z, aTime` | `aTeam` is the issuing player slot, but the stream follows one faction |
| `SendTATaunt` | `0x1835042c` | `aPlayerFrom, aPlayerTaunted, aTAId, aSupportUpgradeLevel` | Damage notification with no position, so it cannot be joined to a deployment |
| `ShowPlayerGiveTANotification` | `0x9f480b16` | `aFromSlot, aToSlot, aNumTA` | Tactical-aid gifting, not a deployment |

The global effect streams are player-free at the source:

- `SupportThingSpawnedDelayed` `0x8f360a82` — `anId, aPosition.x/.y/.z, aTeam, aSupportUppgradeLevel, aTimeSinceCreation`
- `SupportThingFeedback` `0x553807fd` — `anId, aTeam`
- `SupportThingUsed` `0x38120689` — `anId, aPosition.x/.y/.z`

The four support-projectile creators (`ProjectileBallisticSupportCreate`,
`ProjectileStraightSupportCreate`, `ProjectileHomingSupportCreate_Position`,
`ProjectileHomingSupportCreate_Unit`) carry `aSupportThing`, positions, vectors,
and upgrade level, but **no player**. This confirms the projectile-bridge
rejection in `opposing-player-tactical-aid-attribution-2026-08-18.md` from the
writer side rather than by corpus statistics.

Three `SupportThing` messages were previously unknown to the parser. None helps:
`SupportThingReady` `0x3ec106ed` and `SupportThingNotUsed` `0x4dfa07ba` carry a
single `anId`, and `SupportThingMarkerStopped` `0x84fb0a39` carries a single
`anEventId`.

**Conclusion.** No undiscovered player-bearing tactical-aid deployment record
exists in the client writer. Multi-POV alignment
(`multi-pov-tactical-aid-attribution-2026-08-22.md`) remains the only route to the
opposing team's actors, and the throttle bounds even that.

## Corrections and confirmations for the parser

- The countdown message is `SetGameModeData_Float`, **with an underscore**. The
  literal `SetGameModeDataFloat` does not hash to the observed `0x569107fb`; the
  underscored name does. Prior notes used the wrong spelling for the same hash.
- `UpdateBalanceFactor` `0x49160769` names the domination parent event that
  `domination-bar-and-match-timing.md` previously identified only by hash.
- `CameraPosition` `0x2894059f` is the message that opens the end-of-match summary
  pass, matching the envelope-clock reset the parser cuts on.
- All 19 message-name hash constants in the Rust parser resolve to a real message
  in the inventory, with no mismatch.

## Coverage

The parser consumes 19 of 169 messages, plus `UpdateBalanceFactor` through its
`aFactor` field hash. The remainder are unit, mover, shooter, building,
projectile, objective, and UI records outside the current schema. Two carry
player identity and may be worth a later look for non-TA attribution work:
`SpawnerDeployed`/`SpawnerSetPosition` (`aPlayerID`) and
`DeployableCreate`/`UnitSetOwner` (`playerId`).

## Reproduction

```bash
scripts/bintag_message_inventory.py binaries/game/wic.exe \
  --json findings/bintag-message-inventory.json
```

The ignored detailed inventory is `findings/bintag-message-inventory.json`,
SHA-256 `c0f4a8965ed3b58aa5a2b6d2d51e8283eb24cf7aad9ec3484ef0eac113e85dff`.

The scan reads the PE directly and never opens the Ghidra project, so it is safe
to run alongside an MCP or headless session. Field and writer semantics above were
read from Ghidra decompilation of `0x009240a0`, `0x00923db0`, `0x00b86e70`,
`0x00b83000`, `0x00b83060`, and `0x00b86e10`.
