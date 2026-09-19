import { cooperativeSort, LIBRARY_CHUNK_SIZE, yieldToUi } from './cooperative';
import { folderLabel, parentFolder, type ReplayFolderGroup } from './libraryGroups';
import { replayLibraryStats, type ReplayLibraryStats } from './libraryStats';
import { indexReplaySummaries } from './replaySearch';
import type { ReplaySummary } from './types';

interface PreparedLibrary {
  stats: ReplayLibraryStats;
  groups: ReplayFolderGroup[];
}

// Arrays and rows are immutable once published. Weak keys release superseded libraries.
const prepared = new WeakMap<ReplaySummary[], PreparedLibrary>();
export const preparedLibrary = (rows: ReplaySummary[]): PreparedLibrary | undefined =>
  prepared.get(rows);

/** Prepare expensive projections before the single reactive publication of a library. */
export async function prepareLibrary(rows: ReplaySummary[], yieldTask = yieldToUi): Promise<void> {
  if (prepared.has(rows)) {
    return;
  }
  const stats: ReplayLibraryStats = { total: 0, parsed: 0, failed: 0, stale: 0 };
  const folders = new Map<string, ReplayFolderGroup>();
  for (let start = 0; start < rows.length; start += LIBRARY_CHUNK_SIZE) {
    const chunk = rows.slice(start, start + LIBRARY_CHUNK_SIZE);
    const counts = replayLibraryStats(chunk);
    stats.total += counts.total;
    stats.parsed += counts.parsed;
    stats.failed += counts.failed;
    stats.stale += counts.stale;
    indexReplaySummaries(chunk);
    for (const row of chunk) {
      const path = parentFolder(row.path);
      let group = folders.get(path);
      if (!group) {
        group = { path, label: folderLabel(path), rows: [] };
        folders.set(path, group);
      }
      group.rows.push(row);
    }
    if (rows.length > LIBRARY_CHUNK_SIZE) {
      await yieldTask();
    }
  }
  const groups = await cooperativeSort(
    [...folders.values()],
    (left, right) => left.path.localeCompare(right.path, undefined, { sensitivity: 'base' }),
    yieldTask,
  );
  prepared.set(rows, { stats, groups });
}
