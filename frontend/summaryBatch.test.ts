import { describe, expect, it } from 'vitest';

import { mergeReplaySummaries, removeReplaySummaries } from './summaryBatch';
import type { ReplaySummary } from './types';

function summary(path: string, dateTime: string, mapName = 'old'): ReplaySummary {
  return {
    path,
    fileName: `${path}.wicdemo`,
    replayName: null,
    serverName: '',
    fingerprint: { size: 1, modifiedNs: 1 },
    cacheKey: 'test',
    mapName,
    mapDisplayName: mapName,
    gameMode: 'Domination',
    serverModes: '',
    format: '',
    dateTime,
    durationSeconds: 60,
    recordingSeconds: 75,
    winner: null,
    playerCount: 0,
    searchPlayers: [],
    playerNames: '',
    factions: '',
    recorder: null,
    recorderFaction: null,
    incomplete: false,
    parseError: null,
    importedAt: 1,
  };
}

describe('replay summary batches', () => {
  it('replaces existing paths, appends new paths, and sorts once after merging', () => {
    const result = mergeReplaySummaries(
      [summary('/a', '2026-01-01', 'old'), summary('/b', '2026-01-03')],
      [summary('/a', '2026-01-04', 'updated'), summary('/c', '2026-01-02')],
    );

    expect(result.map((row) => row.path)).toEqual(['/a', '/b', '/c']);
    expect(result[0]?.mapName).toBe('updated');
  });

  it('keeps only the latest pending value for a replay path', () => {
    const result = mergeReplaySummaries(
      [],
      [summary('/a', '2026-01-01', 'first'), summary('/a', '2026-01-02', 'second')],
    );

    expect(result).toHaveLength(1);
    expect(result[0]?.mapName).toBe('second');
  });

  it('removes paths pruned by a completed backend scan', () => {
    const current = [summary('/a', '2026-01-01'), summary('/b', '2026-01-02')];

    expect(removeReplaySummaries(current, ['/a']).map((row) => row.path)).toEqual(['/b']);
    expect(removeReplaySummaries(current, [])).toBe(current);
  });
});
