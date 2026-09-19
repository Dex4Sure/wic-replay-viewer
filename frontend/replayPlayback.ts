import type { DominationShare, TimelineValueSample } from './types';

export const REPLAY_FRAME_INTERVAL_MS = 1000 / 30;

const ALLIED_FACTIONS = new Set(['USA', 'NATO']);

/**
 * Recover which faction the serialized sample curve represents when a
 * spectator recording has no POV team. The final result projection already
 * assigns the final raw value to concrete factions from the recorded winner;
 * matching that value identifies the curve's faction without changing it.
 */
export function dominationPlaybackAnchor(
  samples: ReadonlyArray<TimelineValueSample>,
  povAnchorFaction: string | null,
  finalShares: ReadonlyArray<DominationShare> | null,
  finalAnchor: 'povTeam' | 'winnerInferred' | null,
): string | null {
  if (povAnchorFaction) {
    return povAnchorFaction;
  }
  if (finalAnchor !== 'winnerInferred' || finalShares?.length !== 2 || !samples.length) {
    return null;
  }

  const finalValue = samples.at(-1)!.value;
  if (!Number.isFinite(finalValue)) {
    return null;
  }
  const matches = finalShares.filter((share) => Math.abs(share.pct - finalValue) < 0.000_01);
  // At an exact tie either faction represents the same curve value, so the
  // stable result order is sufficient. Any other ambiguity remains unknown.
  if (matches.length === 2 && Math.abs(finalValue - 0.5) < 0.000_01) {
    return matches[0]!.faction;
  }
  return matches.length === 1 ? matches[0]!.faction : null;
}

/**
 * Resolve the latest recorded domination sample at the scrubber time into two
 * concrete faction shares. Samples are held until the next recorded value;
 * interpolation would claim precision the replay does not contain.
 */
export function dominationSharesAt(
  samples: ReadonlyArray<TimelineValueSample>,
  playbackTime: number,
  anchorFaction: string | null,
  availableFactions: ReadonlyArray<string>,
): DominationShare[] | null {
  if (!anchorFaction || !Number.isFinite(playbackTime)) {
    return null;
  }

  const factions = new Set(availableFactions);
  let opposingFaction: string | null = null;
  if (ALLIED_FACTIONS.has(anchorFaction)) {
    if (factions.has('USSR')) {
      opposingFaction = 'USSR';
    }
  } else if (anchorFaction === 'USSR') {
    const allied = [...factions].filter((faction) => ALLIED_FACTIONS.has(faction));
    if (allied.length === 1) {
      opposingFaction = allied[0]!;
    }
  }
  if (!opposingFaction) {
    return null;
  }

  let low = 0;
  let high = samples.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (samples[middle]!.timeSeconds <= playbackTime) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  const sample = samples[low - 1];
  if (!sample || !Number.isFinite(sample.value) || sample.value < 0 || sample.value > 1) {
    return null;
  }
  return [
    { faction: anchorFaction, pct: sample.value },
    { faction: opposingFaction, pct: 1 - sample.value },
  ];
}

export function revealedReplayEventCount(
  rows: ReadonlyArray<{ timeSeconds: number }>,
  playbackTime: number,
): number {
  let low = 0;
  let high = rows.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (rows[middle]!.timeSeconds <= playbackTime) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low;
}

interface ReplayPlaybackOptions {
  duration: () => number;
  speed: () => number;
  time: () => number;
  setPlaying: (playing: boolean) => void;
  setTime: (time: number) => void;
  now?: () => number;
}

export interface ReplayPlaybackTimer {
  start: () => void;
  stop: () => void;
}

export function showReplayMapUnit(unit: { isInfantryMember: boolean }): boolean {
  return !unit.isInfantryMember;
}

export function createReplayPlaybackTimer(options: ReplayPlaybackOptions): ReplayPlaybackTimer {
  const now = options.now ?? (() => performance.now());
  let timer: ReturnType<typeof setTimeout> | null = null;
  let previous = 0;
  let playing = false;

  function stop(): void {
    playing = false;
    options.setPlaying(false);
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = null;
  }

  function scheduleTick(): void {
    if (!playing || timer !== null) {
      return;
    }
    timer = setTimeout(() => {
      timer = null;
      tick(now());
    }, REPLAY_FRAME_INTERVAL_MS);
  }

  function tick(timestamp: number): void {
    if (!playing) {
      return;
    }
    const time = Math.min(
      options.duration(),
      options.time() + ((timestamp - previous) / 1000) * options.speed(),
    );
    options.setTime(time);
    previous = timestamp;
    if (time >= options.duration()) {
      stop();
      return;
    }
    scheduleTick();
  }

  function start(): void {
    if (playing) {
      return;
    }
    playing = true;
    options.setPlaying(true);
    previous = now();
    scheduleTick();
  }

  return { start, stop };
}
