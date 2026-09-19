import type { ReplaySummary } from './types';

export interface ReplayLibraryStats {
  total: number;
  parsed: number;
  failed: number;
  stale: number;
}

/**
 * Classify each cached row exactly once. A stale row needs another parser pass,
 * so an error retained from an older parser contract is not presented as a
 * current failure until that refresh completes.
 */
export function replayLibraryStats(rows: ReplaySummary[]): ReplayLibraryStats {
  const stats: ReplayLibraryStats = { total: rows.length, parsed: 0, failed: 0, stale: 0 };
  for (const row of rows) {
    if (!row.cacheKey) {
      stats.stale += 1;
    } else if (row.parseError) {
      stats.failed += 1;
    } else {
      stats.parsed += 1;
    }
  }
  return stats;
}
