export type MatchEnding = 'totalDomination' | 'timeout' | 'forfeit' | 'unknown';

export interface MatchTiming {
  capturedMatchStart: boolean;
  /** Real length of the replay file, from the Event envelope clock. */
  recordingSeconds: number;
  /** Gameplay observed between the first and last countdown sample. */
  observedGameplaySeconds: number | null;
  matchElapsedSeconds: number | null;
  roundLengthSeconds: number | null;
  roundLengthExact: boolean;
  joinedAtRemainingSeconds: number | null;
  finalRemainingSeconds: number | null;
}

export interface DominationShare {
  faction: string;
  pct: number;
}

export interface FileFingerprint {
  size: number;
  modifiedNs: number;
}

export interface ReplaySummary {
  path: string;
  fileName: string;
  /** Name stored inside ReplayName metadata; absent in some replay variants. */
  replayName: string | null;
  serverName: string;
  fingerprint: FileFingerprint;
  cacheKey: string;
  mapName: string;
  mapDisplayName: string;
  gameMode: string;
  serverModes: string;
  format: string;
  dateTime: string;
  /** Match length: how long the game ran. */
  durationSeconds: number | null;
  /** Replay length: how long the recording runs. */
  recordingSeconds: number | null;
  winner: string | null;
  playerCount: number;
  playerNames: string;
  /** Selected result occupants; unknown factions remain null. */
  searchPlayers: { name: string; faction: string | null }[];
  /** Unique playable factions represented by replay participants. */
  factions: string;
  recorder: string | null;
  recorderFaction: string | null;
  incomplete: boolean;
  parseError: string | null;
  importedAt: number;
}

export interface ReplayOperationResult {
  path: string;
  summary: ReplaySummary | null;
}

export interface ReplayBatchExportResult {
  folder: string;
  exports: ReplayOperationResult[];
}

export interface InitialState {
  summaries: ReplaySummary[];
  locations: string[];
  importing: boolean;
  cachedMapArtCount: number;
}

export interface RemovedLocation {
  locations: string[];
  removedReplayPaths: string[];
}

export interface PlayerView {
  leftAtSeconds?: number | null;
  id: number;
  name: string;
  team: number | null;
  faction: string | null;
  role: string | null;
  score: number | null;
  scoreBeforeLeave?: {
    score: number;
    observedAtSeconds: number;
    leftAtSeconds: number;
  } | null;
  scoreInfantry: number | null;
  scoreSupport: number | null;
  scoreArmor: number | null;
  scoreAir: number | null;
  scoreCapturing: number | null;
  scoreFortification: number | null;
  scoreTransportation: number | null;
  scoreRepair: number | null;
  scoreBridgeLaying: number | null;
  scoreUnitDamage: number | null;
  scoreTacticalAid: number | null;
  scoreTotal: number | null;
}

export interface TimelineRow {
  timeSeconds: number;
  kind: string;
  description: string;
  players: Array<{ name: string; faction: string | null }>;
  commandPointId: number | null;
  commandPointTeam: number | null;
}

export interface TimelineValueSample {
  timeSeconds: number;
  value: number;
}

export interface ScoreParticipant {
  playerId: number;
  sessionIndex: number;
  playerName: string | null;
  startSeconds: number;
  endSeconds: number | null;
}

export interface ScoreTeamChange {
  timeSeconds: number;
  playerId: number;
  team: number;
}

export interface DetailView {
  scoreParticipants: ScoreParticipant[];
  overview: {
    mapName: string;
    mapDisplayName: string;
    serverName: string;
    dateTime: string;
    gameMode: string;
    serverModes: string[];
    format: string | null;
    durationSeconds: number | null;
    timing: MatchTiming;
    matchEnding: MatchEnding;
    winnerDominationPct: number | null;
    loserDominationPct: number | null;
    dominationShares: DominationShare[] | null;
    dominationAnchor: 'povTeam' | 'winnerInferred' | null;
    winner: string | null;
    recorder: string | null;
    incomplete: boolean;
    players: PlayerView[];
  };
  timelineSchema: number;
  durationSeconds: number;
  phases: Array<{
    index: number;
    startSeconds: number;
    endSeconds: number;
    initialClockSeconds: number;
    finalClockSeconds: number;
  }>;
  dominationSamples: TimelineValueSample[];
  dominationAnchorFaction: string | null;
  coverageChat: string;
  coverageTacticalAid: string;
  recorderView: string;
  timelineRows: TimelineRow[];
  chatRows: Array<{
    timeLabel: string;
    stage: string;
    player: string;
    channel: string;
    message: string;
  }>;
  tacticalAidRows: Array<{
    timeSeconds: number;
    support: string;
    player: string;
    faction: string;
    playerAttribution: string;
    honorsCost: number | null;
    position: [number, number, number];
  }>;
  tacticalAidSummary: {
    player: string;
    totalPlacements: number;
    supports: Array<{
      support: string;
      faction: string;
      placementCount: number;
      observedCosts: number[];
    }>;
  };
}

export type ImportEvent =
  | {
      type: 'importPrepared';
      payload: { discovered: number; pending: number; skipped: number };
    }
  | { type: 'summariesStored'; payload: ReplaySummary[] }
  | { type: 'importProblem'; payload: string }
  | {
      type: 'importFinished';
      payload: {
        imported: number;
        failed: number;
        skipped: number;
        removedPaths?: string[];
        removed_paths?: string[];
        cancelled: boolean;
      };
    };

export interface ImportProgress {
  discovered: number;
  pending: number;
  skipped: number;
  processed: number;
  imported: number;
  failed: number;
}

export type DetailTab = 'overview' | 'replay' | 'chat' | 'tacticalAid';

export interface ScoreSample {
  timeSeconds: number;
  playerId: number;
  score: number;
}

export interface PlaybackView {
  areaEffects: Array<{
    timeSeconds: number;
    position: [number, number, number];
    radius: number;
    durationSeconds: number;
    kind: 'explosion' | 'napalm' | 'chemical';
  }>;
  nuclearEffects: Array<{ timeSeconds: number; position: [number, number, number] }>;
  scoreTeams: ScoreTeamChange[];
  scoreSamples: ScoreSample[];
  durationSeconds: number;
  frameCount: number;
  malformedFrames: number;
  semantics: string;
  units: Array<{
    unitId: number;
    generation: number;
    createdSeconds: number;
    spawnPosition: [number, number, number];
    team: number | null;
    unitTypeId: number | null;
    isInfantryMember: boolean;
    initialHealth: number | null;
    frames: Array<{ timeSeconds: number; position: [number, number, number] }>;
    health: Array<{ timeSeconds: number; value: number }>;
    teams: Array<{ timeSeconds: number; team: number }>;
    terminal: { timeSeconds: number; kind: string } | null;
  }>;
  objectives: PlaybackObjective[];
  objectiveChanges: Array<{
    timeSeconds: number;
    kind: 'commandPoint' | 'perimeterPoint';
    id: number | null;
    team: number | null;
  }>;
}

export interface PlaybackObjective {
  kind: 'commandPoint' | 'perimeterPoint';
  id: number | null;
  parentId: number | null;
  position: [number, number, number] | null;
  team: number | null;
}

export interface MapBounds {
  minX: number;
  minZ: number;
  maxX: number;
  maxZ: number;
}

export interface MapArtAsset {
  imageUrl: string;
  bounds: MapBounds | null;
  commandPointNames: Record<string, string>;
}

/// Where map art is read from, and how much was decoded. Both folders are
/// optional and independent: the game folder moves with the installation, while
/// custom maps always live under the user's Documents folder. Having neither is
/// a normal state, not an error.
export interface MapArtState {
  installPath: string | null;
  customMapsPath: string | null;
  mapCount: number;
  problem: string | null;
  /** The backend replaced or cleared the cached image set during validation. */
  cacheChanged: boolean;
}

export type ArtSourceKind = 'install' | 'customMaps';
