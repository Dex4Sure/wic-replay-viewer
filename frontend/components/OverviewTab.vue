<script setup lang="ts">
import { computed } from 'vue';

import {
  controlMeterTitle,
  displayDate,
  dominationSegments,
  formatDuration,
  matchEndingLabel,
  partialRecordingNote,
  playerGroupName,
  resultGroupLabel,
  resultGroupPriority,
  usesDominationControl,
} from '../format';
import { gameScoreOrder } from '../gameScoreOrder';
import type { DetailView, PlayerView } from '../types';
import ControlMeter from './ControlMeter.vue';

const props = defineProps<{ detail: DetailView }>();

interface TeamGroup {
  name: string;
  players: PlayerView[];
  score: number;
  winner: boolean;
  label: string;
}

type ScoreField =
  | 'scoreInfantry'
  | 'scoreSupport'
  | 'scoreArmor'
  | 'scoreAir'
  | 'scoreCapturing'
  | 'scoreFortification'
  | 'scoreRepair'
  | 'scoreUnitDamage'
  | 'scoreTacticalAid'
  | 'scoreTotal';

interface MatchLeader {
  label: string;
  score: number;
  player: PlayerView;
}

interface OverallRank {
  rank: number;
  score: number;
  player: PlayerView;
}

const roleIcons: Record<string, string> = {
  infantry: '/roles/infantry.png',
  armor: '/roles/armor.png',
  air: '/roles/air.png',
  support: '/roles/support.png',
};

function roleIcon(role: string | null): string | null {
  return role ? (roleIcons[role.toLowerCase()] ?? null) : null;
}

const teams = computed<TeamGroup[]>(() => {
  const grouped = new Map<string, PlayerView[]>();
  for (const player of props.detail.overview.players) {
    const key = playerGroupName(player.faction, player.team);
    const players = grouped.get(key) ?? [];
    players.push(player);
    grouped.set(key, players);
  }
  // A faction remains part of the match even when its final roster is empty.
  // Prefer current result evidence; historical events can identify USA vs NATO.
  if (!grouped.has('USA') && !grouped.has('NATO')) {
    const resultAllies = new Set(
      [
        props.detail.overview.winner,
        ...(props.detail.overview.dominationShares ?? []).map((s) => s.faction),
      ].filter((name): name is string => name === 'USA' || name === 'NATO'),
    );
    const recordedAllies = new Set(
      props.detail.timelineRows
        .flatMap((row) => [
          ...row.players.map((player) => player.faction),
          row.commandPointTeam === 1 ? 'USA' : row.commandPointTeam === 2 ? 'NATO' : null,
        ])
        .filter((name): name is string => name === 'USA' || name === 'NATO'),
    );
    const evidence = resultAllies.size ? resultAllies : recordedAllies;
    if (evidence.size === 1) {
      grouped.set([...evidence][0]!, []);
    }
  }
  if (!grouped.has('USSR')) {
    grouped.set('USSR', []);
  }
  return [...grouped.entries()]
    .map(([name, players]) => ({
      name,
      players: [...players].sort((a, b) => (b.score ?? 0) - (a.score ?? 0)),
      score: players.reduce((sum, player) => sum + (player.score ?? 0), 0),
      winner: props.detail.overview.winner === name,
      label: resultGroupLabel(name, props.detail.overview.winner),
    }))
    .sort((left, right) => {
      const priority = resultGroupPriority(left.name) - resultGroupPriority(right.name);
      return priority || right.score - left.score || left.name.localeCompare(right.name);
    });
});

const competitiveTeams = computed(() => teams.value.filter((team) => team.name !== 'Spectator'));

// Driven by the parser's faction-anchored shares rather than by winner/loser, so
// the percentage is an independent fact rather than one derived from the outcome.
const dominationBar = computed(() => {
  const { incomplete, gameMode, dominationShares } = props.detail.overview;
  if (incomplete || !usesDominationControl(gameMode) || !dominationShares) {
    return [];
  }
  return dominationSegments(dominationShares);
});

const endingLabel = computed(() => matchEndingLabel(props.detail.overview.matchEnding));
const meterTitle = computed(() => controlMeterTitle(props.detail.overview.gameMode));
const partialNote = computed(() => partialRecordingNote(props.detail.overview.timing));
const spectators = computed(
  () => teams.value.find((team) => team.name === 'Spectator')?.players ?? [],
);

function isCompetitivePlayer(player: PlayerView): boolean {
  const group = playerGroupName(player.faction, player.team);
  return group !== 'Spectator';
}

function rankedPlayers(field: ScoreField): PlayerView[] {
  return gameScoreOrder(props.detail.overview.players, (player) => player[field]).filter(
    (player) => isCompetitivePlayer(player) && player[field] !== null,
  );
}

const topPlayers = computed<OverallRank[]>(() => {
  return rankedPlayers('scoreTotal')
    .slice(0, 3)
    .map((player, index) => ({
      rank: index + 1,
      score: player.scoreTotal ?? 0,
      player,
    }));
});

const roleCategories: Array<{ field: ScoreField; label: string }> = [
  { field: 'scoreInfantry', label: 'Infantry' },
  { field: 'scoreSupport', label: 'Support' },
  { field: 'scoreArmor', label: 'Armor' },
  { field: 'scoreAir', label: 'Air' },
];

const leaderCategories: Array<{ field: ScoreField; label: string }> = [
  { field: 'scoreCapturing', label: 'Command points' },
  { field: 'scoreFortification', label: 'Fortifications' },
  { field: 'scoreRepair', label: 'Repairs' },
  { field: 'scoreUnitDamage', label: 'Enemy damage' },
  { field: 'scoreTacticalAid', label: 'Tactical Aid' },
];

const matchLeaders = computed<MatchLeader[]>(() => {
  return leaderCategories.flatMap(({ field, label }) => {
    const player = rankedPlayers(field)[0];
    const score = player?.[field] ?? 0;
    if (!player || score <= 0) {
      return [];
    }
    return [
      {
        label,
        score,
        player,
      },
    ];
  });
});

const roleLeaders = computed<MatchLeader[]>(() => {
  return roleCategories.flatMap(({ field, label }) => {
    const player = rankedPlayers(field)[0];
    const score = player?.[field] ?? 0;
    if (!player || score <= 0) {
      return [];
    }
    return [
      {
        label,
        score,
        player,
      },
    ];
  });
});

const facts = computed(() => {
  const rows = [
    ['Map', props.detail.overview.mapDisplayName || props.detail.overview.mapName || 'Unknown'],
  ];
  if (props.detail.overview.serverModes.length) {
    rows.push(['Server mode', props.detail.overview.serverModes.join(' · ')]);
  }
  if (props.detail.overview.format) {
    rows.push(['Format', props.detail.overview.format]);
  }
  rows.push(
    ['Replay length', formatDuration(props.detail.overview.timing.recordingSeconds)],
    ['Match length', formatDuration(props.detail.overview.durationSeconds)],
    ['Result', endingLabel.value],
    ['Server', props.detail.overview.serverName || 'Unknown'],
    ['Recorded', displayDate(props.detail.overview.dateTime)],
    ['Recorder', props.detail.overview.recorder || 'Unknown'],
  );
  return rows;
});
</script>

<template>
  <div class="overview-surface">
    <div class="detail-scroll overview-layout">
      <section class="fact-grid" aria-label="Replay facts">
        <div v-for="[label, value] in facts" :key="label" class="fact-card">
          <span>{{ label }}</span>
          <strong :class="{ 'recorder-name': label === 'Recorder' }">{{ value }}</strong>
        </div>
      </section>

      <section>
        <div class="section-title-row">
          <div>
            <p class="eyebrow">MATCH ROSTER</p>
            <h3>PLAYERS &amp; TEAMS</h3>
          </div>
        </div>

        <ControlMeter
          v-if="dominationBar.length"
          :segments="dominationBar"
          :title="meterTitle"
          :context="endingLabel"
          :note="partialNote"
        />

        <div class="team-grid">
          <article
            v-for="team in competitiveTeams"
            :key="team.name"
            class="team-card"
            :class="{
              'team-card-allied': team.name === 'USA' || team.name === 'NATO',
              'team-card-soviet': team.name === 'USSR',
              'team-card-winner': team.winner,
            }"
          >
            <header>
              <div>
                <p>{{ team.label }}</p>
                <h4>{{ team.name }}</h4>
              </div>
              <strong>{{ team.score.toLocaleString('en-US') }}</strong>
            </header>
            <div class="player-list">
              <div v-for="(player, index) in team.players" :key="player.id" class="player-row">
                <span class="player-rank">{{ index + 1 }}</span>
                <span class="role-badge" :class="{ 'role-badge-empty': !roleIcon(player.role) }">
                  <img
                    v-if="roleIcon(player.role)"
                    :src="roleIcon(player.role)!"
                    :alt="(player.role || 'unknown').toLowerCase()"
                    :title="player.role?.toLowerCase() || undefined"
                  />
                </span>
                <span class="player-identity">
                  <strong :class="{ 'recorder-name': player.name === detail.overview.recorder }">{{
                    player.name
                  }}</strong>
                </span>
                <span
                  class="player-score"
                  :title="
                    player.scoreBeforeLeave
                      ? 'Last recorded score before leaving; final statistics unavailable'
                      : undefined
                  "
                >
                  {{ (player.score ?? 0).toLocaleString('en-US')
                  }}<sup v-if="player.scoreBeforeLeave" aria-label="Score before leaving">*</sup>
                </span>
              </div>
            </div>
          </article>
        </div>

        <section v-if="spectators.length" class="spectator-roster" aria-label="Spectators">
          <header>
            <h4>Spectators</h4>
          </header>
          <ul>
            <li v-for="player in spectators" :key="player.id">
              <strong :class="{ 'recorder-name': player.name === detail.overview.recorder }">{{
                player.name
              }}</strong>
            </li>
          </ul>
        </section>

        <section
          v-if="topPlayers.length || roleLeaders.length || matchLeaders.length"
          class="match-leaders"
          aria-label="Match leaders"
        >
          <div class="section-title-row">
            <div>
              <p class="eyebrow">PLAYER PERFORMANCE</p>
              <h3>Match leaders</h3>
            </div>
          </div>

          <div v-if="topPlayers.length" class="match-leader-group">
            <h4>Top players</h4>
            <div class="match-leader-list">
              <div v-for="place in topPlayers" :key="place.rank" class="match-leader-row">
                <span class="match-leader-label"
                  ><b>#{{ place.rank }}</b></span
                >
                <strong>
                  <span
                    :class="{ 'recorder-name': place.player.name === detail.overview.recorder }"
                    >{{ place.player.name }}</span
                  >
                </strong>
                <b class="match-leader-score">{{ place.score.toLocaleString('en-US') }}</b>
              </div>
            </div>
          </div>

          <div v-if="roleLeaders.length" class="match-leader-group">
            <h4>Role leaders</h4>
            <div class="match-leader-list">
              <div v-for="leader in roleLeaders" :key="leader.label" class="match-leader-row">
                <span class="role-badge">
                  <img
                    :src="roleIcon(leader.label)!"
                    :alt="leader.label.toLowerCase()"
                    :title="leader.label.toLowerCase()"
                  />
                </span>
                <strong>
                  <span
                    :class="{ 'recorder-name': leader.player.name === detail.overview.recorder }"
                    >{{ leader.player.name }}</span
                  >
                </strong>
                <b class="match-leader-score">{{ leader.score.toLocaleString('en-US') }}</b>
              </div>
            </div>
          </div>

          <div v-if="matchLeaders.length" class="match-leader-group">
            <h4>Score leaders</h4>
            <div class="match-leader-list">
              <div v-for="leader in matchLeaders" :key="leader.label" class="match-leader-row">
                <span class="match-leader-label"
                  ><b>{{ leader.label }}</b></span
                >
                <strong>
                  <span
                    :class="{ 'recorder-name': leader.player.name === detail.overview.recorder }"
                    >{{ leader.player.name }}</span
                  >
                </strong>
                <b class="match-leader-score">{{ leader.score.toLocaleString('en-US') }}</b>
              </div>
            </div>
          </div>
        </section>
      </section>

      <section class="coverage-strip">
        <div>
          <span>Chat coverage</span><strong>{{ detail.coverageChat }}</strong>
        </div>
        <div>
          <span>Tactical Aid coverage</span><strong>{{ detail.coverageTacticalAid }}</strong>
        </div>
        <div>
          <span>Internal map</span><strong>{{ detail.overview.mapName || 'Unknown' }}</strong>
        </div>
      </section>
    </div>
  </div>
</template>
