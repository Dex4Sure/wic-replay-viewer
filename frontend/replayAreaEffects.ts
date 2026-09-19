import type { PlaybackView } from './types';

// The backend sorts events by recording time. Search only the longest supported
// lifetime (25 seconds), rather than scanning the replay on each animation frame.
export function activeAreaEffects(effects: PlaybackView['areaEffects'], time: number) {
  const bound = (value: number) => {
    let low = 0,
      high = effects.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (effects[middle]!.timeSeconds <= value) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low;
  };
  const visible = [];
  const end = bound(time);
  for (let index = bound(time - 25); index < end; index++) {
    const effect = effects[index]!;
    const age = time - effect.timeSeconds;
    if (age < effect.durationSeconds) {
      visible.push({ ...effect, key: index, age });
    }
  }
  return visible;
}
