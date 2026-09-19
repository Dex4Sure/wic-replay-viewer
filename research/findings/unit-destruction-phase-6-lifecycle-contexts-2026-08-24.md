# Deterministic building and container destruction contexts

## Question

Can replay-visible lifecycle state explain any remaining complete-unit losses
without guessing an attacker from historical ownership, timing, or proximity?

This phase tests two dedicated-server fatal paths: occupants killed when an
occupied building collapses and units killed with the transport/container that
currently contains them. It deliberately treats mechanical context and attacker
identity as separate questions.

## Binary identity

| Field | Value |
|---|---|
| Program | `/wic_ds.exe` |
| Architecture | x86 little-endian 32-bit PE |
| File version | `1.0.1.1 (b35)` |
| SHA-256 | `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf` |
| Ghidra source path | `<private-evidence-root>/massgate_wicgate/bin/server/wic_ds.exe` |

## Building-collapse path

Ghidra shows the following synchronous path:

- `0x004da180` records the building's lethal killer unit ID at `+0x10c` and
  damage-source context at `+0x110`.
- `0x004d9fd0` transitions the building to state 3 and calls `0x004d9f30`.
- `0x004d9f30` reloads those exact values and invokes resident manager
  `0x00518d30`.
- `0x00518d30` iterates the building's current residents and passes the recorded
  killer/source into `0x004ce8a0`, then clears the resident set.
- `0x004ce8a0` obtains a terminal direction from `0x004cc280` and calls fatal
  routine `0x004ccef0`.
- `0x004ccef0` stores the killer at unit offset `+0x234`, the source at `+0x24c`,
  and writes `UnitDestroy`.

`0x004cc280` chooses integer X/Z components from `[-50, 49]`, normalizes them as
32-bit floats, and writes Y as exact zero. The finite set of 10,000 possible bit
triples is therefore reproducible. The signature alone is not unique because the
same helper has other callers; exact building lifecycle and timing are mandatory.

Production rule:

1. `BuildingSetSlotState` binds the exact active unit generation to a building
   slot and no later vacate or unit-ID reuse invalidates it.
2. That same building enters `BuildingDamaged.aState == 3` at the identical raw
   `Event` timestamp as the unit destruction. Serialization may place the building
   record immediately before or after the resident deaths.
3. `UnitDestroy` has the exact `0x004cc280` terminal-direction bit signature.
4. Any simultaneous container-context match is ambiguity and abstains.

This proves `buildingCollapse`; it does not reveal who damaged the building.

## Destroyed-container path

The relation and fatal path is independently explicit:

- `0x0050f2c0` is the `EXG_Container` entry path. It creates relation type 2 via
  `0x004f7d80(2, container, child)` and calls `OnUnitEnterContainer`.
- `0x005108d0` iterates current relation-type-2 occupants during destruction.
- At `0x00510904`/`0x00510996`, it reads the container's stored damage source via
  `0x004c7f80` (`+0x24c`) and killer via `0x004c7f60` (`+0x234`) and passes both to
  occupant fatal helper `0x004ce8a0`.
- Relation removal flows through `0x004f7140`.
- `0x004ce8a0` and `0x004ccef0` then produce the same synthetic direction and
  serialize the inherited killer/source on the child.

Production rule:

1. A serialized type-2 relation binds exact active container and child
   generations.
2. The relation is still active at death or is torn down at the same raw tick no
   later than the child terminal.
3. Parent and child both have `UnitDestroy` at that raw tick and the same serialized
   killer unit ID.
4. The child carries the exact synthetic terminal direction.
5. Generation reuse, ordering, time, killer, cause, or context ambiguity abstains.

Because the executable copies the container's damage-source pointer as well as its
killer, an independently exact schema-v17 tactical-aid cause on the proven
container is also exact for the child. No proximity or historical owner is used.

## Corpus controls

The raw lifecycle overlap audit covered all 2,880 linked replay paths with zero
failures:

| Population | Exact matches |
|---|---:|
| Sentinel building-collapse contexts | 8,126 |
| Sentinel destroyed-container contexts | 598 |
| Container parents with exact tactical-aid cause | 67 |
| Child direct exact-TA conflicts | 0 |
| Building/container overlap | 0 |

Every one of the 8,126 building matches used a state-3 row whose serialized flag
was true. The independent positive controls found 6,632 known-killer building
resident deaths and 2,191 known-killer container-child deaths with the same exact
signatures; container controls agreed on the parent's killer in every match.

Raw report hashes:

- lifecycle overlap v2: `32c1223235905b932a60a10524b80bc74e55ef8f2c021bedf000fec4ea50f8fc`
- building resident v4: `5d0dd1e10ef6650a91ea70b7b5fe5f19f7aa9d5ed3011ddef01431d2cf1ba918`
- container loss: `b782037f1dd3672fe006a3bfc0cff4a360ba3ef1a84467faf4aeeefd457d4416`

The canonical schema-v18 parser was then run recursively over the same 2,880
paths. It parsed the established 2,865 valid replays and rejected the same 15
corrupt/empty inputs:

| Canonical complete-unit output | Count |
|---|---:|
| All destructions | 713,051 |
| `buildingCollapse` context | 14,723 |
| `buildingCollapse` with cause still unknown | 8,112 |
| `destroyedWithContainer` context | 2,779 |
| `destroyedWithContainer` with cause still unknown | 531 |
| Container children inheriting exact tactical aid | 67 |
| Contextual deaths with player-owned victim | 17,502 |
| Tactical-aid cause after inheritance | 16,395 |
| Cause still unknown | 213,472 |

The larger context totals include known-killer positive-control deaths. Differences
from raw audit candidate counts are the canonical parser's valid-replay,
complete-unit, and gameplay-window bounds. The canonical corpus report SHA-256 is
`cb5f1df9e8c65a272f9d918b040c30e6efc50d43f75a63eace4f9aecdbe7939b`.
All 16 private ground-truth fixtures passed with none skipped.

## Decision

Promote both mechanical contexts to parser schema v18. Keep `cause` orthogonal:
building collapse and most container deaths do not identify an initiating actor.
Promote only the 67 container children whose parent already has deterministic
exact-target tactical-aid evidence. Viewer wording may explain the mechanical loss
while remaining faction-neutral whenever the attacker is unknown.
