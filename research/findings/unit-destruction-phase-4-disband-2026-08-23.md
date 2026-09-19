# Player disband attribution investigation

## Scope

Phase 4 investigates the deliberate player action that removes owned units and
returns their value to incoming reinforcement points. The shipped English
localization names this action **DISBAND SELECTED UNITS**. This is distinct from
the scripting `RemovePlayerUnit` command and the infection-related
`EXG_SelfDestruct` gameplay object.

No parser or viewer attribution is changed by this phase.

## Binary path

The client registers the input action `DISBAND_UNIT` at `wic.exe:0x0098cfa0` and
the minimap button `myButton_Disband` at `wic.exe:0x008b09f0`.

The server-side disband handler at `wic_ds.exe:0x004a8ac0`:

1. resolves the requested unit;
2. verifies that the requesting player owns it; and
3. calls `wic_ds.exe:0x004ca3e0` with mode zero.

`wic_ds.exe:0x004ca3e0` starts the shared `myUnitBlinkBeforeDeathTime` path,
broadcasts a unit state transition, recursively applies it to eligible contained
units, and schedules the unit for the later dead/removal state. The same routine is
also called from `wic_ds.exe:0x004d5810` and `wic_ds.exe:0x004d3150` for other
player/unit lifecycle operations. Blink-before-death is therefore not a unique
disband signal.

The client replay writer at `wic.exe:0x00b80c60` serializes `SetState_Unit` with
only `aUnit`, `aStateName`, and `aTransitionTime`. It contains no requesting player,
command reason, refund, or disband flag.

## Replay evidence

The client request parser recognizes `DisbandUnit` as command `0x12f`, but the
replay corpus contains no such request records. A direct exact-hash scan used
Adler-32 `0x193f0456` for `DisbandUnit` and found:

| Metric | Count |
|---|---:|
| Deduplicated replay paths | 2,880 |
| Replays failed | 0 |
| `DisbandUnit` messages | 0 |
| Replays containing the message | 0 |

This is consistent with replays preserving the server-output stream rather than
the client request that caused the server action.

Exploratory lifecycle matching also finds the common `SetState_Unit` state hash
`0x1a0f0472` before both sentinel-512 deaths and deaths with identified killer
units, over a wide range of delays. It cannot distinguish player disband from
combat or the other callers of the shared blink routine.

## Conclusion and controlled-capture boundary

Existing replay fields do not provide an exact disband attribution rule. Neither
the request nor a reason/refund identifier is serialized, and the visible state
transition is shared. Unknown sentinel-512 deaths must not be rewritten as self-
deletions from blink timing alone.

The remaining verification step requires a controlled multiplayer recording with
known disband timestamps and unit IDs, ideally paired with a non-disband invocation
of the shared blink path. Runtime instrumentation may observe the request handler
or `wic_ds.exe:0x004ca3e0`, but attaching to a running process requires explicit
approval under the workspace rules. Even a controlled capture can justify parser
output only if it reveals a replay-visible sequence that survives the negative
controls; runtime-only knowledge cannot add facts absent from arbitrary replays.
