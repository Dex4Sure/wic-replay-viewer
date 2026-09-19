import { describe, expect, it } from 'vitest';

import { filterReplaySearch, indexReplaySummaries, parseReplaySearch } from './replaySearch';

const matchesReplaySearch = (row: ReturnType<typeof replaySummary>, query: string) =>
  filterReplaySearch(indexReplaySummaries([row]), parseReplaySearch(query)).length === 1;
import { replaySummary } from './test/factories';

const row = replaySummary({
  fileName: 'league-final.wicdemo',
  replayName: 'Deciding round',
  serverName: 'WiCGate Ranked Server',
  mapName: 'maps/ustown4/ustown4.ice',
  mapDisplayName: 'Seaside',
  gameMode: 'Domination',
  serverModes: 'Ranked · Bots',
  format: '4vs4',
  dateTime: '2010-01-02T03:04:00Z',
  winner: 'NATO',
  recorder: 'Dexter',
  recorderFaction: 'USA',
  playerNames: 'Alice, Bravo',
  factions: 'USA, USSR',
  parseError: 'Recovered trailing data',
});

describe('replay library search', () => {
  it.each([
    'league-final',
    'deciding',
    'wicgate',
    'ustown4',
    'seaside',
    'domination',
    'bots',
    '4vs4',
    '2010-01-02',
    'dexter',
    'bravo',
    'ussr',
    'trailing data',
  ])('searches the cached summary field containing %s', (query) => {
    expect(matchesReplaySearch(row, query)).toBe(true);
  });

  it('matches every term independently across metadata fields', () => {
    expect(matchesReplaySearch(row, '  WiCGate  SEASIDE ussr domination  ')).toBe(true);
    expect(matchesReplaySearch(row, 'wicgate seaside missing')).toBe(false);
  });

  it('does not treat the winner or full path as searchable metadata', () => {
    expect(matchesReplaySearch(row, 'nato')).toBe(false);
    expect(
      matchesReplaySearch(
        replaySummary({
          path: '/private/hidden-folder/example.wicdemo',
          fileName: 'example.wicdemo',
        }),
        'hidden-folder',
      ),
    ).toBe(false);
  });

  it('matches an empty query', () => {
    expect(matchesReplaySearch(row, '   ')).toBe(true);
  });
});

describe('precise cached player search', () => {
  const nato = replaySummary({
    path: '/nato.wicdemo',
    mapDisplayName: 'Riviera',
    playerNames: 'GeneralX, Other',
    factions: 'NATO, USSR',
    searchPlayers: [
      { name: 'GeneralX', faction: 'NATO' },
      { name: 'Other', faction: 'USSR' },
    ],
    winner: 'USSR',
  });
  const ussr = replaySummary({
    ...nato,
    path: '/ussr.wicdemo',
    searchPlayers: [
      { name: 'GeneralX', faction: 'USSR' },
      { name: 'Other', faction: 'NATO' },
    ],
  });
  const unknown = replaySummary({
    ...nato,
    path: '/unknown.wicdemo',
    searchPlayers: [{ name: 'GeneralX', faction: null }],
  });
  const index = indexReplaySummaries([nato, ussr, unknown]);
  const find = (query: string) => filterReplaySearch(index, parseReplaySearch(query));

  it('associates faction with the named player, independent of token order', () => {
    expect(find('player:generalx map:riviera faction:nato')).toEqual([nato]);
    expect(find('FACTION:USSR map:riv player:GENERALX')).toEqual([ussr]);
    expect(find('generalx riviera nato')).toEqual([nato]);
    expect(find('USSR RIVIERA GENERALX')).toEqual([ussr]);
    expect(find('player:generalx')).toEqual([nato, ussr, unknown]);
  });

  it('limits map filters to their field and faction values to exact matches', () => {
    expect(find('map:generalx')).toEqual([]);
    expect(find('winner:ussr')).toEqual([]);
    expect(find('winner:nato')).toEqual([]);
    expect(find('faction:us')).toEqual([]);
    expect(find('faction:nato faction:ussr')).toEqual([nato, ussr, unknown]);
    expect(find('player:generalx player:other faction:nato')).toEqual([]);
  });

  it('uses any result player, not the recorder, and rejects filename-only player matches', () => {
    const filenameOnly = replaySummary({
      ...nato,
      path: '/filename-only.wicdemo',
      fileName: 'GeneralX-Riviera.wicdemo',
      playerNames: 'Other',
      searchPlayers: [{ name: 'Other', faction: 'NATO' }],
    });
    const rows = indexReplaySummaries([nato, ussr, unknown, filenameOnly]);
    expect(filterReplaySearch(rows, parseReplaySearch('generalx nato riviera'))).toEqual([nato]);
    expect(nato.recorder).not.toBe('GeneralX');
    expect(find('riviera nato')).toEqual([nato, ussr, unknown]);
    expect(find('player:generalx nato riviera')).toEqual([nato]);
    expect(find('generalx faction:nato map:riviera')).toEqual([nato]);
  });

  it('supports quoted names and phrases, including an unfinished quote while typing', () => {
    const rows = indexReplaySummaries([
      replaySummary({
        playerNames: 'Weed Queen',
        searchPlayers: [{ name: 'Weed Queen', faction: 'USSR' }],
      }),
    ]);
    for (const query of [
      'weed queen ussr',
      '"weed queen" ussr',
      'player:"weed queen"',
      'player:"weed queen',
      '"weed queen"',
    ]) {
      expect(filterReplaySearch(rows, parseReplaySearch(query))).toHaveLength(1);
    }
    expect(filterReplaySearch(rows, parseReplaySearch('"queen weed"'))).toHaveLength(0);
    expect(find('"generalx riviera"')).toHaveLength(0);
  });

  it('keeps unsupported prefixes and minus signs literal and incomplete filters nonmatching', () => {
    expect(parseReplaySearch('unknown:value -player:generalx player:').text).toEqual([
      'unknown:value',
      '-player:generalx',
      'player:',
    ]);
    expect(find('-player:generalx')).toEqual([]);
    expect(find('player:')).toEqual([]);
    expect(parseReplaySearch('  ""  ').empty).toBe(true);
  });

  it('reuses unchanged entries and replaces renamed or reimported rows without stale matches', () => {
    const initial = indexReplaySummaries([nato, ussr]);
    const renamed = {
      ...nato,
      playerNames: 'Replacement',
      searchPlayers: [{ name: 'Replacement', faction: 'NATO' }],
    };
    const updated = indexReplaySummaries([renamed, ussr]);
    expect(updated[1]).toBe(initial[1]);
    expect(updated[0]).not.toBe(initial[0]);
    expect(filterReplaySearch(updated, parseReplaySearch('player:generalx faction:nato'))).toEqual(
      [],
    );
    expect(filterReplaySearch(updated, parseReplaySearch('player:replacement'))).toEqual([renamed]);
    expect(
      filterReplaySearch(indexReplaySummaries([]), parseReplaySearch('player:generalx')),
    ).toEqual([]);
  });

  it('does not infer structured players from a legacy comma-separated name list', () => {
    const legacy = replaySummary({
      playerNames: 'GeneralX',
      factions: 'NATO, USSR',
      searchPlayers: [],
    });
    expect(matchesReplaySearch(legacy, 'generalx')).toBe(true);
    expect(matchesReplaySearch(legacy, 'generalx nato')).toBe(false);
    expect(matchesReplaySearch(legacy, 'player:generalx faction:nato')).toBe(false);
  });
});
