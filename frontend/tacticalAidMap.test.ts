import { describe, expect, it } from 'vitest';

import { groupTacticalAid, projectPosition } from './tacticalAidMap';

const bounds = { minX: 100, minZ: 200, maxX: 500, maxZ: 600 };
const terrainBounds = { minX: 0, minZ: 0, maxX: 1536, maxZ: 1536 };

describe('tactical aid map projection', () => {
  it('matches the overview image rotation used by the game', () => {
    expect(projectPosition([100, 0, 200], bounds)).toEqual({ left: 1, top: 0 });
    expect(projectPosition([500, 0, 600], bounds)).toEqual({ left: 0, top: 1 });
    expect(projectPosition([300, 10, 400], bounds)).toEqual({ left: 0.5, top: 0.5 });
  });

  it('projects the exact heightmap-derived terrain rectangle', () => {
    expect(projectPosition([0, 0, 0], terrainBounds)).toEqual({ left: 1, top: 0 });
    expect(projectPosition([1536, 0, 1536], terrainBounds)).toEqual({ left: 0, top: 1 });
    expect(projectPosition([768, 25, 768], terrainBounds)).toEqual({ left: 0.5, top: 0.5 });
  });

  it('groups exact x/z duplicates and omits out-of-bounds rows', () => {
    const row = {
      timeSeconds: 1,
      support: 'Air strike',
      player: 'Alpha',
      faction: 'USA',
      playerAttribution: 'exact',
      honorsCost: 10,
      position: [300, 7, 400] as [number, number, number],
    };
    const result = groupTacticalAid(
      [
        row,
        { ...row, timeSeconds: 2, position: [300, 99, 400] },
        { ...row, position: [99, 0, 400] },
      ],
      bounds,
    );
    expect(result.groups).toHaveLength(1);
    expect(result.groups[0]?.rows).toHaveLength(2);
    expect(result.omitted).toBe(1);
  });
});
