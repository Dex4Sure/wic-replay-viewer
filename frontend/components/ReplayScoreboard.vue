<script setup lang="ts">
import { computed } from 'vue';
import { createScoreboard } from '../replayScoreboard';
import type { ScoreParticipant, ScoreSample, ScoreTeamChange } from '../types';

const props = defineProps<{
  participants: ScoreParticipant[];
  teams: ScoreTeamChange[];
  samples: ScoreSample[];
  time: number;
  winner?: string;
  recorder?: string | null;
}>();
const scoreboard = computed(() => createScoreboard(props.participants, props.teams, props.samples));
const groups = computed(() => scoreboard.value(props.time));
const scoreText = (score: number | null): string => (score === null ? '—' : score.toLocaleString());
</script>

<template>
  <section class="replay-scoreboard" aria-label="Live scoreboard">
    <p v-if="!groups.length" class="replay-scoreboard-empty">Waiting for recorded players</p>
    <section
      v-for="group in groups"
      :key="group.team"
      class="team-card replay-score-team"
      :class="{
        'team-card-allied': group.label === 'USA' || group.label === 'NATO',
        'team-card-soviet': group.label === 'USSR',
        'team-card-winner': group.label === winner,
      }"
      :aria-label="`${group.label} scores`"
    >
      <header>
        <h4>{{ group.label }}</h4>
        <strong title="Sum of the displayed players’ recorded scores">{{
          scoreText(group.total)
        }}</strong>
      </header>
      <ol class="player-list">
        <li v-for="player in group.players" :key="player.key" class="player-row">
          <span class="player-identity"
            ><strong :title="player.name" :class="{ 'recorder-name': player.name === recorder }">{{
              player.name
            }}</strong></span
          >
          <strong
            class="player-score"
            :aria-label="player.score === null ? 'Score not recorded yet' : undefined"
            >{{ scoreText(player.score) }}</strong
          >
        </li>
      </ol>
    </section>
  </section>
</template>
