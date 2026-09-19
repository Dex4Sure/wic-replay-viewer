# Unit destruction Phase 8: forced-death callers and the terminal-direction signature

Date: 2026-08-24

## Decision

No new canonical attribution. Phase 8 closes the two bounded binary questions the
Phase 7 report left open and, in doing so, **retires the remaining forced-death
surface as a source of multiplayer unit losses**. It also corrects an implicit
assumption in the Phase 6/7 framing: the exact `0x004cc280` terminal-direction
bit signature is *not* confined to the forced and inherited death helpers, so it
must never be used on its own as a mechanism discriminator.

## Binary identity

| Field | Value |
|---|---|
| Program | `/wic_ds.exe` |
| Architecture | x86 little-endian 32-bit PE |
| File version | `1.0.1.1 (b35)` |
| SHA-256 | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` |

## The fatal-path caller set is provably complete

A raw image scan for pointer-sized references found **zero** occurrences of
`0x004ccef0`, `0x004ce880`, `0x004ce8a0`, `0x004ce310`, or `0x004cc280` anywhere in
`.text`, `.rdata`, `.data`, `.tls`, or `.rsrc`. None of these routines is reachable
through a vtable slot or a function pointer, so Ghidra's direct-call cross-reference
lists are exhaustive rather than partial:

| Routine | Role | Callers |
|---|---|---|
| `0x004ccef0` | fatal routine; writes `UnitDestroy` | exactly 3 |
| `0x004ce310` | ordinary health-damage path | exactly 3 |
| `0x004ce880` | forced death, killer sentinel `0x200` | exactly 6 |
| `0x004ce8a0` | inherited death (killer/source copied) | exactly 2 |
| `0x004cc280` | synthetic terminal direction | exactly 3 |

## All six forced-death callers classified

RTTI complete-object locators recovered from the vtables that hold each caller, plus
the assert/log strings inside them, identify every remaining caller:

| Caller | Identity | Evidence |
|---|---|---|
| `0x004ddf20` | `EXG_Spawner::Spawn()` | `.\EXG_Spawner.cpp` asserts; `EXG_Spawner::Spawn(): failed to spawn unit of type %d` |
| `0x004d37c0` | `EXG_UnitContainer::KillAllUnitsInArea()` | `EXG_UnitContainer::KillAllUnitsInArea() : Killing unit %d` |
| `0x00506bd0` | `EXG_Blower` vtable slot 8 (update) | RTTI `.?AVEXG_Blower@@` at vtable `0x00778540` |
| `0x00536670` | `WICG_FortificationPoint` unfortify | `.\WICG_FortificationPoint.cpp` assert |
| `0x004a6ac0` | `EXG_Game` debug/cheat interface slot 4 | RTTI `.?AVEXG_Game@@` at vtable `0x0076ef1c` |
| `0x00513260` | `EXG_SelfDestruct` vtable slot 8 (update) | RTTI `.?AVEXG_SelfDestruct@@` at vtable `0x0077a114` |

### Five of the six cannot produce a multiplayer loss, and the sixth is a prop

1. **`EXG_Spawner::Spawn()` is unreachable for every shipped multiplayer unit.**
   The forced death at `0x004de275` runs only when the spawned unit's *type* has
   `[type+0xd8] == 0`. That field is the type's maximum health: both the fatal
   routine `0x004ccef0` and `EXG_Unit::ChangeHealth` `0x004cf5b0` divide by it when
   scaling damage score, and `0x004cf5b0` computes `maxHealth - currentDamage` from
   it. All 122 shipped multiplayer unit definitions have a non-zero `myHealth`
   (minimum 30), so the branch is dead in multiplayer.

2. **`EXG_SelfDestruct` is absent from every real multiplayer unit.** The parasite
   constructor `0x005132b0` reads `myTimeToLive`, and `EXG_Unit::Infest()`
   (`0x004cb2f0`) instantiates it for the parasite class hashed `0x1e4f04d9`
   (`SelfDestruct`). That hash appears in exactly two shipped entries,
   `-------Normal-Units-------` and `-------Special-Units-------`, which are section
   separators in `units/unittypes_wic.ice`, not playable units.

3. **`EXG_Blower` never fires in the corpus.** The blower parasite reads `myDamage`
   and `myBlastRadius` (`0x00506e00`); `EXG_BlowerTrigger` reads `myTrigRadius`
   (`0x00513870`) and appears on 18 definitions, all civilian props plus
   `NATO_LandRover`, `USSR_UAZ469`, and single-player entries. Detonation is armed
   by `0x00506c30`, splashed by `0x004d7250`, and chains to other blowers. The
   client serializes the event as `BlowerBlew(aBlower)`. The full 2,880-path corpus
   scan counts **0** `BlowerBlew` records, so no corpus death uses this path.

4. **`EXG_Game` slot 4 is the debug/cheat command interface.** Its sibling slots are
   `CreateUnit` (`0x004a6930`, gated by the cheat check `0x0041bc20` and logging
   `A cheating bastard spawned a unit at %.2f %.2f %.2f`), unit removal
   (`0x004a6ae0`), and the two score adders (`0x004a6b00`, `0x004a6b40`). Slot 4
   performs no ownership validation, unlike every genuine client request in
   `EXG_Game`, which logs `illegal client request` or `Not units owner`. No replay
   message accompanies it.

5. **`WICG_FortificationPoint` unfortify is script-driven and typed.** The caller
   `0x0052bc30` is a map-script command handler (`Perimeter point %s not defined`,
   `Could not unfortify with level %u`). It forces the fortification unit's death
   and then removes it with `0x004d5590`. Fortification losses are already a
   separate typed class in the parser and are not player unit kills.

6. **`EXG_UnitContainer::KillAllUnitsInArea()` is the only live gameplay path.** Its
   two callers are the destruction handlers at vtable slot 26 of
   `WICG_GameObject_Destroyable_Path` (`0x0051c430`, RTTI at vtable `0x0077aea4`) and
   `WICG_Bridge` (`0x0051d380`, RTTI at vtable `0x0077b014`, which also raises the
   `OnBridgeDestroyed` script event). Both pass a fixed axis-aligned box from object
   offsets `+0x110`…`+0x124` and kill every targetable unit inside it. The bounding
   box is runtime state and is not serialized; earlier bridge screens found
   essentially no corpus coverage, and `PropDamaged` co-occurrence is worthless as
   evidence because the corpus contains 1,113,998 `PropDamaged` records.

## The self-deletion caller split is complete

The shared blink scheduler is `EXG_Unit` blink-before-death, `0x004ca3e0(unit, aReason)`.
It reads the definition value `myUnitBlinkBeforeDeathTime`, stores the expiry at unit
`+0x324`, sets the blinking flag at `+0x320`, and — this is the new fact — **records
`aReason` in the unit byte at `+0x128`, but only when that byte is still zero**:

```
004ca442 CMP byte ptr [ESI + 0x128],0x0
004ca45a JNZ 0x004ca462
004ca45c MOV byte ptr [ESI + 0x128],BL
```

The broadcast itself carries no reason: `0x004b3c90(unitId, 1, 6.0f, 0.5f)` writes
`BlinkUnit` with constant time and frequency on every path.

A whole-image instruction scan finds exactly one writer besides `0x004ca45c`
(`0x004d181d`, the `EXG_Unit` constructor zeroing the field) and exactly one reader
(`0x004d7e10`). The complete reason table is therefore:

| Blink caller | Reason | Gameplay meaning | Serialized companion |
|---|---:|---|---|
| `0x004a8ac0` `EXG_Game::DisbandUnit` | 0 | player disbands own unit | none |
| `0x004d3150` from `0x004b1490` `EXG_Game::PlayerJoinTeam` | request argument | team change clears units | `PlayerJoinedTeam` |
| `0x004d3150` from `0x004aefd0` `EXG_Game::PlayerSetRole` | 1 | role change clears units | `PlayerSetRole` |
| `0x004d5810` from `0x004b7d20` `EXG_PlayerContainer::RemovePlayer` | 1 | player leaves the game | `PlayerLeavesGame` |

The single reader is `EXG_UnitContainer::Update()` (`0x004d7d20`). When the per-unit
update returns state 3 — blink expired — it checks `reason == 1` together with the
unit's order state and, on a match, refunds the owning player through `0x004bd3e0`
before removing the unit with `0x004d5590(unitId, 0)`. State 2 removes with
`0x004d5590(unitId, 1)`.

Two consequences follow. First, the reason byte defaults to zero, so disband is
indistinguishable from "no reason was ever recorded"; the reason cannot be inferred
from the wire. Second, and decisively for this investigation, **this entire branch
ends in `UnitRemove`, not `UnitDestroy`**. Disband, team change, role change, and
player departure are not members of the unattributed `UnitDestroy` population at
all. `UnitRemove.aIsToBeReplacedBySpecialistFlag` is the state-2 versus state-3
discriminator, not a disband marker.

## Correction: what the synthetic terminal direction actually means

`0x004cc280` returns a normalised direction built from two `rand() % 100 - 50`
integers with `y` exactly zero, giving 6,765 distinct bit triples. Phase 6 treated
this signature as characteristic of the forced and inherited death helpers. The
complete cross-reference list shows a **third** caller, `0x004ce830`, which feeds the
*ordinary* damage routine `0x004ce310`:

```c
void FUN_004ce830(damage, damageSourcePlayer, supportIndex, killerUnitObject) {
    killerId = killerUnitObject ? killerUnitObject->id : 0x200;
    FUN_004ce310(damage, 0, killerId, FUN_004cc280(), 0,
                 damageSourcePlayer, supportIndex);
}
```

`0x004ce830` has four callers, and they cover common gameplay:

| Caller | Meaning | Killer written |
|---|---|---|
| `0x004d7d20` `EXG_UnitContainer::Update()` | collision crush; damage equals the victim's remaining health | the crushing unit's real ID |
| `0x004d7250` | blower splash | sentinel `0x200` |
| `0x00517f10` from `0x004da180` | building damage splashed onto its current residents, carrying the building's damage source and support index | sentinel `0x200` |
| `0x004cf5b0` `EXG_Unit::ChangeHealth` (negative delta) | support clouds `0x004fbd80`, fortification damage `0x00536220`, per-tick attrition from `0x004d26b0`, and seven further callers | sentinel `0x200` |

So the signature does not identify a mechanism. It identifies **fatal damage that
carried no direction vector**: any death routed through `0x004ce830` or through the
two synthetic-direction death helpers. Its complement — a serialized hit direction
outside the 6,765-value set — identifies directional damage. The Phase 6
`buildingCollapse` and `destroyedWithContainer` rules remain sound because they
require exact building-slot or type-2 relation binding, same-raw-tick terminal
events, and killer agreement in addition to the signature; only the signature's
stated meaning changes.

## Corpus measurement

`scripts/forced_death_partition_scan.py` partitions every raw `UnitDestroy` in the
linked corpus by killer sentinel and by the exact terminal-direction bit set, and
counts same-raw-tick replay companions for the synthetic half.

```bash
"$HOME/.venvs/wic-analysis/bin/python" scripts/forced_death_partition_scan.py \
  replays/main replays/settings replays/wicgate-documents \
  --jobs 12 --json /tmp/unit-destruction-phase8-forced-death.json
```

All 2,880 paths analysed with zero failures. Report SHA-256:
`2a74a811760d533c21e332c2bae5e017bccbd53e641b18e4bdac094cd56e5410`.

| Population | Deaths |
|---|---:|
| Known killer, directional hit direction | 691,234 |
| Known killer, synthetic direction | 76,751 |
| Sentinel killer, directional hit direction | 288,025 |
| Sentinel killer, synthetic direction | 112,725 |
| `BlowerBlew` records | 0 |

Same-raw-tick companions of the synthetic half (categories overlap and must not be
summed):

| Companion at the identical raw tick | Known killer | Sentinel killer |
|---|---:|---:|
| `BuildingDamaged` with `aState == 3` | 28,841 | 42,917 |
| More than one `UnitDestroy` | 47,675 | 67,525 |
| `UnitCreate` for the same unit | 454 | 311 |
| Any `PropDamaged` / `RepairablePropDamaged` | 4,316 | 17,530 |
| None of the above | 26,610 | 40,465 |

These are census features, not attribution. The `PropDamaged` column in particular
is meaningless as evidence: the corpus carries 1,113,998 `PropDamaged` and 90,783
`RepairablePropDamaged` records, so same-tick co-occurrence is expected by volume
alone. The 454 and 311 same-tick `UnitCreate` matches are *not* the spawner path,
which Section 1 above proves unreachable; they are units created and killed within
one tick by ordinary damage.

The large known-killer synthetic population (76,751) is now explained without
appealing to unclassified mechanisms: the collision-crush path at `0x004d7d20`
writes the crushing unit's real ID, and the inherited-death helper copies a real
killer from a destroyed building or container.

## What this closes and what remains

Closed:

- Every direct caller of `0x004ce880` is identified, and five of six are proven
  unreachable, unused in the corpus, or non-gameplay.
- The self-deletion caller split is complete, with the reason byte located and its
  only reader identified. The branch terminates in `UnitRemove`, so it does not
  contribute to the unattributed `UnitDestroy` population.
- The terminal-direction signature's meaning is corrected to "non-directional fatal
  damage".

Open, in the order that now looks most promising:

1. The sentinel + directional population (288,025 raw records) is fatal damage that
   carried a real direction and no killer unit. In this executable that is support
   weaponry: support projectiles are created without an owning unit, and the four
   support-projectile messages carry no player. Establishing whether *every* such
   death is support-caused requires enumerating the remaining callers of
   `0x004d2bd0` and `0x004d26b0` and proving no ordinary-unit weapon can reach the
   fatal routine with a null killer. If that holds, it supports a mechanism-level
   `tacticalAid` cause with no actor, which is weaker than the schema-v17 exact
   subset but far broader.
2. `0x00517f10` gives building residents the building's own damage source and
   support index. A resident death co-timed with a serialized `BuildingDamaged`
   health decrease, under the Phase 6 slot binding, is a candidate `buildingDamage`
   mechanical context distinct from `buildingCollapse`. It needs the same positive
   controls and ambiguity rejection before promotion.
3. The bridge and destroyable-path kill box remains unserialized. Only shipped map
   geometry could bound it, and prior screens found negligible corpus coverage.
