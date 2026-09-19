const PLAYER_SLOT_COUNT = 16;
const ZERO_SCORE_SENTINEL = -10_000;

interface ScoreSlot {
  id: number;
  score: number;
}

function swap(slots: ScoreSlot[], left: number, right: number): void {
  if (left === right) {
    return;
  }
  [slots[left], slots[right]] = [slots[right], slots[left]];
}

// Mirrors wic.exe FUN_0043e7a0's descending partition. Equal scores advance
// from the left rather than using a secondary player identifier comparison.
function partitionDescending(
  slots: ScoreSlot[],
  left: number,
  right: number,
  pivotIndex: number,
): number {
  swap(slots, left, pivotIndex);
  const pivotIndexAtLeft = left;
  let cursor = left + 1;
  let tail = right;

  while (cursor <= tail) {
    const score = slots[cursor].score;
    const pivotScore = slots[pivotIndexAtLeft].score;
    if (score >= pivotScore) {
      cursor += 1;
    } else if (slots[tail].score < pivotScore) {
      tail -= 1;
    } else {
      swap(slots, cursor, tail);
    }
  }

  swap(slots, pivotIndexAtLeft, tail);
  return tail;
}

// Mirrors wic.exe FUN_0043e750, including its recursive-left/iterative-right
// traversal. That detail makes tied results deterministic but not stable.
function sortDescending(slots: ScoreSlot[], initialLeft: number, right: number): void {
  let left = initialLeft;
  while (left <= right) {
    const pivotIndex = Math.floor((left + right) / 2);
    const partition = partitionDescending(slots, left, right, pivotIndex);
    if (left < partition) {
      sortDescending(slots, left, partition - 1);
    }
    if (right <= partition) {
      return;
    }
    left = partition + 1;
  }
}

/**
 * Reproduces the match-summary score ordering used by the original game.
 *
 * WiC always sorts all 16 player slots, substitutes -10000 for a zero score,
 * and only then rejects empty or otherwise ineligible player slots. Returning
 * parsed players in that slot order lets each caller apply the same eligibility
 * rule as its summary panel without replacing WiC's tie behavior.
 */
export function gameScoreOrder<T extends { id: number }>(
  players: readonly T[],
  scoreFor: (player: T) => number | null,
): T[] {
  const playersById = new Map<number, T>();
  for (const player of players) {
    if (Number.isInteger(player.id) && player.id >= 0 && player.id < PLAYER_SLOT_COUNT) {
      playersById.set(player.id, player);
    }
  }

  const slots = Array.from({ length: PLAYER_SLOT_COUNT }, (_, id) => {
    const player = playersById.get(id);
    const score = player ? (scoreFor(player) ?? 0) : 0;
    return { id, score: score === 0 ? ZERO_SCORE_SENTINEL : score };
  });
  sortDescending(slots, 0, slots.length - 1);

  return slots.flatMap(({ id }) => {
    const player = playersById.get(id);
    return player ? [player] : [];
  });
}
