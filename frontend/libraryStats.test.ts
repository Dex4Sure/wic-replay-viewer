import { describe, expect, it } from 'vitest';

import { replayLibraryStats } from './libraryStats';
import type { ReplaySummary } from './types';

function replay(cacheKey: string, parseError: string | null): ReplaySummary {
  return { cacheKey, parseError } as ReplaySummary;
}

describe('replay library statistics', () => {
  it('separates parsed rows, current failures, and stale rows', () => {
    expect(
      replayLibraryStats([
        replay('current', null),
        replay('current', 'File too small'),
        replay('', null),
        replay('', 'Failure retained from the old parser'),
      ]),
    ).toEqual({ total: 4, parsed: 1, failed: 1, stale: 2 });
  });
});
