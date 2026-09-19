import type { DominationShare, MatchEnding, MatchTiming, PlayerView, TimelineRow } from './types';

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) {
    return 'Unknown';
  }
  const rounded = Math.max(0, Math.round(seconds));
  const minutes = Math.floor(rounded / 60);
  const remainder = rounded % 60;
  return `${minutes}:${remainder.toString().padStart(2, '0')}`;
}

/// An event timestamp as `M:SS.s`, matching the cached chat labels.
///
/// Timestamps run to the length of a recording, where raw seconds stop being
/// readable well before the end of a round. Tenths are kept because the events
/// resolve to them and simultaneous ones need to stay distinguishable. The whole
/// value is converted to tenths before it is split, so a timestamp just short of
/// a minute cannot round into a `0:60.0`.
export function formatTimestamp(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined || !Number.isFinite(seconds)) {
    return '—';
  }
  const tenths = Math.round(Math.max(0, seconds) * 10);
  const minutes = Math.floor(tenths / 600);
  const whole = Math.floor((tenths % 600) / 10);
  return `${minutes}:${whole.toString().padStart(2, '0')}.${tenths % 10}`;
}

/// Replace only the generic command-point fallback with the map's own localized
/// UI label. Missing installation data, malformed labels, and invalid teams keep
/// the backend's evidence-preserving generic description.
export function timelineDescription(
  row: TimelineRow,
  commandPointNames: Record<string, string> | null | undefined,
): string {
  if (row.commandPointId === null || row.commandPointTeam === null) {
    return row.description;
  }
  const name = commandPointNames?.[String(row.commandPointId)]?.trim();
  if (!name) {
    return row.description;
  }
  if (row.commandPointTeam === 0) {
    return `${name} became neutral`;
  }
  const faction = ['USA', 'NATO', 'USSR'][row.commandPointTeam - 1];
  if (faction) {
    return `${faction} captured ${name}`;
  }
  return row.description;
}

export interface PlayerNameSegment {
  text: string;
  tone: 'allied' | 'soviet' | null;
}

function playerNameIndex(description: string, name: string, cursor: number): number {
  let index = description.indexOf(name, cursor);
  while (index >= 0) {
    const before = index > 0 ? description[index - 1] : '';
    const afterIndex = index + name.length;
    const after = afterIndex < description.length ? description[afterIndex] : '';
    const startsInsideWord = /[\p{L}\p{N}_]/u.test(before) && /[\p{L}\p{N}_]/u.test(name[0] ?? '');
    const endsInsideWord = /[\p{L}\p{N}_]/u.test(name.at(-1) ?? '') && /[\p{L}\p{N}_]/u.test(after);
    if (!startsInsideWord && !endsInsideWord) {
      return index;
    }
    index = description.indexOf(name, index + 1);
  }
  return -1;
}

/// Split an event description around exact roster names so faction color can be
/// applied to players without tinting the event itself. Duplicate names with
/// conflicting factions deliberately remain neutral.
export function playerNameSegments(
  description: string,
  players: Pick<PlayerView, 'name' | 'faction'>[],
): PlayerNameSegment[] {
  const tones = new Map<string, 'allied' | 'soviet' | null>([
    ['USA', 'allied'],
    ['NATO', 'allied'],
    ['USSR', 'soviet'],
  ]);
  for (const player of players) {
    const name = player.name.trim();
    if (!name) {
      continue;
    }
    if (name === 'USA' || name === 'NATO' || name === 'USSR') {
      continue;
    }
    const tone = factionTone(player.faction);
    if (tones.has(name) && tones.get(name) !== tone) {
      tones.set(name, null);
    } else if (!tones.has(name)) {
      tones.set(name, tone);
    }
  }

  const names = [...tones.keys()].sort((left, right) => right.length - left.length);
  if (!names.length) {
    return [{ text: description, tone: null }];
  }

  const segments: PlayerNameSegment[] = [];
  let cursor = 0;
  while (cursor < description.length) {
    let nextIndex = -1;
    let nextName = '';
    for (const name of names) {
      const index = playerNameIndex(description, name, cursor);
      if (index >= 0 && (nextIndex < 0 || index < nextIndex)) {
        nextIndex = index;
        nextName = name;
      }
    }
    if (nextIndex < 0) {
      break;
    }
    if (nextIndex > cursor) {
      segments.push({ text: description.slice(cursor, nextIndex), tone: null });
    }
    segments.push({ text: nextName, tone: tones.get(nextName) ?? null });
    cursor = nextIndex + nextName.length;
  }
  if (cursor < description.length) {
    segments.push({ text: description.slice(cursor), tone: null });
  }
  return segments.length ? segments : [{ text: description, tone: null }];
}

export function displayDate(value: string): string {
  if (!value) {
    return 'Unknown date';
  }
  return value.replace('T', ' ').replace(/\.\d+Z?$/, '');
}

export function replayTitle(fileName: string): string {
  return fileName.replace(/\.wicdemo$/i, '');
}

export function replayFileName(title: string): string {
  return `${replayTitle(title)}.wicdemo`;
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

// Domination is a continuous lead bar; Tug of War is a discrete front line. Both
// describe a split between the two sides. Assault works differently — its value
// tracks attacker progress — so it carries no control split.
export function usesDominationControl(gameMode: string): boolean {
  return gameMode === 'Domination' || gameMode === 'Tug of War';
}

export function controlMeterTitle(gameMode: string): string {
  return gameMode === 'Tug of War' ? 'Final front line' : 'Final domination bar';
}

const MATCH_ENDING_LABELS: Record<MatchEnding, string> = {
  totalDomination: 'Total domination',
  timeout: 'Decided on the timer',
  forfeit: 'Forfeit',
  unknown: 'Unknown ending',
};

export function matchEndingLabel(ending: MatchEnding): string {
  return MATCH_ENDING_LABELS[ending] ?? MATCH_ENDING_LABELS.unknown;
}

/// Percentage text carrying every digit the bar actually resolves.
///
/// The bar is quantised to 1/3000, i.e. steps of 0.0333%, so rounding to whole
/// percent collapses a real result like 50.07 / 49.93 into a false 50 / 50 tie.
/// Two decimals is the game's own precision; trailing zeros are dropped so a
/// clean value still reads as "58" rather than "58.00".
export function formatShare(percent: number): string {
  const text = percent.toFixed(2);
  return text.includes('.') ? text.replace(/0+$/, '').replace(/\.$/, '') : text;
}

export interface DominationSegment {
  faction: string;
  /// Bar width in percent. The two always total exactly 100.
  width: number;
  label: string;
}

export function dominationSegments(shares: DominationShare[]): DominationSegment[] {
  if (shares.length !== 2) {
    return [];
  }
  const ordered = shares[0].faction === 'USSR' ? [shares[1], shares[0]] : [shares[0], shares[1]];
  const first = ordered[0].pct * 100;
  const second = 100 - first;
  return [
    { faction: ordered[0].faction, width: first, label: formatShare(first) },
    { faction: ordered[1].faction, width: second, label: formatShare(second) },
  ];
}

export function partialRecordingNote(timing: MatchTiming): string | null {
  if (timing.capturedMatchStart) {
    return null;
  }
  if (timing.joinedAtRemainingSeconds === null) {
    return 'Recorder joined mid-match';
  }
  const minutes = timing.joinedAtRemainingSeconds / 60;
  return `Recorder joined mid-match, with ${minutes.toFixed(1)} min left on the clock`;
}

const PLAYING_FACTIONS = new Set(['USA', 'NATO', 'USSR']);

export function factionTone(faction: string | null): 'allied' | 'soviet' | null {
  if (faction === 'USSR') {
    return 'soviet';
  }
  if (faction === 'USA' || faction === 'NATO') {
    return 'allied';
  }
  return null;
}

/// The faction tone class for a chip, or null when the faction is unproven so
/// the chip keeps its neutral styling instead of a dangling `faction-tone-null`.
export function factionToneClass(faction: string | null): string | null {
  const tone = factionTone(faction);
  return tone && `faction-tone-${tone}`;
}

/// WiC team IDs are fixed faction IDs, so a team's faction never depends on the
/// roster; spectator recordings can omit every player of a side.
const TEAM_FACTIONS: Record<number, string> = { 0: 'Spectator', 1: 'USA', 2: 'NATO', 3: 'USSR' };

export function teamFaction(team: number | null): string | null {
  return (team === null ? undefined : TEAM_FACTIONS[team]) ?? null;
}

export function playerGroupName(faction: string | null, team: number | null): string {
  if (faction && (faction === 'Spectator' || factionTone(faction))) {
    return faction;
  }
  return teamFaction(team) ?? 'Spectator';
}

export function resultGroupPriority(name: string): number {
  if (name === 'USA' || name === 'NATO') {
    return 0;
  }
  if (name === 'USSR') {
    return 1;
  }
  if (name === 'Spectator') {
    return 2;
  }
  return 3;
}

export function resultGroupLabel(name: string, winner: string | null): string {
  if (name === winner) {
    return 'WINNER';
  }
  if (winner && PLAYING_FACTIONS.has(name)) {
    return 'LOSER';
  }
  if (name === 'Spectator') {
    return 'Spectators';
  }
  if (!PLAYING_FACTIONS.has(name)) {
    return 'Secondary';
  }
  return 'Formation';
}
