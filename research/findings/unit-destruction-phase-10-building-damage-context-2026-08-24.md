# Phase 10 — the `buildingDamage` mechanical context is rejected

Date: 2026-08-24
Target: `binaries/game/wic_ds.exe`
SHA-256: `c150494dcd3b59c4afe3cf7580619b4864cd0238a892f350f1efcedfe6ac10cf`
Architecture: 32-bit Windows PE, image base `0x00400000`

## Question

Can a resident killed by building splash damage be deterministically joined to a
same-raw-tick `BuildingDamaged` health decrease, so that a `buildingDamage`
mechanical context could be promoted into the parser?

## Answer

**No. The rule is rejected on deterministic grounds**, and separately its yield
would have been negligible. It must not be promoted.

## Binary evidence

### The splash path discards the attacker deliberately

`0x004da180` is the building damage handler. It subtracts the damage from
building health at `+0xd0` and, when the building becomes lethal, records the
killer unit at `+0x10c`:

```c
*(int *)(param_1 + 0xd0) = *(int *)(param_1 + 0xd0) - param_2;
if (*(int *)(param_1 + 0xd0) < 1) {
    *(short *)(param_1 + 0x10c) = (short)param_3;   /* killer unit id */
    *(undefined4 *)(param_1 + 0x110) = param_6;
}
...
if (*(int *)(param_1 + 0x168) != 3) {
    FUN_00517f10(local_4, param_6, param_7);        /* resident splash */
}
```

`0x00517f10` walks the building's resident array (count at `+0x4c`, entries at
`+0x58`, resident unit id at entry `+8`) and damages each one via

```c
FUN_004ce830(param_2, param_3, param_4, 0);
```

The final argument is the **literal `0`**. `0x004ce830` branches on exactly that
argument:

```c
uVar2 = (param_4 == 0) ? 0x200 : *(undefined2 *)(param_4 + 0xc0);
uVar1 = FUN_004cc280(0, param_2, param_3);   /* synthetic terminal direction */
FUN_004ce310(param_1, 0, uVar2, uVar1, uVar3, param_2, param_3);
```

So the building's attacker is available at the call site and is deliberately not
propagated. Resident splash deaths are therefore, **by construction**:

- killer `= 0x200` (`EX_MAX_UNITS`, the engine's "no owning unit" encoding), and
- carrying a **synthetic** terminal direction from `0x004cc280`.

This is the same shape as the Phase 9 blast result: no actor because the engine
chose not to record one, not because the replay lost it.

### The decisive consequence

A genuine building-splash death **must** be serialized as sentinel killer *and*
synthetic direction. Any candidate rule that fires on a different class is
firing on a death it cannot possibly explain.

## Corpus measurement

`scripts/building_damage_context_audit.py` over `replays/main`,
`replays/settings`, `replays/wicgate-documents`:

- 2,880 replays analysed, **0 failures**
- report SHA-256 `5337409763dbb0609eb5b68abc9f94d35b4287b19406df9186907cfe29918b3f`

Residency is tracked from `CreateBuildingRelation`, `DestroyBuildingRelations`
and `DestroyBuildingRelations_Unit`; building health from `BuildingCreate` and
`BuildingDamaged`. Residency is snapshotted at the start of each raw tick so
same-tick relation teardown cannot contaminate the state a death is judged
against.

The candidate rule: *victim was a resident of a building that took a strict
health decrease in the same raw tick.*

| class | residents | rule fires | rate | can be splash? |
|---|---:|---:|---:|---|
| sentinel + synthetic | 22,276 | 1,569 | 7.04% | yes, the only candidate |
| sentinel + directional | 20,084 | 1,127 | 5.61% | **no** — wrong direction class |
| attributed + directional | 56,755 | 814 | 1.43% | **no** — killer resolved |
| attributed + synthetic | 15,028 | 535 | 3.56% | **no** — killer resolved |

The rule fires **2,476 times on deaths that provably are not building splash**
against **1,569 times where it could be right**. A majority of all matches are
false by construction. Against the resolved-killer control alone the base rate is
1.88%, so of the 1,569 candidate matches roughly 419 would be coincidental even
before considering the sentinel-directional evidence.

The explanation is mundane: a building under attack is a dangerous place, so
residents die there from every cause at once. Same-tick co-occurrence carries no
causal information.

## Yield, had it worked

1,569 deaths corpus-wide is 1.4% of the 112,725 sentinel-synthetic destructions
and about 0.2% of the 713,051 complete-unit destructions. Because the attacker is
discarded in the binary, even a perfect rule would have added a mechanism label
with **no actor and no team** — not attribution.

## Conclusion

Rejected on two independent grounds: it is not deterministic, and its yield is
negligible. Unknown causes stay unknown.

Closed and not to be reopened without new evidence.
