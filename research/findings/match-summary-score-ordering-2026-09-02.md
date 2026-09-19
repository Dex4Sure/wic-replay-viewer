# Match-summary score ordering and tie behavior

Date: 2026-09-02

## Result

The World in Conflict match-summary screen does not retain every player tied for
a score and does not apply a semantic tie-break such as total score, player slot,
name, role, or faction. It sorts one score field at a time with a deterministic,
unstable quicksort and displays the first eligible player from that resulting
order. To a player, the selected member of a tie is therefore arbitrary even
though the same complete 16-slot score table produces the same result.

Overall score is not consulted when resolving a tie in Command Points or another
category. The client invokes the same ordering path separately for total score,
each role score, and each post-match category score.

## Source evidence

Client target: `binaries/game/wic.exe`, PE32 x86, image base `0x00400000`, version
`1.0.1.1 (b35)`, SHA-256
`41017f352cabd5fb3dc2b4457727c6e32678fdbdd89e881b11a8868edba18ebc`.

Read-only Ghidra inspection established the following path:

- `wic.exe:0x008bcf30` initializes the match-summary widgets. It creates three
  individual Top Players positions, up to four individual role-leader positions,
  and one player-name position for each score category.
- `wic.exe:0x008bc220` populates those widgets. The total-score result is scanned
  until three eligible player slots have been displayed. Each role and score
  category is ordered independently; a role displays its first eligible non-zero
  result, while a score category displays its first eligible positive result.
- `wic.exe:0x008bb760` constructs 16 `{slot, score}` entries in original slot order.
  It replaces an exact zero score with `-10000`, then calls the descending sorter.
- `wic.exe:0x0043e750` implements recursive-left, iterative-right quicksort using
  the middle array position as its pivot.
- `wic.exe:0x0043e7a0` partitions solely on the selected score. In descending mode,
  a left-side value advances when it is greater than or equal to the pivot. No
  comparison of total score, player slot, name, role, faction, or another field is
  present.
- `wic.exe:0x008bc050` validates player slots only after sorting. Invalid or
  non-displayable entries are skipped by the population loops rather than removed
  before the score table is ordered.

The descending partition is equivalent to:

```text
swap(middlePivot, left)
cursor = left + 1
tail = right

while cursor <= tail:
    if score[cursor] >= pivotScore:
        cursor += 1
    else if score[tail] < pivotScore:
        tail -= 1
    else:
        swap(cursor, tail)

swap(left, tail)
```

The pivot swaps and recursive partitions can reorder equal values. Initial slot
order therefore affects the outcome, but neither the lowest nor highest slot is
consistently favored. With all 16 scores equal, the recovered routine produces
this slot order:

```text
15, 11, 9, 1, 13, 2, 8, 3, 10, 4, 12, 5, 14, 6, 0, 7
```

This is deterministic evidence that the tie result is not a lowest-slot rule.

No Ghidra annotations or source evidence were modified.

## Replay positive control

Private replay `demo312.wicdemo`, SHA-256
`23498b5857bb1797c20ded95946f112fe9412ce30c56efc07b126cc2c15e026b`,
contains the following 16 serialized Command Points scores in slot order:

```text
0, 80, 140, 0, 0, 220, 140, 180, 100, 100, 0, 0, 0, 220, 220, 0
```

Slots 5, 13, and 14 are tied at 220. Applying the recovered client routine orders
those leaders as `5, 13, 14`, so slot 5 is the single displayed leader. This
positive control proves that a real replay exercises the multi-player tie path;
the predicted displayed slot comes from the static client routine and has not been
independently compared with a captured 2007 client screenshot.

The replay was read directly with the unified parser; it was not renamed, moved,
patched, or overwritten.

## Viewer implementation boundary

The replay does not serialize a chosen leader or tie-break field. The viewer must
derive match-summary presentation from the serialized per-player scores. The
game-faithful implementation in
`components/replay-viewer/frontend/gameScoreOrder.ts` reproduces the fixed 16-slot
table, zero sentinel, pivot choice, partition behavior, and traversal order.

The viewer then applies that ordering as follows:

- Top Players takes the first three eligible players as three individual ranks.
- Each displayed role and score category takes the first eligible positive player.
- Missing parser scores remain absent rather than being presented as observed game
  results.
- Labels, layout, colors, and the subset of categories shown remain viewer product
  decisions rather than claims about the original screen layout.

Regression tests cover ordinary descending order, the exact all-equal permutation,
the real three-way replay vector, and the original zero-score sentinel behavior.
