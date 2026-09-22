import type { ScoreParticipant, ScoreSample, ScoreTeamChange } from './types';

export interface ScoreboardPlayer {
  key: string;
  playerId: number;
  name: string;
  score: number | null;
}

export interface ScoreboardTeam {
  team: number;
  label: string;
  players: ScoreboardPlayer[];
  total: number | null;
}

function indexByPlayer<T extends { playerId: number; timeSeconds: number }>(
  rows: T[],
): Map<number, T[]> {
  const index = new Map<number, T[]>();
  for (const row of rows) {
    const list = index.get(row.playerId) ?? [];
    list.push(row);
    index.set(row.playerId, list);
  }
  for (const list of index.values()) {
    list.sort((a, b) => a.timeSeconds - b.timeSeconds);
  }
  return index;
}

function latest<T extends { timeSeconds: number }>(rows: T[], time: number): T | undefined {
  let low = 0;
  let high = rows.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (rows[middle]!.timeSeconds <= time) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return rows[low - 1];
}

/** Index once per replay; playback and backward seeks only query observations. */
export function createScoreboard(
  participants: ScoreParticipant[],
  teams: ScoreTeamChange[],
  samples: ScoreSample[],
): (time: number) => ScoreboardTeam[] {
  const scoreIndex = indexByPlayer(samples);
  const teamIndex = indexByPlayer(teams);
  const labels: Record<number, string> = { 1: 'USA', 2: 'NATO', 3: 'USSR' };
  return (time) => {
    const groups = new Map<number, ScoreboardTeam>();
    for (const session of participants) {
      if (
        session.startSeconds > time ||
        (session.endSeconds !== null && time >= session.endSeconds)
      ) {
        continue;
      }
      const assignment = latest(teamIndex.get(session.playerId) ?? [], time);
      // Final roster teams are not evidence for earlier moments or other occupants.
      const team =
        assignment && assignment.timeSeconds >= session.startSeconds ? assignment.team : null;
      if (team === null || !labels[team]) {
        continue;
      }
      const sample = latest(scoreIndex.get(session.playerId) ?? [], time);
      // A reused slot can have an old and a new occupant at the same timestamp.
      // Without an envelope offset in this projection, wait for a later score.
      const score =
        sample &&
        (session.sessionIndex > 0
          ? sample.timeSeconds > session.startSeconds
          : sample.timeSeconds >= session.startSeconds)
          ? sample.score
          : null;
      // Unoccupied slots receive zero score broadcasts too.
      if (!session.playerName && (score === null || score === 0)) {
        continue;
      }
      const group = groups.get(team) ?? {
        team,
        label: labels[team]!,
        players: [],
        total: null,
      };
      group.players.push({
        key: `${session.playerId}:${session.sessionIndex}`,
        playerId: session.playerId,
        name: session.playerName ?? `Player ${session.playerId}`,
        score,
      });
      groups.set(team, group);
    }
    for (const group of groups.values()) {
      group.players.sort((a, b) => {
        if (a.score === null) {
          return b.score === null ? a.playerId - b.playerId : 1;
        }
        if (b.score === null) {
          return -1;
        }
        return b.score - a.score || a.playerId - b.playerId;
      });
      group.total = group.players.every((player) => player.score !== null)
        ? group.players.reduce((sum, player) => sum + player.score!, 0)
        : null;
    }
    return [...groups.values()].sort((a, b) => a.team - b.team);
  };
}
