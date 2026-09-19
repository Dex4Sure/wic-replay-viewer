import { describe, expect, it } from 'vitest';

import {
  controlMeterTitle,
  factionTone,
  factionToneClass,
  displayDate,
  dominationSegments,
  formatDuration,
  formatShare,
  formatTimestamp,
  matchEndingLabel,
  partialRecordingNote,
  playerGroupName,
  playerNameSegments,
  replayFileName,
  replayTitle,
  resultGroupLabel,
  resultGroupPriority,
  teamFaction,
  timelineDescription,
  usesDominationControl,
} from './format';
import type { MatchTiming, TimelineRow } from './types';

function timing(overrides: Partial<MatchTiming> = {}): MatchTiming {
  return {
    capturedMatchStart: true,
    recordingSeconds: 1320,
    observedGameplaySeconds: 1200,
    matchElapsedSeconds: 1200,
    roundLengthSeconds: 1200,
    roundLengthExact: true,
    joinedAtRemainingSeconds: null,
    finalRemainingSeconds: 0.95,
    ...overrides,
  };
}

describe('viewer formatting', () => {
  const commandPointRow = (overrides: Partial<TimelineRow> = {}): TimelineRow => ({
    timeSeconds: 10,
    kind: 'Command point',
    description: 'USA captured a command point',
    players: [],
    commandPointId: 0x2eae05b8,
    commandPointTeam: 1,
    ...overrides,
  });

  it('formats replay duration as a stable clock', () => {
    expect(formatDuration(65.4)).toBe('1:05');
    expect(formatDuration(null)).toBe('Unknown');
  });

  it('uses the source filename without its replay extension as the replay title', () => {
    expect(replayTitle('demo01.wicdemo')).toBe('demo01');
    expect(replayTitle('demo02.WICDEMO')).toBe('demo02');
    expect(replayTitle('demo03')).toBe('demo03');
    expect(displayDate('')).toBe('Unknown date');
  });

  it('keeps the replay extension outside the editable file name', () => {
    expect(replayFileName('renamed replay')).toBe('renamed replay.wicdemo');
    expect(replayFileName('pasted replay.wicdemo')).toBe('pasted replay.wicdemo');
    expect(replayFileName('pasted replay.WICDEMO')).toBe('pasted replay.wicdemo');
  });

  it('keeps USA/NATO left and USSR right before non-playing groups', () => {
    const groups = ['Unassigned', 'Spectator', 'USSR', 'NATO'];
    groups.sort((left, right) => resultGroupPriority(left) - resultGroupPriority(right));

    expect(groups).toEqual(['NATO', 'USSR', 'Spectator', 'Unassigned']);
    expect(
      ['USA', 'USSR'].sort((left, right) => resultGroupPriority(left) - resultGroupPriority(right)),
    ).toEqual(['USA', 'USSR']);
    expect(resultGroupLabel('NATO', 'NATO')).toBe('WINNER');
    expect(resultGroupLabel('USSR', 'NATO')).toBe('LOSER');
    expect(resultGroupLabel('Spectator', 'NATO')).toBe('Spectators');
    expect(resultGroupLabel('Unassigned', 'NATO')).toBe('Secondary');
  });

  it('groups unresolved teams with spectators for display', () => {
    expect(playerGroupName(null, null)).toBe('Spectator');
    expect(playerGroupName('Unassigned', null)).toBe('Spectator');
    expect(playerGroupName('NATO', 2)).toBe('NATO');
    expect(playerGroupName(null, 4)).toBe('Spectator');
    expect(playerGroupName('Team unknown', null)).toBe('Spectator');
    expect(playerGroupName(null, 0)).toBe('Spectator');
    expect(playerGroupName(null, 1)).toBe('USA');
  });

  it('maps fixed team IDs to factions without roster evidence', () => {
    expect([0, 1, 2, 3, 4, null].map(teamFaction)).toEqual([
      'Spectator',
      'USA',
      'NATO',
      'USSR',
      null,
      null,
    ]);
  });

  it('shows the control bar only where aFactor is a two-sided split', () => {
    expect(usesDominationControl('Domination')).toBe(true);
    expect(usesDominationControl('Tug of War')).toBe(true);
    // Assault tracks attacker progress, not a split between the two sides.
    expect(usesDominationControl('Assault')).toBe(false);
    expect(usesDominationControl('Unknown')).toBe(false);
  });

  it('names the meter for the mode it describes', () => {
    expect(controlMeterTitle('Domination')).toBe('Final domination bar');
    expect(controlMeterTitle('Tug of War')).toBe('Final front line');
  });

  it('keeps the bar segments totalling exactly 100 percent', () => {
    const segments = dominationSegments([
      { faction: 'USA', pct: 0.505 },
      { faction: 'USSR', pct: 0.495 },
    ]);
    expect(segments[0].width + segments[1].width).toBe(100);
    expect(segments.map((s) => s.label)).toEqual(['50.5', '49.5']);
  });

  it('does not collapse a razor-thin finish into a false 50/50 tie', () => {
    // demo335 finished USA 50.07 / USSR 49.93; whole-percent rounding hid it.
    const segments = dominationSegments([
      { faction: 'USA', pct: 0.5006666779518127 },
      { faction: 'USSR', pct: 0.49933332204818726 },
    ]);
    expect(segments.map((s) => s.label)).toEqual(['50.07', '49.93']);
    expect(matchEndingLabel('timeout')).toBe('Decided on the timer');
  });

  it('drops trailing zeros so clean results stay readable', () => {
    expect(formatShare(100)).toBe('100');
    expect(formatShare(0)).toBe('0');
    expect(formatShare(58)).toBe('58');
    expect(formatShare(50.06666779518127)).toBe('50.07');
    expect(
      dominationSegments([
        { faction: 'USA', pct: 1 },
        { faction: 'USSR', pct: 0 },
      ]).map((s) => s.label),
    ).toEqual(['100', '0']);
  });

  it('notes a mid-match join and stays silent for a full recording', () => {
    expect(partialRecordingNote(timing())).toBeNull();
    expect(
      partialRecordingNote(timing({ capturedMatchStart: false, joinedAtRemainingSeconds: 926.6 })),
    ).toBe('Recorder joined mid-match, with 15.4 min left on the clock');
  });

  it('maps playing factions to their battlefield color families', () => {
    expect(factionTone('USSR')).toBe('soviet');
    expect(factionTone('USA')).toBe('allied');
    expect(factionTone('NATO')).toBe('allied');
    expect(factionTone('Spectator')).toBeNull();
    expect(factionTone(null)).toBeNull();
    expect(factionToneClass('USSR')).toBe('faction-tone-soviet');
    expect(factionToneClass('NATO')).toBe('faction-tone-allied');
    expect(factionToneClass('Unknown faction')).toBeNull();
    expect(factionToneClass(null)).toBeNull();
  });

  it('colors each named event player by their own faction', () => {
    const players = [
      { name: 'Alpha', faction: 'USA' },
      { name: 'Bravo', faction: 'USSR' },
    ];
    expect(playerNameSegments("Bravo destroyed one of Alpha's units", players)).toEqual([
      { text: 'Bravo', tone: 'soviet' },
      { text: ' destroyed one of ', tone: null },
      { text: 'Alpha', tone: 'allied' },
      { text: "'s units", tone: null },
    ]);
  });

  it('colors team names and keeps conflicting player names neutral', () => {
    expect(playerNameSegments('USSR captured a command point', [])).toEqual([
      { text: 'USSR', tone: 'soviet' },
      { text: ' captured a command point', tone: null },
    ]);
    expect(playerNameSegments('USA attacked NATO', [])).toEqual([
      { text: 'USA', tone: 'allied' },
      { text: ' attacked ', tone: null },
      { text: 'NATO', tone: 'allied' },
    ]);
    expect(
      playerNameSegments('Alpha deployed artillery', [
        { name: 'Alpha', faction: 'USA' },
        { name: 'Alpha', faction: 'USSR' },
      ]),
    ).toEqual([
      { text: 'Alpha', tone: null },
      { text: ' deployed artillery', tone: null },
    ]);
    expect(
      playerNameSegments('Alpha deployed artillery', [{ name: 'Al', faction: 'USA' }]),
    ).toEqual([{ text: 'Alpha deployed artillery', tone: null }]);
  });

  it('reads event timestamps as minutes and seconds', () => {
    expect(formatTimestamp(0)).toBe('0:00.0');
    expect(formatTimestamp(7.25)).toBe('0:07.3');
    expect(formatTimestamp(65)).toBe('1:05.0');
    expect(formatTimestamp(812.4)).toBe('13:32.4');
    expect(formatTimestamp(3600)).toBe('60:00.0');
    // A value just short of a minute must not round into a "0:60.0".
    expect(formatTimestamp(59.97)).toBe('1:00.0');
    expect(formatTimestamp(null)).toBe('—');
  });

  it('uses installed command-point names and preserves the generic fallback', () => {
    const names = { [String(0x2eae05b8)]: 'Space Needle' };
    expect(timelineDescription(commandPointRow(), names)).toBe('USA captured Space Needle');
    expect(
      timelineDescription(
        commandPointRow({
          description: 'Command point became neutral',
          commandPointTeam: 0,
        }),
        names,
      ),
    ).toBe('Space Needle became neutral');
    expect(timelineDescription(commandPointRow(), {})).toBe('USA captured a command point');
    expect(
      timelineDescription(
        commandPointRow({
          description: 'Invalid faction data captured a command point',
          commandPointTeam: 99,
        }),
        names,
      ),
    ).toBe('Invalid faction data captured a command point');
  });
});
