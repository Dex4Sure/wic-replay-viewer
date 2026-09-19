<script setup lang="ts">
import { Pause, Play } from '@lucide/vue';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';

import { factionToneClass, formatTimestamp, teamFaction } from '../format';
import { activeAreaEffects } from '../replayAreaEffects';
import { createReplayPlaybackTimer, showReplayMapUnit } from '../replayPlayback';
import { layoutTaLabels } from '../taLabelLayout';
import { projectPosition } from '../tacticalAidMap';
import type { DetailView, MapArtAsset, PlaybackView } from '../types';

const props = defineProps<{
  asset: MapArtAsset | null;
  playback: PlaybackView;
  tacticalAidRows: DetailView['tacticalAidRows'];
  mapName: string;
  modelValue: number;
}>();

const emit = defineEmits<{ 'update:modelValue': [time: number] }>();

const time = computed({
  get: () => props.modelValue,
  set: (value: number) => emit('update:modelValue', value),
});
const playing = ref(false);
const speed = ref(1);
const playbackSpeeds = [0.5, 1, 2, 4] as const;
const speedControl = ref<HTMLElement | null>(null);
const speedTrigger = ref<HTMLButtonElement | null>(null);
const speedMenu = ref<HTMLElement | null>(null);
const speedMenuOpen = ref(false);

type PlaybackSpeed = (typeof playbackSpeeds)[number];

async function openSpeedMenu(focus: 'first' | 'last' | null = null): Promise<void> {
  speedMenuOpen.value = true;
  if (!focus) {
    return;
  }
  await nextTick();
  const options = speedMenu.value?.querySelectorAll<HTMLButtonElement>('button');
  options?.[focus === 'first' ? 0 : options.length - 1]?.focus();
}

function closeSpeedMenu(returnFocus = false): void {
  speedMenuOpen.value = false;
  if (returnFocus) {
    void nextTick(() => speedTrigger.value?.focus());
  }
}

function toggleSpeedMenu(): void {
  if (speedMenuOpen.value) {
    closeSpeedMenu();
  } else {
    void openSpeedMenu();
  }
}

function selectSpeed(option: PlaybackSpeed, event: MouseEvent): void {
  speed.value = option;
  closeSpeedMenu(event.detail === 0);
}

function handleSpeedMenuKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault();
    event.stopPropagation();
    closeSpeedMenu(true);
    return;
  }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    return;
  }
  event.preventDefault();
  const options = Array.from(speedMenu.value?.querySelectorAll<HTMLButtonElement>('button') ?? []);
  if (!options.length) {
    return;
  }
  const current = options.indexOf(document.activeElement as HTMLButtonElement);
  let next = current;
  if (event.key === 'Home') {
    next = 0;
  } else if (event.key === 'End') {
    next = options.length - 1;
  } else if (event.key === 'ArrowDown') {
    next = (current + 1 + options.length) % options.length;
  } else {
    next = (current - 1 + options.length) % options.length;
  }
  options[next]?.focus();
}

function closeSpeedMenuOnOutsidePointer(event: PointerEvent): void {
  if (!speedControl.value?.contains(event.target as Node)) {
    closeSpeedMenu();
  }
}

function closeSpeedMenuOnFocusOut(event: FocusEvent): void {
  const next = event.relatedTarget;
  if (!(next instanceof Node) || !speedControl.value?.contains(next)) {
    closeSpeedMenu();
  }
}

function latest<T>(rows: T[], seconds: number, getTime: (row: T) => number): T | null {
  let low = 0,
    high = rows.length - 1,
    found: T | null = null;
  while (low <= high) {
    const middle = (low + high) >> 1;
    const row = rows[middle]!;
    if (getTime(row) <= seconds) {
      found = row;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return found;
}

function teamClass(team: number | null): string {
  return factionToneClass(teamFaction(team)) ?? '';
}

const units = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return props.playback.units.flatMap((unit) => {
    if (!showReplayMapUnit(unit)) {
      return [];
    }
    if (
      unit.createdSeconds > time.value ||
      (unit.terminal && unit.terminal.timeSeconds <= time.value)
    ) {
      return [];
    }
    const frame = latest(unit.frames, time.value, (row) => row.timeSeconds);
    const point = projectPosition(frame?.position ?? unit.spawnPosition, bounds);
    if (!point) {
      return [];
    }
    const team = latest(unit.teams, time.value, (row) => row.timeSeconds)?.team ?? unit.team;
    return [{ key: `${unit.unitId}:${unit.generation}`, ...point, team }];
  });
});

const objectives = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return props.playback.objectives.flatMap((objective) => {
    if (objective.kind !== 'perimeterPoint' || !objective.position) {
      return [];
    }
    const point = projectPosition(objective.position, bounds);
    if (!point) {
      return [];
    }
    const team =
      latest(
        props.playback.objectiveChanges.filter(
          (change) => change.kind === objective.kind && change.id === objective.id,
        ),
        time.value,
        (row) => row.timeSeconds,
      )?.team ?? objective.team;
    return [
      {
        key: objective.id ?? `${point.left}:${point.top}`,
        commandPointId: objective.parentId,
        ...point,
        team,
      },
    ];
  });
});

const deaths = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return props.playback.units.flatMap((unit) => {
    if (!showReplayMapUnit(unit)) {
      return [];
    }
    const terminal = unit.terminal;
    if (!terminal || terminal.kind !== 'destroyed') {
      return [];
    }
    const age = time.value - terminal.timeSeconds;
    if (age < 0 || age > 1.5) {
      return [];
    }
    const frame = latest(unit.frames, terminal.timeSeconds, (row) => row.timeSeconds);
    const point = projectPosition(frame?.position ?? unit.spawnPosition, bounds);
    return point ? [{ key: `${unit.unitId}:${unit.generation}:death`, ...point, age }] : [];
  });
});

// Shipped myMarkerTimeToLive values for every top-level TA, verified by
// research/findings/tactical-aid-timers-2026-09-11.md. Expiry is not an impact event.
const tacticalAidDurations = new Map([
  ['Airborne Infantry', 35],
  ['Airdropped Light Tank', 35],
  ['Airdropped Transport', 35],
  ['Aerial Recon', 15],
  ['Nuclear Strike', 16],
  ['Carpet Bombing', 15],
  ['Repair Bridge', 15],
  ['Napalm Strike', 20],
  ['Tank Buster', 12],
  ['Laser Guided Bomb', 13],
  ['Air-to-Air Strike', 12],
  ['Chemical Strike', 15],
  ['Heavy Air Support', 15],
  ['Light Artillery Barrage', 10],
  ['Precision Artillery', 10],
  ['Heavy Artillery Barrage', 12],
  ['Airstrike', 20],
  ['Daisy Cutter Bomb', 18],
  ['Fuel Air Bomb', 18],
]);

const unitDropSupports = new Set([
  'Airborne Infantry',
  'Airdropped Light Tank',
  'Airdropped Transport',
]);
const effects = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return props.tacticalAidRows.flatMap((row, index) => {
    const age = time.value - row.timeSeconds;
    // USSR Bunkerbuster is the only faction-specific marker lifetime.
    const countdownDuration =
      row.support === 'Laser Guided Bomb' && row.faction === 'USSR'
        ? 11
        : tacticalAidDurations.get(row.support);
    const duration = countdownDuration ?? 1.5;
    if (age < 0 || age >= duration) {
      return [];
    }
    const point = projectPosition(row.position, bounds);
    return point
      ? [
          {
            key: `${index}:${row.timeSeconds}`,
            ...point,
            age,
            faction: row.faction,
            groupingCategory: unitDropSupports.has(row.support)
              ? ('unit-drop' as const)
              : ('other' as const),
            support: row.support,
            player: row.player,
            remainingSeconds: countdownDuration === undefined ? null : Math.ceil(duration - age),
          },
        ]
      : [];
  });
});

const canvas = ref<HTMLElement | null>(null);
const areaBlasts = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return activeAreaEffects(props.playback.areaEffects, time.value).flatMap((effect) => {
    const point = projectPosition(effect.position, bounds);
    if (!point) {
      return [];
    }
    const progress = effect.age / effect.durationSeconds;
    return [
      {
        ...effect,
        ...point,
        width: ((effect.radius * 2) / (bounds.maxX - bounds.minX)) * 100,
        height: ((effect.radius * 2) / (bounds.maxZ - bounds.minZ)) * 100,
        opacity: effect.kind === 'explosion' ? 1 - progress : Math.min(1, (1 - progress) * 5),
        wave: Math.min(1, effect.age / 0.6),
      },
    ];
  });
});
const nuclearBlasts = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return props.playback.nuclearEffects.flatMap((effect, index) => {
    const age = time.value - effect.timeSeconds;
    if (age < 0 || age >= 4) {
      return [];
    }
    const point = projectPosition(effect.position, bounds);
    if (!point) {
      return [];
    }
    return [
      {
        key: `${index}:${effect.timeSeconds}`,
        ...point,
        age,
        width: (440 / (bounds.maxX - bounds.minX)) * 100,
        height: (440 / (bounds.maxZ - bounds.minZ)) * 100,
        wave: Math.min(1, (age * 85) / 220),
      },
    ];
  });
});
const canvasSize = ref({ width: 800, height: 800 });
let canvasObserver: ResizeObserver | undefined;
watch(
  canvas,
  (element) => {
    canvasObserver?.disconnect();
    if (!element) {
      return;
    }
    const measure = () => {
      if (element.clientWidth && element.clientHeight) {
        canvasSize.value = { width: element.clientWidth, height: element.clientHeight };
      }
    };
    measure();
    canvasObserver = new ResizeObserver(measure);
    canvasObserver.observe(element);
  },
  { flush: 'post' },
);
let labelLayoutKey = '';
let cachedLabelLayout: ReturnType<typeof layoutTaLabels> = [];
watch(
  () => props.playback,
  () => {
    labelLayoutKey = '';
  },
);
const labelGroups = computed(() => {
  const byKey = new Map(effects.value.map((effect) => [effect.key, effect]));
  const { width, height } = canvasSize.value;
  // Sample occupancy when membership/viewport changes, not on every countdown tick.
  const key = JSON.stringify([
    width,
    height,
    effects.value.map((e) => [e.key, e.left, e.top, e.faction, e.groupingCategory]),
  ]);
  if (key !== labelLayoutKey) {
    cachedLabelLayout = layoutTaLabels(effects.value, width, height, [
      ...units.value,
      ...objectives.value.map((point) => ({ ...point, radius: 16 })),
      ...areaBlasts.value.map((effect) => ({
        ...effect,
        radius: Math.max(effect.width * width, effect.height * height) / 200,
      })),
      ...nuclearBlasts.value.map((effect) => ({ ...effect, radius: 50 })),
    ]);
    labelLayoutKey = key;
  }
  return cachedLabelLayout.map((group) => ({
    ...group,
    effects: group.members.map((key) => byKey.get(key)!),
  }));
});
const hoveredTa = ref<string | null>(null);
const focusedTa = ref<string | null>(null);
const activeTa = computed(() => hoveredTa.value ?? focusedTa.value);
function clearTaHighlight() {
  hoveredTa.value = null;
  focusedTa.value = null;
}
watch(() => props.playback, clearTaHighlight);
watch(labelGroups, (groups) => {
  const keys = new Set(groups.flatMap((g) => g.members));
  if (hoveredTa.value && !keys.has(hoveredTa.value)) {
    hoveredTa.value = null;
  }
  if (focusedTa.value && !keys.has(focusedTa.value)) {
    focusedTa.value = null;
  }
});
const playbackTimer = createReplayPlaybackTimer({
  duration: () => props.playback.durationSeconds,
  speed: () => speed.value,
  time: () => time.value,
  setPlaying: (value) => {
    playing.value = value;
  },
  setTime: (value) => {
    time.value = value;
  },
});

function toggle(): void {
  if (playing.value) {
    playbackTimer.stop();
    return;
  }
  if (time.value >= props.playback.durationSeconds) {
    time.value = 0;
  }
  playbackTimer.start();
}

watch(
  () => props.playback,
  () => {
    time.value = 0;
    closeSpeedMenu();
    playbackTimer.stop();
  },
);
onMounted(() => document.addEventListener('pointerdown', closeSpeedMenuOnOutsidePointer));
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', closeSpeedMenuOnOutsidePointer);
  playbackTimer.stop();
  canvasObserver?.disconnect();
});
</script>

<template>
  <section class="replay-map" aria-label="Replay map playback">
    <div v-if="!asset" class="aid-map-unavailable">
      Map art is unavailable. Configure your World in Conflict installation to play this replay on
      its map.
    </div>
    <div v-else-if="!asset.bounds" class="aid-map-unavailable">
      This map has no verified terrain bounds, so unit coordinates cannot be plotted safely.
    </div>
    <template v-else>
      <div ref="canvas" class="replay-map-canvas">
        <img :src="asset.imageUrl" :alt="`${mapName} replay map`" decoding="async" />
        <div
          v-for="blast in nuclearBlasts"
          :key="blast.key"
          class="replay-nuclear-effect"
          role="img"
          aria-label="Recorded nuclear detonation"
          :style="{
            left: `${blast.left * 100}%`,
            top: `${blast.top * 100}%`,
            width: `${blast.width}%`,
            height: `${blast.height}%`,
          }"
        >
          <span
            class="replay-nuclear-glow"
            :style="{ opacity: Math.max(0, 1 - blast.age / 4) * 0.55 }"
          />
          <span
            class="replay-nuclear-flash"
            :style="{ opacity: Math.max(0, 1 - blast.age / 0.8) * 0.8 }"
          />
          <span
            class="replay-nuclear-wave"
            :style="{
              transform: `scale(${blast.wave})`,
              opacity: Math.min(1, (4 - blast.age) / 1.4),
            }"
          />
        </div>
        <div
          v-for="effect in areaBlasts"
          :key="effect.key"
          class="replay-area-effect"
          :class="`replay-area-${effect.kind}`"
          role="img"
          :aria-label="`Recorded ${effect.kind} effect`"
          :style="{
            left: `${effect.left * 100}%`,
            top: `${effect.top * 100}%`,
            width: `${effect.width}%`,
            height: `${effect.height}%`,
            opacity: effect.opacity,
          }"
        >
          <span
            v-if="effect.kind === 'explosion'"
            class="replay-area-wave"
            :style="{ transform: `scale(${effect.wave})` }"
          />
        </div>
        <span
          v-for="objective in objectives"
          :key="objective.key"
          class="replay-objective"
          :class="teamClass(objective.team)"
          :style="{ left: `${objective.left * 100}%`, top: `${objective.top * 100}%` }"
        />
        <span
          v-for="unit in units"
          :key="unit.key"
          class="replay-unit"
          :class="teamClass(unit.team)"
          :style="{ left: `${unit.left * 100}%`, top: `${unit.top * 100}%` }"
        />
        <svg
          class="replay-ta-connectors"
          :viewBox="`0 0 ${canvasSize.width} ${canvasSize.height}`"
          aria-hidden="true"
        >
          <g v-for="group in labelGroups" :key="group.key">
            <line
              v-for="effect in group.effects"
              :key="effect.key"
              :class="[
                factionToneClass(effect.faction),
                {
                  'replay-ta-line-active': activeTa === effect.key,
                  'replay-ta-line-dim':
                    activeTa !== effect.key && group.members.includes(activeTa ?? ''),
                },
              ]"
              :x1="effect.left * canvasSize.width"
              :y1="effect.top * canvasSize.height"
              :x2="
                Math.max(
                  group.left,
                  Math.min(group.left + group.width, effect.left * canvasSize.width),
                )
              "
              :y2="
                Math.max(
                  group.top,
                  Math.min(group.top + group.height, effect.top * canvasSize.height),
                )
              "
            />
          </g>
        </svg>
        <span
          v-for="effect in effects"
          :key="effect.key"
          class="replay-ta-anchor"
          :class="[
            factionToneClass(effect.faction),
            { 'replay-ta-anchor-active': activeTa === effect.key },
          ]"
          :style="{ left: `${effect.left * 100}%`, top: `${effect.top * 100}%` }"
          aria-hidden="true"
        />
        <template v-for="group in labelGroups" :key="group.key">
          <span
            v-if="group.effects.length === 1"
            class="replay-effect replay-ta-card"
            :class="[
              factionToneClass(group.effects[0]!.faction),
              { 'replay-ta-card-active': activeTa === group.effects[0]!.key },
            ]"
            tabindex="0"
            @mouseenter="hoveredTa = group.effects[0]!.key"
            @mouseleave="hoveredTa = null"
            @focus="focusedTa = group.effects[0]!.key"
            @blur="focusedTa = null"
            role="img"
            :aria-label="
              group.effects[0]!.remainingSeconds !== null
                ? `${group.effects[0]!.support}, ${group.effects[0]!.player}, ${group.effects[0]!.remainingSeconds} seconds remaining`
                : `${group.effects[0]!.support}, ${group.effects[0]!.player}`
            "
            :style="{
              left: `${group.left}px`,
              top: `${group.top}px`,
              width: `${group.width}px`,
              height: `${group.height}px`,
              opacity:
                group.effects[0]!.remainingSeconds !== null ? 1 : 1 - group.effects[0]!.age / 1.5,
            }"
          >
            <span class="replay-ta-card-label" aria-hidden="true">
              <span class="replay-effect-name">{{ group.effects[0]!.support }}</span>
              <small class="replay-ta-player">{{ group.effects[0]!.player }}</small>
            </span>
            <span
              v-if="group.effects[0]!.remainingSeconds !== null"
              class="replay-effect-countdown"
              aria-hidden="true"
              >{{ group.effects[0]!.remainingSeconds }}s</span
            >
          </span>
          <ul
            v-else
            class="replay-ta-stack"
            aria-label="Grouped Tactical Aid"
            @scroll="hoveredTa = null"
            :style="{
              left: `${group.left}px`,
              top: `${group.top}px`,
              width: `${group.width}px`,
              height: `${group.height}px`,
            }"
          >
            <li
              v-for="effect in group.effects"
              :key="effect.key"
              class="replay-ta-card"
              :class="[
                factionToneClass(effect.faction),
                { 'replay-ta-card-active': activeTa === effect.key },
              ]"
              tabindex="0"
              @mouseenter="hoveredTa = effect.key"
              @mouseleave="hoveredTa = null"
              @focus="focusedTa = effect.key"
              @blur="focusedTa = null"
            >
              <span class="replay-ta-card-label"
                >{{ effect.support
                }}<small class="replay-ta-player">{{ effect.player }}</small></span
              >
              <span class="replay-effect-countdown">{{
                effect.remainingSeconds === null ? 'Event' : `${effect.remainingSeconds}s`
              }}</span>
            </li>
          </ul>
        </template>
        <span
          v-for="death in deaths"
          :key="death.key"
          class="replay-death"
          :style="{
            left: `${death.left * 100}%`,
            top: `${death.top * 100}%`,
            opacity: 1 - death.age / 1.5,
          }"
          >×</span
        >
      </div>
    </template>
    <div class="replay-controls">
      <button
        type="button"
        class="replay-play-button"
        :aria-label="playing ? 'Pause replay' : 'Play replay'"
        @click="toggle"
      >
        <Pause v-if="playing" :size="17" :stroke-width="2" aria-hidden="true" />
        <Play v-else :size="17" :stroke-width="2" aria-hidden="true" />
      </button>
      <input
        v-model.number="time"
        type="range"
        min="0"
        :max="playback.durationSeconds"
        step="0.01"
        aria-label="Replay time"
      />
      <div ref="speedControl" class="replay-speed-control" @focusout="closeSpeedMenuOnFocusOut">
        <button
          ref="speedTrigger"
          type="button"
          class="replay-speed-trigger"
          aria-haspopup="menu"
          aria-controls="replay-speed-menu"
          :aria-expanded="speedMenuOpen"
          @click="toggleSpeedMenu"
          @keydown.down.prevent="openSpeedMenu('first')"
          @keydown.up.prevent="openSpeedMenu('last')"
          @keydown.esc.stop.prevent="closeSpeedMenu()"
        >
          <span>{{ speed }}×</span>
          <span
            class="replay-speed-chevron"
            :class="{ 'replay-speed-chevron-open': speedMenuOpen }"
            aria-hidden="true"
            >⌄</span
          >
        </button>
        <div
          v-if="speedMenuOpen"
          id="replay-speed-menu"
          ref="speedMenu"
          class="replay-speed-menu"
          role="menu"
          aria-label="Replay speed"
          @keydown="handleSpeedMenuKeydown"
        >
          <button
            v-for="option in playbackSpeeds"
            :key="option"
            type="button"
            role="menuitemradio"
            :class="{ 'replay-speed-button-active': speed === option }"
            :aria-checked="speed === option"
            @click="selectSpeed(option, $event)"
          >
            {{ option }}×
          </button>
        </div>
      </div>
      <strong class="mono-cell"
        >{{ formatTimestamp(time) }} / {{ formatTimestamp(playback.durationSeconds) }}</strong
      >
    </div>
    <p class="aid-map-hint">
      {{ units.length }} active units · {{ playback.frameCount.toLocaleString() }} recorded
      checkpoints · hold-last between checkpoints
    </p>
  </section>
</template>
