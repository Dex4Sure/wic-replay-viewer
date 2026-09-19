import { expect, it } from 'vitest';
import { activeAreaEffects } from './replayAreaEffects';
import type { PlaybackView } from './types';

it('keeps actual impact sequences, cloud lifetimes and deterministic backward seeking', () => {
  const effects: PlaybackView['areaEffects'] = [
    { timeSeconds: 1, position: [0, 0, 0], radius: 14, kind: 'napalm', durationSeconds: 25 },
    { timeSeconds: 10, position: [10, 0, 0], radius: 6, kind: 'explosion', durationSeconds: 1.5 },
    { timeSeconds: 11, position: [20, 0, 0], radius: 35, kind: 'explosion', durationSeconds: 1.5 },
    { timeSeconds: 12, position: [30, 0, 0], radius: 65, kind: 'chemical', durationSeconds: 25 },
  ];
  expect(activeAreaEffects(effects, 0)).toEqual([]);
  expect(activeAreaEffects(effects, 10).map((e) => e.kind)).toEqual(['napalm', 'explosion']);
  expect(activeAreaEffects(effects, 11.5).map((e) => e.key)).toEqual([0, 2]);
  const beforeSeek = activeAreaEffects(effects, 12);
  expect(activeAreaEffects(effects, 37)).toEqual([]);
  expect(activeAreaEffects(effects, 12)).toEqual(beforeSeek);
  expect(activeAreaEffects(effects, 26).map((e) => e.kind)).toEqual(['chemical']);
});
