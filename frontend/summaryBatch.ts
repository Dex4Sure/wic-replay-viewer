import { cooperativeSort, LIBRARY_CHUNK_SIZE, yieldToUi } from './cooperative';
import type { ReplaySummary } from './types';

export function compareReplaySummaries(left: ReplaySummary, right: ReplaySummary): number {
  return (
    right.dateTime.localeCompare(left.dateTime) ||
    left.fileName.localeCompare(right.fileName, undefined, { sensitivity: 'base' }) ||
    left.path.localeCompare(right.path)
  );
}

export function mergeReplaySummaries(
  current: ReplaySummary[],
  pending: Iterable<ReplaySummary>,
): ReplaySummary[] {
  const byPath = new Map(current.map((summary) => [summary.path, summary]));
  for (const summary of pending) {
    byPath.set(summary.path, summary);
  }
  return [...byPath.values()].sort(compareReplaySummaries);
}

export function removeReplaySummaries(
  current: ReplaySummary[],
  removedPaths: Iterable<string>,
): ReplaySummary[] {
  const removed = new Set(removedPaths);
  const retained = removed.size ? current.filter((summary) => !removed.has(summary.path)) : current;
  return retained.length === current.length ? current : retained;
}

/** Bulk import counterpart: preserve last-write-wins semantics without a long synchronous sort. */
export async function mergeReplaySummariesAsync(
  current: ReplaySummary[],
  pending: Iterable<ReplaySummary>,
  removedPaths: Iterable<string> = [],
  yieldTask = yieldToUi,
): Promise<ReplaySummary[]> {
  const removed = new Set(removedPaths);
  const byPath = new Map<string, ReplaySummary>();
  let count = 0;
  for (const source of [current, pending]) {
    for (const row of source) {
      if (!removed.has(row.path)) {
        byPath.set(row.path, row);
      }
      if (++count % LIBRARY_CHUNK_SIZE === 0) {
        await yieldTask();
      }
    }
  }
  return cooperativeSort([...byPath.values()], compareReplaySummaries, yieldTask);
}
