import { describe, expect, it } from 'vitest';

import { groupReplaysByFolder, parentFolder } from './libraryGroups';
import type { ReplaySummary } from './types';

function replay(path: string): ReplaySummary {
  return { path, fileName: path.split(/[\\/]/).at(-1) ?? path } as ReplaySummary;
}

describe('replay library folder grouping', () => {
  it('groups every replay by its actual parent directory', () => {
    const groups = groupReplaysByFolder([
      replay('/archive/clan/one.wicdemo'),
      replay('/archive/public/two.wicdemo'),
      replay('/archive/clan/three.wicdemo'),
      replay('/other/four.wicdemo'),
    ]);

    expect(groups.map((group) => [group.path, group.label, group.rows.length])).toEqual([
      ['/archive/clan', 'clan', 2],
      ['/archive/public', 'public', 1],
      ['/other', 'other', 1],
    ]);
  });

  it('handles Windows replay paths', () => {
    expect(parentFolder('C:\\Replays\\Clan\\match.wicdemo')).toBe('C:/Replays/Clan');
  });
});
