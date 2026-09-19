import { describe, expect, it } from 'vitest';
import { displayLocationPath } from './locationPath';

describe('displayLocationPath', () => {
  it('shows ordinary drive and network paths without changing other namespaces', () => {
    expect(displayLocationPath(String.raw`\\?\C:\Users\Carita\Replay`)).toBe(
      String.raw`C:\Users\Carita\Replay`,
    );
    expect(displayLocationPath(String.raw`\\?\UNC\server\share\Replay`)).toBe(
      String.raw`\\server\share\Replay`,
    );
    for (const path of [
      '/home/player/Replay',
      String.raw`C:\Replay`,
      String.raw`\\?\Volume{abc}\Replay`,
    ]) {
      expect(displayLocationPath(path)).toBe(path);
    }
  });
});
