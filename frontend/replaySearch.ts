import type { ReplaySummary } from './types';

interface SearchEntry {
  row: ReplaySummary;
  text: string;
  map: string;
  playerNames: string;
  players: { name: string; faction: string }[];
  factions: string[];
}

export interface ReplaySearchQuery {
  text: string[];
  player: string[];
  map: string[];
  faction: string[];
  empty: boolean;
}

const normalize = (value: string | null | undefined) => value?.toLocaleLowerCase() ?? '';
const prepared = new WeakMap<ReplaySummary, SearchEntry>();

/** Summaries are replaced on import/rename, never edited in place. Reuse unchanged rows. */
export function indexReplaySummaries(rows: readonly ReplaySummary[]): SearchEntry[] {
  return rows.map((row) => {
    const existing = prepared.get(row);
    if (existing) {
      return existing;
    }
    const entry: SearchEntry = {
      row,
      // A separator prevents phrases from matching across unrelated fields.
      text: [
        row.fileName,
        row.replayName,
        row.serverName,
        row.mapName,
        row.mapDisplayName,
        row.gameMode,
        row.serverModes,
        row.format,
        row.dateTime,
        row.recorder,
        row.playerNames,
        ...row.searchPlayers.map((player) => player.name),
        row.factions,
        row.parseError,
      ]
        .map(normalize)
        .join('\0'),
      map: [row.mapName, row.mapDisplayName].map(normalize).join('\0'),
      playerNames: normalize(row.playerNames),
      players: row.searchPlayers.map((player) => ({
        name: normalize(player.name),
        faction: normalize(player.faction),
      })),
      factions: row.factions.split(',').map((faction) => normalize(faction.trim())),
    };
    prepared.set(row, entry);
    return entry;
  });
}

/** Parse once per query, including quoted names/maps; unknown prefixes remain literal text. */
export function parseReplaySearch(input: string): ReplaySearchQuery {
  const query: ReplaySearchQuery = {
    text: [],
    player: [],
    map: [],
    faction: [],
    empty: true,
  };
  let token = '';
  let quoted = false;
  const addToken = () => {
    if (!token) {
      return;
    }
    const colon = token.indexOf(':');
    const field = token.slice(0, colon);
    const value = token.slice(colon + 1);
    if (colon > 0 && value && (field === 'player' || field === 'map' || field === 'faction')) {
      query[field].push(value);
    } else if (token === 'usa' || token === 'nato' || token === 'ussr') {
      query.faction.push(token);
    } else {
      query.text.push(token);
    }
    query.empty = false;
    token = '';
  };
  for (const character of normalize(input)) {
    if (character === '"') {
      quoted = !quoted;
    } else if (!quoted && /\s/.test(character)) {
      addToken();
    } else {
      token += character;
    }
  }
  addToken();
  return query;
}

/** Only prepared strings are inspected here: no parsing, normalization, or detail requests. */
export function filterReplaySearch(
  index: readonly SearchEntry[],
  query: ReplaySearchQuery,
): ReplaySummary[] {
  // Recognize player terms across the library so a filename alone cannot satisfy
  // a player/faction query in another row. Legacy names identify terms, but never
  // supply a faction association: only structured result players can do that.
  const playerTerms = query.faction.length
    ? query.text.filter((term) =>
        index.some(
          (entry) =>
            entry.playerNames.includes(term) ||
            entry.players.some((player) => player.name.includes(term)),
        ),
      )
    : [];
  const names = [...query.player, ...playerTerms];
  return index
    .filter((entry) => {
      if (!query.text.every((term) => entry.text.includes(term))) {
        return false;
      }
      if (!query.map.every((term) => entry.map.includes(term))) {
        return false;
      }
      if (names.length) {
        return names.every((name) =>
          entry.players.some(
            (player) =>
              player.name.includes(name) &&
              query.faction.every((faction) => player.faction === faction),
          ),
        );
      }
      return query.faction.every((faction) => entry.factions.includes(faction));
    })
    .map((entry) => entry.row);
}
