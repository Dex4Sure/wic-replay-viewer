import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createReplayPlaybackTimer,
  dominationPlaybackAnchor,
  dominationSharesAt,
  REPLAY_FRAME_INTERVAL_MS,
  revealedReplayEventCount,
  showReplayMapUnit,
} from './replayPlayback';

afterEach(() => vi.useRealTimers());

describe('replay playback timer', () => {
  it('updates at 30 fps using elapsed time and stops when paused', () => {
    vi.useFakeTimers();
    let time = 0;
    let playing = false;
    const setTime = vi.fn((value: number) => {
      time = value;
    });
    const timer = createReplayPlaybackTimer({
      duration: () => 10,
      speed: () => 1,
      time: () => time,
      setPlaying: (value) => {
        playing = value;
      },
      setTime,
      now: () => performance.now(),
    });

    timer.start();
    expect(playing).toBe(true);
    expect(vi.getTimerCount()).toBe(1);

    vi.advanceTimersByTime(REPLAY_FRAME_INTERVAL_MS * 3);
    expect(REPLAY_FRAME_INTERVAL_MS).toBeCloseTo(33.333, 3);
    expect(setTime).toHaveBeenCalledTimes(3);
    expect(time).toBeCloseTo(0.099, 5);
    expect(vi.getTimerCount()).toBe(1);

    timer.stop();
    expect(playing).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(1000);
    expect(time).toBeCloseTo(0.099, 5);
  });

  it('clamps to the replay duration and clears the final timer', () => {
    vi.useFakeTimers();
    let time = 0;
    let playing = false;
    const timer = createReplayPlaybackTimer({
      duration: () => 0.05,
      speed: () => 2,
      time: () => time,
      setPlaying: (value) => {
        playing = value;
      },
      setTime: (value) => {
        time = value;
      },
      now: () => performance.now(),
    });

    timer.start();
    vi.advanceTimersByTime(REPLAY_FRAME_INTERVAL_MS);

    expect(time).toBe(0.05);
    expect(playing).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe('replay event reveal', () => {
  const rows = [{ timeSeconds: 0 }, { timeSeconds: 1.5 }, { timeSeconds: 1.5 }, { timeSeconds: 4 }];

  it('reveals only events at or before the playback time', () => {
    expect(revealedReplayEventCount(rows, 0)).toBe(1);
    expect(revealedReplayEventCount(rows, 1.49)).toBe(1);
    expect(revealedReplayEventCount(rows, 1.5)).toBe(3);
    expect(revealedReplayEventCount(rows, 10)).toBe(4);
  });

  it('returns no events before the first timestamp or for an empty timeline', () => {
    expect(revealedReplayEventCount(rows, -0.01)).toBe(0);
    expect(revealedReplayEventCount([], 10)).toBe(0);
  });
});

describe('replay map unit visibility', () => {
  it('hides infantry members while retaining complete units', () => {
    expect(showReplayMapUnit({ isInfantryMember: true })).toBe(false);
    expect(showReplayMapUnit({ isInfantryMember: false })).toBe(true);
  });
});

describe('live domination samples', () => {
  const samples = [
    { timeSeconds: 2, value: 0.5 },
    { timeSeconds: 7, value: 0.61 },
    { timeSeconds: 12, value: 0.58 },
  ];

  it('holds the latest recorded value without interpolating', () => {
    expect(dominationSharesAt(samples, 6.99, 'USA', ['USA', 'USSR'])).toEqual([
      { faction: 'USA', pct: 0.5 },
      { faction: 'USSR', pct: 0.5 },
    ]);
    expect(dominationSharesAt(samples, 7, 'USA', ['USA', 'USSR'])).toEqual([
      { faction: 'USA', pct: 0.61 },
      { faction: 'USSR', pct: 0.39 },
    ]);
  });

  it('resolves the allied opponent while preserving USA versus NATO', () => {
    expect(
      dominationSharesAt(samples, 20, 'USSR', ['NATO', 'USSR'])?.map((share) => share.faction),
    ).toEqual(['USSR', 'NATO']);
  });

  it('anchors a spectator curve from the winner-inferred final shares', () => {
    expect(
      dominationPlaybackAnchor(
        samples,
        null,
        [
          { faction: 'USSR', pct: 0.58 },
          { faction: 'USA', pct: 0.42 },
        ],
        'winnerInferred',
      ),
    ).toBe('USSR');
    expect(dominationSharesAt(samples, 7, 'USSR', ['USA', 'USSR'])).toEqual([
      { faction: 'USSR', pct: 0.61 },
      { faction: 'USA', pct: 0.39 },
    ]);
  });

  it('does not invent a spectator anchor without winner-inferred evidence', () => {
    expect(dominationPlaybackAnchor(samples, null, null, null)).toBeNull();
    expect(
      dominationPlaybackAnchor(
        samples,
        null,
        [
          { faction: 'USSR', pct: 0.7 },
          { faction: 'USA', pct: 0.3 },
        ],
        'winnerInferred',
      ),
    ).toBeNull();
  });

  it('stays hidden before the first sample or without deterministic factions', () => {
    expect(dominationSharesAt(samples, 1.99, 'USA', ['USA', 'USSR'])).toBeNull();
    expect(dominationSharesAt(samples, 20, null, ['USA', 'USSR'])).toBeNull();
    expect(dominationSharesAt(samples, 20, 'USSR', ['USA', 'NATO', 'USSR'])).toBeNull();
  });
});
