import { expect, it, vi } from 'vitest';
import { cooperativeSort } from './cooperative';
import { prepareLibrary, preparedLibrary } from './libraryPreparation';
import { groupReplaysByFolder } from './libraryGroups';
import { replayLibraryStats } from './libraryStats';
import { filterReplaySearch, indexReplaySummaries, parseReplaySearch } from './replaySearch';
import {
  mergeReplaySummaries,
  mergeReplaySummariesAsync,
  removeReplaySummaries,
} from './summaryBatch';
import { replaySummary } from './test/factories';

it('matches synchronous merge, removal, stats, groups, and search for 10,000 rows while yielding', async () => {
  const rows = Array.from({ length: 10000 }, (_, index) =>
    replaySummary({
      path: `/folder${index % 300}/${index}.wicdemo`,
      fileName: `${index % 91}.wicdemo`,
      dateTime: `2026-09-${String((index % 28) + 1).padStart(2, '0')}`,
      cacheKey: index % 5 ? 'current' : '',
      parseError: index % 7 ? null : 'invalid',
    }),
  );
  const updates = rows.slice(0, 600).map((row) => ({ ...row, fileName: 'updated.wicdemo' }));
  const removed = rows.slice(10, 35).map((row) => row.path);
  let heartbeat = 0;
  const timer = setInterval(() => heartbeat++, 0);
  try {
    const merged = await mergeReplaySummariesAsync(rows, updates, removed);
    expect(merged).toEqual(removeReplaySummaries(mergeReplaySummaries(rows, updates), removed));
    await prepareLibrary(merged);
    expect(preparedLibrary(merged)).toEqual({
      stats: replayLibraryStats(merged),
      groups: groupReplaysByFolder(merged),
    });
    expect(
      filterReplaySearch(indexReplaySummaries(merged), parseReplaySearch('updated')),
    ).toHaveLength(575);
    expect(heartbeat).toBeGreaterThan(20);
    const yieldTask = vi.fn(async () => {});
    await prepareLibrary(merged, yieldTask);
    expect(yieldTask).not.toHaveBeenCalled();
  } finally {
    clearInterval(timer);
  }
});

it.each([0, 1, 255, 256, 257, 513, 1025])(
  'sorts %i rows stably without modifying inputs',
  async (count) => {
    const input = Array.from({ length: count }, (_, id) => ({ id, key: id % 17 }));
    const original = [...input];
    const compare = (left: (typeof input)[number], right: (typeof input)[number]) =>
      left.key - right.key;
    const yieldTask = vi.fn(async () => {});
    expect(await cooperativeSort(input, compare, yieldTask)).toEqual([...input].sort(compare));
    expect(input).toEqual(original);
    expect(yieldTask.mock.calls.length > 0).toBe(count > 256);
  },
);

it('prepares an empty library and deduplicates repeated pending paths', async () => {
  const old = replaySummary({ path: '/a' });
  const latest = { ...old, fileName: 'latest' };
  expect(await mergeReplaySummariesAsync([], [old, latest])).toEqual([latest]);
  const rows: (typeof old)[] = [];
  await prepareLibrary(rows);
  expect(preparedLibrary(rows)).toEqual({
    stats: { total: 0, parsed: 0, failed: 0, stale: 0 },
    groups: [],
  });
});
