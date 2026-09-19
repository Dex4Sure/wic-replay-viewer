import type { DetailView, MapArtAsset, PlaybackView, ReplaySummary } from '../types';

export function replaySummary(overrides: Partial<ReplaySummary> = {}): ReplaySummary {
  return {
    path: '/replays/example.wicdemo',
    fileName: 'example.wicdemo',
    replayName: 'Example',
    serverName: 'WiCGate Ranked Server',
    fingerprint: { size: 100, modifiedNs: 1 },
    cacheKey: 'timeline-v18/detail-v26',
    mapName: 'maps/ustown4/ustown4.ice',
    mapDisplayName: 'Seaside',
    gameMode: 'Domination',
    serverModes: 'Ranked',
    format: '4v4',
    dateTime: '2010-01-02T03:04:00Z',
    durationSeconds: 1200,
    recordingSeconds: 1250,
    winner: 'USA',
    playerCount: 8,
    searchPlayers: [],
    playerNames: 'Alice Bob',
    factions: 'USA, USSR',
    recorder: 'Alice',
    recorderFaction: 'USA',
    incomplete: false,
    parseError: null,
    importedAt: 1,
    ...overrides,
  };
}

export function replayDetail(overrides: Partial<DetailView> = {}): DetailView {
  return {
    scoreParticipants: [],
    overview: {
      mapName: 'maps/ustown4/ustown4.ice',
      mapDisplayName: 'Seaside',
      serverName: 'Server',
      dateTime: '2010-01-02T03:04:00Z',
      gameMode: 'Domination',
      serverModes: [],
      format: '4v4',
      durationSeconds: 1200,
      timing: {
        capturedMatchStart: true,
        recordingSeconds: 1250,
        observedGameplaySeconds: 1200,
        matchElapsedSeconds: 1200,
        roundLengthSeconds: 1200,
        roundLengthExact: true,
        joinedAtRemainingSeconds: null,
        finalRemainingSeconds: 0,
      },
      matchEnding: 'timeout',
      winnerDominationPct: 0.6,
      loserDominationPct: 0.4,
      dominationShares: [
        { faction: 'USA', pct: 0.6 },
        { faction: 'USSR', pct: 0.4 },
      ],
      dominationAnchor: 'povTeam',
      winner: 'USA',
      recorder: 'Alice',
      incomplete: false,
      players: [],
    },
    timelineSchema: 18,
    durationSeconds: 1250,
    phases: [],
    dominationSamples: [],
    dominationAnchorFaction: 'USA',
    coverageChat: 'complete',
    coverageTacticalAid: 'complete',
    recorderView: 'USA',
    timelineRows: [],
    chatRows: [],
    tacticalAidRows: [],
    tacticalAidSummary: { player: 'Alice', totalPlacements: 0, supports: [] },
    ...overrides,
  };
}

export function playbackView(overrides: Partial<PlaybackView> = {}): PlaybackView {
  return {
    areaEffects: [],
    nuclearEffects: [],
    durationSeconds: 10,
    scoreSamples: [],
    scoreTeams: [],
    frameCount: 1,
    malformedFrames: 0,
    semantics: 'authoritative checkpoints; hold-last between frames',
    units: [],
    objectives: [],
    objectiveChanges: [],
    ...overrides,
  };
}

export function mapArtAsset(overrides: Partial<MapArtAsset> = {}): MapArtAsset {
  return {
    imageUrl: 'data:image/png;base64,AA==',
    bounds: null,
    commandPointNames: {},
    ...overrides,
  };
}
