import type { DetailView, MapBounds } from './types';

export type TacticalAidRow = DetailView['tacticalAidRows'][number];

export interface ProjectedAidGroup {
  key: string;
  left: number;
  top: number;
  rows: TacticalAidRow[];
}

export function projectPosition(
  position: [number, number, number],
  bounds: MapBounds,
): { left: number; top: number } | null {
  const width = bounds.maxX - bounds.minX;
  const height = bounds.maxZ - bounds.minZ;
  if (width <= 0 || height <= 0) {
    return null;
  }
  // The shipped overview is rotated 180 degrees relative to world x/z. This
  // matches the game's own static-radar transform: x decreases left-to-right,
  // while z increases top-to-bottom.
  const left = (bounds.maxX - position[0]) / width;
  const top = (position[2] - bounds.minZ) / height;
  if (![left, top].every(Number.isFinite) || left < 0 || left > 1 || top < 0 || top > 1) {
    return null;
  }
  return { left, top };
}

export function groupTacticalAid(
  rows: TacticalAidRow[],
  bounds: MapBounds,
): { groups: ProjectedAidGroup[]; omitted: number } {
  const groups = new Map<string, ProjectedAidGroup>();
  let omitted = 0;
  for (const row of rows) {
    const point = projectPosition(row.position, bounds);
    if (!point) {
      omitted += 1;
      continue;
    }
    const key = `${row.position[0]}:${row.position[2]}`;
    const existing = groups.get(key);
    if (existing) {
      existing.rows.push(row);
    } else {
      groups.set(key, { key, ...point, rows: [row] });
    }
  }
  return { groups: [...groups.values()], omitted };
}
