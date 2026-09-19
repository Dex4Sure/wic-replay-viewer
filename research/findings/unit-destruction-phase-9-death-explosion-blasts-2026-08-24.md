# Phase 9 — Death-explosion blasts produce sentinel-killer directional deaths

Date: 2026-08-24
Target: `binaries/game/wic_ds.exe`
SHA-256: `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf`
Architecture: 32-bit Windows PE, image base `0x00400000`

## Question

Phase 8 left one bounded hypothesis open: *is every sentinel-killer unit death
that carries a directional hit vector caused by tactical-aid (support) damage?*
That class is the single largest remaining unattributed population.

## Answer

**The hypothesis is refuted.** Death explosions are a proven producer of
sentinel-killer directional deaths. A sentinel killer plus a directional hit
vector is therefore not a support signature, and must not be labelled as one.

## Binary evidence

### 1. The engine's own definition of an attributable kill

`EXG_Unit::Kill` at `0x004ccef0` is in `.\EXG_Unit.cpp` and opens with

```
assert(aKillerUnit <= EX_MAX_UNITS)     ; .\EXG_Unit.cpp:0x4e0
```

`EX_MAX_UNITS` is `0x200` (512). The routine stores the killer as a `u16` at
unit `+0x234` — the field serialized into `UnitDestroy` — and gates the scoring
calls `0x004b5de0` / `0x004b80c0` on the expression

```c
-(uint)(killerUnitId < 0x200)
```

So `512` is not a parser artefact or a lost value: it is the engine's own
in-band encoding for *this damage had no owning unit*, and the engine itself
declines to award the kill.

### 2. Blast damage hard-codes the sentinel

`EXG_BlastContainer` applies queued blasts at `0x004f2b50`
(`.\EXG_BlastContainer.cpp`, assert `aBlastRadius > 0.0f` at line `0x3a`). For
every blastable object found within the radius it issues

```c
(**(code **)(**(int **)(*(int *)(param_1 + 0x1c) + i * 4) + 0x50))
          (damage, 0x200, 0xffffffff, position, ..., 0);
//                  ^^^^^ EX_MAX_UNITS, literal
```

The killer unit is the literal `0x200`. The blast nevertheless carries a real
world `position`, from which the fatal path derives a genuine hit direction.
Blast damage therefore produces exactly the observed combination: **sentinel
killer, real directional vector**.

### 3. The only blast producer is a dying entity

Blasts are enqueued only by `0x004f2df0`, which allocates a blast record and
appends it to the container's growing array. That function has exactly one
caller in the image, at `0x0051350d`, inside the method beginning at
`0x00513450` in `.\EXG_Death.cpp`. Immediately before the enqueue that method
reads two shipped type properties by name:

- `"myBlastRadius"` (`0x007784f8`)
- `"myDamage"` (`0x00778508`)

`0x00513400` is the `EXG_Death` constructor proper (allocates `0x18` bytes and
installs `EXG_Death::vftable`); `0x00513450` is the virtual method that runs the
explosion.

### 4. The blast is applied one tick later

`EXG_BlastContainer::Update` (`0x004f2e60`) pops exactly **one** blast per tick
and swap-removes it. It is driven from the `EXG_Game` tick at `0x004b2970`
(`.\EXG_Game.cpp`), with the container living at game object offset `0x13200c`;
a raw scan finds only four references to that offset in `.text`, all inside
`EXG_Game` construction and tick.

### Resulting chain

```
entity dies -> EXG_Death (0x00513450) reads myDamage / myBlastRadius
            -> blast enqueued (0x004f2df0)
            -> EXG_Game tick (0x004b2970) -> BlastContainer::Update (0x004f2e60)
            -> blast applied (0x004f2b50) with aKillerUnit = EX_MAX_UNITS
            -> EXG_Unit::Kill (0x004ccef0) records killer 512 + real direction
```

### Exhaustiveness of the caller claims

Function entries were taken from every rel32 call destination in `.text`, and
compilation units from every `push imm32` of an assert filename string
(`scripts/wic_source_map.py`: 361 source strings, 12,061 assert sites, 9,229
function entries, 2,847 attributed functions). `EXG_Unit::TakeDamage`
(`0x004d2bd0`) has exactly one reference in the whole image — the DATA
reference at `0x007738b0`, which is `EXG_Unit` vtable start `0x00773864` plus
slot 19 (`0x4C`) — confirming it is reached only by virtual dispatch.

## Corpus measurement

`scripts/death_explosion_chain_scan.py` over `replays/main`,
`replays/settings`, `replays/wicgate-documents`:

- 2,880 replays analysed, **0 failures**
- report SHA-256 `c5f9333385c46953dfaece11cecc7fd36d0b90982539ea3f6c673d16ed539c2a`

If a sentinel directional death is a blast from a nearby entity's death, it must
be preceded within a very short window by that entity's own `UnitDestroy`.

| class | n | same tick | <=0.1s | <=0.25s | <=0.5s | <=1.0s |
|---|---:|---:|---:|---:|---:|---:|
| sentinel + directional | 288,025 | **50.4%** | 17.5% | **46.4%** | 57.9% | 70.0% |
| attributed + directional | 691,234 | 10.2% | 3.9% | 13.7% | 24.3% | 39.9% |
| sentinel + synthetic | 112,716 | 59.9% | 6.1% | 20.0% | 31.2% | 46.8% |
| attributed + synthetic | 76,747 | 62.1% | 5.9% | 19.9% | 32.6% | 48.1% |

Sentinel directional deaths are **4.93x** more likely than the attributed
control to share a tick with another death, and **3.38x** more likely to follow
one within 0.25 s.

**Negative control.** The enrichment is specific to the directional class. In
the synthetic-direction class the sentinel and attributed rates are
indistinguishable (59.9% vs 62.1% same tick; 20.0% vs 19.9% at 0.25 s). A
generic "combat is bursty, deaths cluster" explanation would raise both classes
equally, and does not.

Note the population: these counts cover every `UnitDestroy` in the corpus. The
narrower Phase 8 figure of 142,928 counted only player-owned complete units.

## What this does and does not establish

Established:

- Sentinel killer means "no owning unit" by engine design, not by data loss.
- Death explosions provably emit sentinel-killer damage carrying a real
  direction, so the class is not exclusively — or even mostly — support fire.
- The class carries the temporal signature that mechanism predicts, with a
  clean negative control.

Not established, and not to be asserted:

- That death explosions are the *only* producer of sentinel directional deaths.
  Blast is proven to be *a* producer; that alone refutes the support hypothesis,
  but other unowned sources are not ruled out.
- Which specific parent death caused any individual chain kill. `UnitDestroy`
  serializes no position, so the spatial half of the blast test
  (`myBlastRadius`) cannot be evaluated from the replay stream. Temporal
  adjacency is aggregate evidence, not a per-death bridge.

Consequently there is still **no deterministic per-death actor or team** for
this population, and the viewer must continue to report it as an unattributed
loss rather than inventing a cause.

## Tooling added

- `scripts/wic_source_map.py` — deterministic address to compilation-unit map
  built from assert strings and rel32 call targets. Needs no Ghidra code
  indexing, which remains deliberately suppressed for this project.
- `scripts/death_explosion_chain_scan.py` — the corpus adjacency measurement
  above.
