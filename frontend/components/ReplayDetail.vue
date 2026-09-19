<script setup lang="ts">
import { ArrowLeft, FilePlay } from '@lucide/vue';
import { computed, nextTick, ref, watch, watchEffect } from 'vue';

import { getReplayObjectives, getReplayPlayback } from '../api';
import {
  dominationSegments,
  factionTone,
  factionToneClass,
  formatDuration,
  formatTimestamp,
  playerNameSegments,
  replayTitle,
  timelineDescription,
  usesDominationControl,
} from '../format';
import { mapArtFor, mapAssetFor, requestMapArt } from '../mapArt';
import {
  dominationPlaybackAnchor,
  dominationSharesAt,
  revealedReplayEventCount,
} from '../replayPlayback';
import type {
  DetailTab,
  DetailView,
  PlaybackObjective,
  PlaybackView,
  ReplaySummary,
} from '../types';
import OverviewTab from './OverviewTab.vue';
import ControlMeter from './ControlMeter.vue';
import ReplayMap from './ReplayMap.vue';
import ReplayScoreboard from './ReplayScoreboard.vue';
import TacticalAidMap from './TacticalAidMap.vue';
import VirtualList from './VirtualList.vue';

const props = defineProps<{
  summary: ReplaySummary | null;
  detail: DetailView | null;
  loading: boolean;
  error: string | null;
  tab: DetailTab;
  locationCount: number;
  libraryCount: number;
}>();

const emit = defineEmits<{
  'update:tab': [tab: DetailTab];
  manage: [mode: 'rename' | 'name'];
  back: [];
}>();
const selectedAidIndex = ref<number | null>(null);
const highlightedAidCategory = ref<{ faction: string; support: string } | null>(null);
const aidTimeline = ref<{ scrollToIndex: (index: number) => void } | null>(null);
const playback = ref<PlaybackView | null>(null);
const playbackLoading = ref(false);
const playbackError = ref<string | null>(null);
const playbackTime = ref(0);
const tacticalObjectives = ref<PlaybackObjective[]>([]);
const replayTimeline = ref<{ scrollToIndex: (index: number) => void } | null>(null);
let playbackGeneration = 0;
let objectiveGeneration = 0;

watch(
  () => props.detail?.tacticalAidRows,
  () => {
    selectedAidIndex.value = null;
    highlightedAidCategory.value = null;
  },
);

watch(
  () => props.summary?.path,
  () => {
    playback.value = null;
    playbackError.value = null;
    playbackLoading.value = false;
    playbackTime.value = 0;
    tacticalObjectives.value = [];
    playbackGeneration += 1;
    objectiveGeneration += 1;
  },
);

watch(
  [() => props.tab, () => props.summary?.path],
  async ([tab]) => {
    if (tab !== 'replay' || !props.summary || playback.value || playbackLoading.value) {
      return;
    }
    const generation = ++playbackGeneration;
    playbackLoading.value = true;
    playbackError.value = null;
    try {
      const result = await getReplayPlayback(props.summary.path);
      if (generation === playbackGeneration) {
        playback.value = result;
      }
    } catch (error) {
      if (generation === playbackGeneration) {
        playbackError.value = error instanceof Error ? error.message : String(error);
      }
    } finally {
      if (generation === playbackGeneration) {
        playbackLoading.value = false;
      }
    }
  },
  { immediate: true },
);

watch(
  () => props.tab,
  async (tab) => {
    if (tab !== 'tacticalAid' || !props.summary || tacticalObjectives.value.length) {
      return;
    }
    if (playback.value) {
      tacticalObjectives.value = playback.value.objectives;
      return;
    }
    const generation = ++objectiveGeneration;
    try {
      const result = await getReplayObjectives(props.summary.path);
      if (generation === objectiveGeneration) {
        tacticalObjectives.value = result;
      }
    } catch {
      // Command-point context is optional; TA evidence and its table remain usable.
    }
  },
);

function selectAidFromMap(index: number): void {
  selectedAidIndex.value = index;
  highlightedAidCategory.value = null;
  void nextTick(() => aidTimeline.value?.scrollToIndex(index));
}

function selectAidFromTimeline(index: number): void {
  selectedAidIndex.value = index;
  highlightedAidCategory.value = null;
}

function toggleAidCategory(faction: string, support: string): void {
  const current = highlightedAidCategory.value;
  highlightedAidCategory.value =
    current?.faction === faction && current.support === support ? null : { faction, support };
  selectedAidIndex.value = null;
}

// One shared backdrop follows the selected match across every detail tab. Map
// art is optional, so an unconfigured installation or unsupported custom map
// continues to render the normal plain panel.
const backdrop = computed(() => (props.detail ? mapArtFor(props.detail.overview.mapName) : null));
const mapAsset = computed(() => (props.detail ? mapAssetFor(props.detail.overview.mapName) : null));
const eventDescription = (row: DetailView['timelineRows'][number]) =>
  timelineDescription(row, mapAsset.value?.commandPointNames);
const eventDescriptionSegments = (row: DetailView['timelineRows'][number]) =>
  playerNameSegments(eventDescription(row), row.players);
const playerFactionClass = (faction: string | null) => {
  const tone = factionTone(faction);
  return tone ? `player-name-${tone}` : null;
};
watchEffect(() => {
  if (props.detail) {
    requestMapArt(props.detail.overview.mapName);
  }
});

const tabs: Array<{ id: DetailTab; label: string; count?: () => number }> = [
  { id: 'overview', label: 'Overview' },
  { id: 'chat', label: 'Chat', count: () => props.detail?.chatRows.length ?? 0 },
  { id: 'replay', label: 'Replay' },
  {
    id: 'tacticalAid',
    label: 'Tactical Aid',
    count: () => props.detail?.tacticalAidRows.length ?? 0,
  },
];

const revealedReplayEvents = computed(() => {
  const rows = props.detail?.timelineRows ?? [];
  return rows.slice(0, revealedReplayEventCount(rows, playbackTime.value));
});

const aidCoverageLabel = computed(() => {
  const coverage = props.detail?.coverageTacticalAid ?? '';
  if (coverage === 'Both factions; exact player where proven') {
    return 'Both teams';
  }
  return coverage === 'recorderOnly' ? 'Recorder only' : coverage;
});

const aidGroups = computed(() => {
  const groups = new Map<string, DetailView['tacticalAidSummary']['supports']>();
  for (const support of props.detail?.tacticalAidSummary.supports ?? []) {
    const rows = groups.get(support.faction) ?? [];
    rows.push(support);
    groups.set(support.faction, rows);
  }
  return [...groups].map(([faction, supports]) => ({ faction, supports }));
});

const liveWinner = computed(() => {
  // Only reveal a winner from the recorded result row, never the final overview.
  const result = revealedReplayEvents.value.findLast((row) => row.kind === 'Match result');
  return ['USA', 'NATO', 'USSR'].find((team) => result?.description === `${team} won the match`);
});

const liveDominationFactions = computed(() => {
  if (!props.detail) {
    return [];
  }
  return [
    ...(props.detail.overview.dominationShares?.map((share) => share.faction) ?? []),
    ...props.detail.overview.players.flatMap((player) => (player.faction ? [player.faction] : [])),
  ];
});

const liveDominationAnchor = computed(() => {
  if (!props.detail) {
    return null;
  }
  return dominationPlaybackAnchor(
    props.detail.dominationSamples,
    props.detail.dominationAnchorFaction,
    props.detail.overview.dominationShares,
    props.detail.overview.dominationAnchor,
  );
});

const liveDominationBar = computed(() => {
  if (!props.detail || !usesDominationControl(props.detail.overview.gameMode)) {
    return [];
  }
  const shares = dominationSharesAt(
    props.detail.dominationSamples,
    playbackTime.value,
    liveDominationAnchor.value,
    liveDominationFactions.value,
  );
  return shares ? dominationSegments(shares) : [];
});

const liveDominationAvailable = computed(() =>
  Boolean(
    props.detail &&
    usesDominationControl(props.detail.overview.gameMode) &&
    props.detail.dominationSamples.length &&
    dominationSharesAt(
      props.detail.dominationSamples,
      Number.MAX_VALUE,
      liveDominationAnchor.value,
      liveDominationFactions.value,
    ),
  ),
);

const liveMeterTitle = computed(() =>
  props.detail?.overview.gameMode === 'Tug of War' ? 'Live front line' : 'Live domination bar',
);

watch(
  () => revealedReplayEvents.value.length,
  (count) => {
    if (props.tab === 'replay' && count > 0) {
      void nextTick(() => replayTimeline.value?.scrollToIndex(count - 1));
    }
  },
);

function seekReplay(timeSeconds: number): void {
  playbackTime.value = timeSeconds;
}

const subtitle = computed(() => {
  if (!props.summary) {
    return '';
  }
  return [
    props.summary.mapDisplayName || props.summary.mapName || 'Unknown map',
    `${props.summary.playerCount} players`,
    formatDuration(props.summary.recordingSeconds),
  ].join('  •  ');
});

const emptyCopy = computed(() => {
  if (!props.locationCount) {
    return {
      eyebrow: 'Replay library',
      title: 'Add replay folders',
      message:
        'Add folders from the library panel, or drop one or more folders anywhere in this window.',
    };
  }
  if (!props.libraryCount) {
    return {
      eyebrow: `${props.locationCount} ${props.locationCount === 1 ? 'location' : 'locations'} ready`,
      title: 'Scan your library',
      message: 'Scan the saved locations to discover and index their .wicdemo files.',
    };
  }
  return {
    eyebrow: `${props.libraryCount} indexed ${props.libraryCount === 1 ? 'replay' : 'replays'}`,
    title: 'Select a replay',
    message: 'Choose a replay from the library to view its overview, chat, timeline, and playback.',
  };
});
</script>

<template>
  <main class="detail-panel">
    <div v-if="!summary" class="detail-empty">
      <span class="detail-empty-icon" aria-hidden="true">
        <FilePlay :size="38" :stroke-width="1.5" />
      </span>
      <p class="eyebrow">{{ emptyCopy.eyebrow }}</p>
      <h2>{{ emptyCopy.title }}</h2>
      <p>{{ emptyCopy.message }}</p>
    </div>

    <template v-else>
      <header class="detail-header">
        <div class="detail-title">
          <div class="detail-title-navigation">
            <button type="button" class="detail-back-button" @click="emit('back')">
              <ArrowLeft :size="14" :stroke-width="1.8" aria-hidden="true" />
              <span>Back to library</span>
            </button>
            <p class="eyebrow">After-action report</p>
          </div>
          <div class="detail-title-line">
            <h2>{{ replayTitle(summary.fileName) }}</h2>
            <span v-if="summary.incomplete" class="status-chip status-muted">Incomplete</span>
            <span
              v-else-if="summary.winner"
              class="winner-chip"
              :class="factionToneClass(summary.winner)"
            >
              {{ summary.winner }} victory
            </span>
          </div>
          <p>{{ subtitle }}</p>
        </div>
        <div class="replay-identity">
          <div>
            <span>File name</span>
            <strong>{{ summary.fileName }}</strong>
          </div>
          <div>
            <span>In-game name</span>
            <strong>{{ summary.replayName ?? 'Not stored' }}</strong>
          </div>
          <div class="replay-management-actions">
            <button type="button" @click="emit('manage', 'rename')">Rename file</button>
            <button
              type="button"
              :disabled="summary.replayName === null"
              :title="
                summary.replayName === null
                  ? 'This replay has no editable ReplayName field'
                  : undefined
              "
              @click="emit('manage', 'name')"
            >
              Edit in-game name
            </button>
          </div>
        </div>
      </header>

      <nav class="detail-tabs" aria-label="Replay details">
        <button
          v-for="item in tabs"
          :key="item.id"
          type="button"
          :class="{ 'detail-tab-active': tab === item.id }"
          @click="emit('update:tab', item.id)"
        >
          {{ item.label }}
          <span v-if="item.count">{{ item.count() }}</span>
        </button>
      </nav>

      <div v-if="loading" class="loading-state">
        <span class="scanner" aria-hidden="true" />
        <strong>Decoding replay intelligence</strong>
        <p>Loading the selected replay’s cached detail and timeline.</p>
      </div>

      <div v-else-if="error" class="error-state">
        <span>!</span>
        <div>
          <strong>Unable to load replay details</strong>
          <p>{{ error }}</p>
        </div>
      </div>

      <div v-else-if="detail" class="detail-content-surface">
        <div v-if="backdrop" class="detail-backdrop" aria-hidden="true">
          <img :src="backdrop" alt="" decoding="async" />
        </div>
        <OverviewTab v-if="tab === 'overview'" :detail="detail" />

        <section v-else-if="tab === 'replay'" class="data-view replay-data-view">
          <div v-if="playbackLoading" class="loading-state">
            <span class="scanner" aria-hidden="true" />
            <strong>Decoding unit checkpoints</strong>
            <p>This high-volume stream is loaded only when Replay is opened.</p>
          </div>
          <div v-else-if="playbackError" class="error-state">
            <span>!</span>
            <div>
              <strong>Unable to reconstruct replay map</strong>
              <p>{{ playbackError }}</p>
            </div>
          </div>
          <div v-else-if="playback" class="replay-watch-surface">
            <div
              class="replay-watch-layout"
              :class="{ 'replay-watch-layout-with-meter': liveDominationAvailable }"
            >
              <ControlMeter
                v-if="liveDominationAvailable"
                class="replay-live-control-meter"
                :segments="liveDominationBar"
                :title="liveMeterTitle"
                :context="
                  liveDominationBar.length ? formatTimestamp(playbackTime) : 'Awaiting first sample'
                "
              />
              <ReplayMap
                v-model="playbackTime"
                :asset="mapAsset"
                :playback="playback"
                :tactical-aid-rows="detail.tacticalAidRows"
                :map-name="detail.overview.mapName"
              />
              <aside
                class="replay-event-stream replay-event-stream-with-scores"
                aria-label="Replay scores and events"
              >
                <ReplayScoreboard
                  :participants="detail.scoreParticipants"
                  :teams="playback.scoreTeams"
                  :samples="playback.scoreSamples"
                  :time="playbackTime"
                  :winner="liveWinner"
                  :recorder="detail.overview.recorder"
                />
                <header>
                  <div><strong>Event stream</strong><span>Synced to replay time</span></div>
                  <b>{{ revealedReplayEvents.length }} / {{ detail.timelineRows.length }}</b>
                </header>
                <VirtualList
                  ref="replayTimeline"
                  :items="revealedReplayEvents"
                  :estimate-size="58"
                  :item-key="(_, index) => index"
                >
                  <template #default="{ item, index }">
                    <button
                      type="button"
                      class="replay-event-row"
                      :class="{ 'data-row-striped': index % 2 === 1 }"
                      @click="seekReplay(item.timeSeconds)"
                    >
                      <span class="mono-cell">{{ formatTimestamp(item.timeSeconds) }}</span>
                      <span
                        ><strong>{{ item.kind }}</strong
                        ><small
                          ><span
                            v-for="(segment, segmentIndex) in eventDescriptionSegments(item)"
                            :key="segmentIndex"
                            :class="segment.tone && `player-name-${segment.tone}`"
                            >{{ segment.text }}</span
                          ></small
                        ></span
                      >
                    </button>
                  </template>
                </VirtualList>
              </aside>
            </div>
          </div>
        </section>

        <section v-else-if="tab === 'chat'" class="data-view">
          <div class="data-summary">
            <div>
              <span>Visible messages</span><strong>{{ detail.chatRows.length }}</strong>
            </div>
            <div>
              <span>Coverage</span><strong>{{ detail.coverageChat }}</strong>
            </div>
          </div>
          <div class="data-table-header chat-grid">
            <span>Time</span><span>Stage</span><span>Player</span><span>Channel</span
            ><span>Message</span>
          </div>
          <VirtualList :items="detail.chatRows" :estimate-size="44" :item-key="(_, index) => index">
            <template #default="{ item, index }">
              <div class="data-row chat-grid" :class="{ 'data-row-striped': index % 2 === 1 }">
                <span class="mono-cell">{{ item.timeLabel }}</span>
                <span>{{ item.stage }}</span>
                <span class="truncate-cell">{{ item.player }}</span>
                <span
                  ><span class="channel-chip">{{ item.channel }}</span></span
                >
                <span class="truncate-cell">{{ item.message }}</span>
              </div>
            </template>
          </VirtualList>
        </section>

        <section v-else class="data-view tactical-aid-data-view">
          <div class="aid-watch-layout">
            <TacticalAidMap
              :asset="mapAsset"
              :rows="detail.tacticalAidRows"
              :objectives="tacticalObjectives"
              :map-name="detail.overview.mapName"
              :selected-index="selectedAidIndex"
              :highlighted-category="highlightedAidCategory"
              @select="selectAidFromMap"
            />

            <aside class="aid-event-stream" aria-label="Tactical Aid deployment events">
              <div class="aid-stream-summary">
                <div class="aid-summary-grid">
                  <div :title="detail.coverageTacticalAid">
                    <span>Includes</span><strong>{{ aidCoverageLabel }}</strong>
                  </div>
                  <div>
                    <span>Recorder view</span><strong>{{ detail.recorderView }}</strong>
                  </div>
                </div>
                <div class="support-summary" v-if="aidGroups.length">
                  <section
                    v-for="group in aidGroups"
                    :key="group.faction"
                    class="team-card aid-category-card"
                    :class="{
                      'team-card-allied': group.faction === 'USA' || group.faction === 'NATO',
                      'team-card-soviet': group.faction === 'USSR',
                    }"
                    :aria-label="`${group.faction} Tactical Aid`"
                  >
                    <header>
                      <h4>{{ group.faction }}</h4>
                    </header>
                    <button
                      v-for="support in group.supports"
                      :key="support.support"
                      type="button"
                      :class="{
                        'support-summary-active':
                          highlightedAidCategory?.faction === support.faction &&
                          highlightedAidCategory?.support === support.support,
                      }"
                      :aria-label="`${support.faction} · ${support.support}: ${support.placementCount} deployments`"
                      :aria-pressed="
                        highlightedAidCategory?.faction === support.faction &&
                        highlightedAidCategory?.support === support.support
                      "
                      @click="toggleAidCategory(support.faction, support.support)"
                    >
                      <span>{{ support.support }}</span>
                      <strong>{{ support.placementCount }}×</strong>
                    </button>
                  </section>
                </div>
              </div>

              <header>
                <div><strong>Deployment events</strong><span>Linked to map markers</span></div>
                <b>{{ detail.tacticalAidRows.length }}</b>
              </header>

              <VirtualList
                ref="aidTimeline"
                :items="detail.tacticalAidRows"
                :estimate-size="76"
                :item-key="(_, index) => index"
              >
                <template #default="{ item, index }">
                  <button
                    type="button"
                    class="aid-event-row"
                    :class="{
                      'aid-event-row-selected': selectedAidIndex === index,
                      'data-row-striped': index % 2 === 1,
                    }"
                    @click="selectAidFromTimeline(index)"
                  >
                    <span class="mono-cell">{{ formatTimestamp(item.timeSeconds) }}</span>
                    <span class="aid-event-copy">
                      <span class="aid-event-title">
                        <strong>{{ item.support }}</strong>
                        <span class="faction-chip" :class="factionToneClass(item.faction)">{{
                          item.faction
                        }}</span>
                      </span>
                      <small>
                        <span class="truncate-cell" :class="playerFactionClass(item.faction)">{{
                          item.player
                        }}</span>
                        <span>{{ item.playerAttribution }}</span>
                      </small>
                      <small class="aid-event-meta">
                        <span>{{ item.position.map((value) => value.toFixed(1)).join(', ') }}</span>
                      </small>
                    </span>
                  </button>
                </template>
              </VirtualList>
            </aside>
          </div>
        </section>
      </div>
    </template>
  </main>
</template>
