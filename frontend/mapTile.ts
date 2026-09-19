/// Procedural library tiles keyed by a replay's map.
///
/// The viewer ships no map artwork, so a tile is derived from the map name
/// alone. The point is not to depict the terrain — it is to give every map a
/// stable silhouette and colour so a long library list can be scanned by shape
/// instead of read line by line. The same map therefore has to produce the same
/// tile on every launch and on every machine, which rules out anything seeded
/// by row order, import time, or `Math.random`.

/// Hues are picked from a curated ring rather than the full circle so the tiles
/// stay inside the Massgate palette's cold, desaturated mood. Full-spectrum
/// hashing produces pinks and limes that fight the surrounding UI.
const HUES = [204, 196, 188, 172, 148, 96, 74, 44, 24, 12, 348, 218];

/// FNV-1a. Any stable 32-bit hash would do; this one is short and needs no
/// dependency.
export function hashMapName(name: string): number {
  let hash = 0x811c9dc5;
  for (let index = 0; index < name.length; index += 1) {
    hash ^= name.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
}

/// A deterministic 0..1 generator seeded from the map hash, so the ridge shape
/// is a function of the name and nothing else.
function sequence(seed: number): () => number {
  let state = seed || 1;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

/// Two to three letters standing in for the map. Multi-word names give their
/// word initials, a single word gives its opening letters, so "Cold Winter"
/// reads as CW and "Vineyards" as VI.
export function mapInitials(name: string): string {
  const words = name.split(/[^A-Za-z0-9]+/).filter(Boolean);
  if (!words.length) {
    return '??';
  }
  if (words.length === 1) {
    return words[0].slice(0, 2).toUpperCase();
  }
  return words
    .slice(0, 3)
    .map((word) => word[0])
    .join('')
    .toUpperCase();
}

export interface MapTileArt {
  initials: string;
  skyTop: string;
  skyBottom: string;
  ridgeFar: string;
  ridgeNear: string;
  ridgeFarPath: string;
  ridgeNearPath: string;
  /// Unique per map, so two tiles rendered side by side cannot share a gradient id.
  gradientId: string;
}

export const TILE_WIDTH = 96;
export const TILE_HEIGHT = 80;

/// A ridge line across the tile, closed down to the bottom edge so it fills as
/// a silhouette. `base` is the ridge's resting height and `amplitude` how far
/// its peaks travel, both measured from the bottom.
function ridgePath(next: () => number, base: number, amplitude: number): string {
  const steps = 4;
  const points: string[] = [];
  for (let index = 0; index <= steps; index += 1) {
    const x = (TILE_WIDTH / steps) * index;
    const y = TILE_HEIGHT - base - next() * amplitude;
    points.push(`${x.toFixed(1)},${y.toFixed(1)}`);
  }
  return `M0,${TILE_HEIGHT} L${points.join(' L')} L${TILE_WIDTH},${TILE_HEIGHT} Z`;
}

/// The tile for a map. An empty or unknown name still yields a valid tile, so a
/// replay whose map the parser could not resolve is drawn as neutral slate
/// rather than dropping out of the row layout.
export function mapTileArt(mapName: string): MapTileArt {
  const key = mapName.trim() || 'Unknown';
  const hash = hashMapName(key);
  const hue = HUES[hash % HUES.length];
  const next = sequence(hash);

  return {
    initials: mapInitials(key),
    skyTop: `hsl(${hue} 26% 24%)`,
    skyBottom: `hsl(${hue} 30% 12%)`,
    ridgeFar: `hsl(${hue} 22% 30%)`,
    ridgeNear: `hsl(${(hue + 8) % 360} 26% 17%)`,
    ridgeFarPath: ridgePath(next, 26, 22),
    ridgeNearPath: ridgePath(next, 6, 18),
    gradientId: `map-tile-${hash.toString(36)}`,
  };
}
