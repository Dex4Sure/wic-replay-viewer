import { describe, expect, it } from 'vitest';

import { gameScoreOrder } from './gameScoreOrder';

interface ScoredPlayer {
  id: number;
  score: number | null;
}

function player(id: number, score: number | null): ScoredPlayer {
  return { id, score };
}

describe('original game score ordering', () => {
  it('sorts scores descending across the fixed 16 player slots', () => {
    const players = [player(15, 30), player(0, 10), player(8, 20)];

    expect(gameScoreOrder(players, (entry) => entry.score).map((entry) => entry.id)).toEqual([
      15, 8, 0,
    ]);
  });

  it("retains the executable's deterministic but unstable order for ties", () => {
    const players = Array.from({ length: 16 }, (_, id) => player(id, 100));

    expect(gameScoreOrder(players, (entry) => entry.score).map((entry) => entry.id)).toEqual([
      15, 11, 9, 1, 13, 2, 8, 3, 10, 4, 12, 5, 14, 6, 0, 7,
    ]);
  });

  it('selects the original-game leader for a real three-way replay tie', () => {
    // demo312.wicdemo, SHA-256
    // 23498b5857bb1797c20ded95946f112fe9412ce30c56efc07b126cc2c15e026b
    const capturingScores = [0, 80, 140, 0, 0, 220, 140, 180, 100, 100, 0, 0, 0, 220, 220, 0];
    const players = capturingScores.map((score, id) => player(id, score));
    const ordered = gameScoreOrder(players, (entry) => entry.score);

    expect(ordered[0].id).toBe(5);
    expect(ordered.slice(0, 3).map((entry) => entry.id)).toEqual([5, 13, 14]);
  });

  it("uses the game's zero-score sentinel without discarding real players", () => {
    const players = [player(0, 0), player(1, null), player(2, -1), player(3, 1)];
    const ordered = gameScoreOrder(players, (entry) => entry.score);

    expect(ordered.map((entry) => entry.id)).toEqual([3, 2, 1, 0]);
  });
});
