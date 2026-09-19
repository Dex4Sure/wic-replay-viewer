import { describe, expect, it } from 'vitest';
import { createScoreboard } from './replayScoreboard';
import type { ScoreParticipant } from './types';

const player = (playerId: number, overrides: Partial<ScoreParticipant> = {}): ScoreParticipant => ({
  playerId,
  sessionIndex: 0,
  playerName: `Player ${playerId}`,
  startSeconds: 0,
  endSeconds: null,
  ...overrides,
});

describe('recorded live scores', () => {
  it('seeks both ways, preserves penalties, keeps player order and sums only known player scores', () => {
    const board = createScoreboard(
      [player(0), player(1), player(2)],
      [0, 1, 2].map((playerId) => ({ playerId, timeSeconds: 1, team: playerId === 2 ? 3 : 1 })),
      [
        { playerId: 0, timeSeconds: 5, score: 10 },
        { playerId: 1, timeSeconds: 5, score: 20 },
        { playerId: 2, timeSeconds: 5, score: 8 },
        { playerId: 0, timeSeconds: 7, score: -2 },
      ],
    );
    expect(board(0)).toEqual([]);
    expect(board(4).map((t) => t.total)).toEqual([null, null]);
    expect(board(5)[0]).toMatchObject({
      label: 'USA',
      total: 30,
      players: [
        { playerId: 0, score: 10 },
        { playerId: 1, score: 20 },
      ],
    });
    expect(board(7).map((t) => t.total)).toEqual([18, 8]);
    expect(board(7)[0]!.players.map((p) => p.playerId)).toEqual([0, 1]);
    expect(board(5)[0]!.total).toBe(30);
    expect(board(0)).toEqual([]);
  });
  it('does not transfer a departed occupant’s score or team to a reused slot', () => {
    const board = createScoreboard(
      [
        player(0, { endSeconds: 5 }),
        player(0, { sessionIndex: 1, playerName: 'New arrival', startSeconds: 8 }),
      ],
      [
        { playerId: 0, timeSeconds: 1, team: 1 },
        { playerId: 0, timeSeconds: 9, team: 3 },
      ],
      [
        { playerId: 0, timeSeconds: 2, score: 50 },
        { playerId: 0, timeSeconds: 8, score: 50 },
        { playerId: 0, timeSeconds: 10, score: 3 },
      ],
    );
    expect(board(5)).toEqual([]);
    expect(board(7)).toEqual([]);
    expect(board(8)).toEqual([]);
    expect(board(9)[0]).toMatchObject({ label: 'USSR', total: null });
    expect(board(10)[0]!.total).toBe(3);
  });
  it('handles late starts, spectators, team changes, unnamed and empty slots', () => {
    const board = createScoreboard(
      [
        player(0, { startSeconds: 3 }),
        player(1, { playerName: null }),
        player(2, { playerName: null }),
        player(3),
      ],
      [
        { playerId: 0, timeSeconds: 4, team: 2 },
        { playerId: 0, timeSeconds: 7, team: 3 },
        { playerId: 3, timeSeconds: 0, team: 0 },
        { playerId: 1, timeSeconds: 4, team: 9 },
      ],
      [
        { playerId: 0, timeSeconds: 5, score: 4 },
        { playerId: 1, timeSeconds: 5, score: 6 },
        { playerId: 2, timeSeconds: 5, score: 0 },
      ],
    );
    expect(board(2)).toEqual([]);
    expect(board(4)[0]).toMatchObject({ label: 'NATO', total: null });
    expect(board(5).map((t) => t.label)).toEqual(['NATO']);
    expect(board(7).map((t) => t.label)).toEqual(['USSR']);
  });
  it('retains file order for simultaneous observations and stable slot order for ties', () => {
    const board = createScoreboard(
      [player(1), player(0)],
      [
        { playerId: 0, timeSeconds: 0, team: 1 },
        { playerId: 1, timeSeconds: 0, team: 1 },
      ],
      [
        { playerId: 0, timeSeconds: 2, score: 3 },
        { playerId: 0, timeSeconds: 2, score: 4 },
        { playerId: 1, timeSeconds: 2, score: 4 },
      ],
    );
    expect(board(2)[0]!.players.map((p) => [p.playerId, p.score])).toEqual([
      [0, 4],
      [1, 4],
    ]);
    expect(createScoreboard([], [], [])(100)).toEqual([]);
  });
});
