import { describe, expect, it } from 'vitest';

import { updateReplaySelection } from './replaySelection';

const visible = ['/one.wicdemo', '/two.wicdemo', '/three.wicdemo', '/four.wicdemo'];

describe('replay library selection', () => {
  it('replaces the selection on an ordinary click', () => {
    const result = updateReplaySelection(
      visible,
      new Set(['/one.wicdemo', '/two.wicdemo']),
      '/three.wicdemo',
      '/one.wicdemo',
      { toggle: false, range: false },
    );

    expect([...result.paths]).toEqual(['/three.wicdemo']);
    expect(result.anchor).toBe('/three.wicdemo');
  });

  it('toggles individual replays without disturbing the rest', () => {
    const removed = updateReplaySelection(
      visible,
      new Set(['/one.wicdemo', '/two.wicdemo']),
      '/one.wicdemo',
      '/two.wicdemo',
      { toggle: true, range: false },
    );
    const added = updateReplaySelection(visible, removed.paths, '/three.wicdemo', removed.anchor, {
      toggle: true,
      range: false,
    });

    expect([...added.paths]).toEqual(['/two.wicdemo', '/three.wicdemo']);
  });

  it('selects a visible range and can add another range', () => {
    const range = updateReplaySelection(
      visible,
      new Set(['/four.wicdemo']),
      '/three.wicdemo',
      '/one.wicdemo',
      { toggle: false, range: true },
    );
    expect([...range.paths]).toEqual(['/one.wicdemo', '/two.wicdemo', '/three.wicdemo']);

    const additive = updateReplaySelection(
      visible,
      new Set(['/four.wicdemo']),
      '/two.wicdemo',
      '/one.wicdemo',
      { toggle: true, range: true },
    );
    expect([...additive.paths]).toEqual(['/four.wicdemo', '/one.wicdemo', '/two.wicdemo']);
  });

  it('starts a new range when the previous anchor is not visible', () => {
    const result = updateReplaySelection(
      visible,
      new Set(['/one.wicdemo']),
      '/three.wicdemo',
      '/hidden.wicdemo',
      { toggle: false, range: true },
    );

    expect([...result.paths]).toEqual(['/three.wicdemo']);
    expect(result.anchor).toBe('/three.wicdemo');
  });
});
