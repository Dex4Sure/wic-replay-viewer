<script setup lang="ts">
import { computed } from 'vue';

import { factionToneClass } from '../format';
import type { DetailView, MapArtAsset, PlaybackObjective } from '../types';
import { groupTacticalAid } from '../tacticalAidMap';
import { projectPosition } from '../tacticalAidMap';

const props = defineProps<{
  asset: MapArtAsset | null;
  rows: DetailView['tacticalAidRows'];
  objectives: PlaybackObjective[];
  mapName: string;
  selectedIndex: number | null;
  highlightedCategory: { faction: string; support: string } | null;
}>();

const emit = defineEmits<{ select: [index: number] }>();
const plotted = computed(() =>
  props.asset?.bounds ? groupTacticalAid(props.rows, props.asset.bounds) : null,
);
const selected = computed(() => {
  const index = props.selectedIndex;
  if (index === null) {
    return null;
  }
  const row = props.rows[index];
  if (!row) {
    return null;
  }
  return plotted.value?.groups.find((group) => group.rows.includes(row)) ?? null;
});
const captureCircles = computed(() => {
  const bounds = props.asset?.bounds;
  if (!bounds) {
    return [];
  }
  return props.objectives.flatMap((objective) => {
    if (objective.kind !== 'perimeterPoint' || !objective.position) {
      return [];
    }
    const point = projectPosition(objective.position, bounds);
    return point ? [{ key: objective.id ?? `${point.left}:${point.top}`, ...point }] : [];
  });
});

function rowIndex(row: DetailView['tacticalAidRows'][number]): number {
  return props.rows.indexOf(row);
}

function selectMarker(rows: DetailView['tacticalAidRows']): void {
  const current = props.selectedIndex;
  if (current !== null && rows.includes(props.rows[current]!)) {
    const position = rows.indexOf(props.rows[current]!);
    const next = rows[(position + 1) % rows.length]!;
    emit('select', rowIndex(next));
    return;
  }
  const index = rowIndex(rows[0]!);
  if (index >= 0) {
    emit('select', index);
  }
}

function categoryMatches(rows: DetailView['tacticalAidRows']): boolean {
  const category = props.highlightedCategory;
  return (
    category !== null &&
    rows.some((row) => row.faction === category.faction && row.support === category.support)
  );
}

function markerFaction(rows: DetailView['tacticalAidRows']): string | null {
  return categoryMatches(rows)
    ? (props.highlightedCategory?.faction ?? null)
    : (rows[0]?.faction ?? null);
}
</script>

<template>
  <section class="aid-map-section" aria-label="Tactical Aid deployment map">
    <div v-if="!asset" class="aid-map-unavailable">
      Map art is unavailable. Configure your own World in Conflict installation to plot deployments.
    </div>
    <div v-else-if="!asset.bounds" class="aid-map-unavailable">
      This map has no verified terrain bounds, so deployment coordinates cannot be plotted safely.
    </div>
    <template v-else>
      <div class="aid-map-canvas">
        <img :src="asset.imageUrl" :alt="`${mapName} tactical aid map`" decoding="async" />
        <span
          v-for="circle in captureCircles"
          :key="circle.key"
          class="aid-command-point-circle"
          :style="{ left: `${circle.left * 100}%`, top: `${circle.top * 100}%` }"
          aria-hidden="true"
        />
        <button
          v-for="group in plotted?.groups"
          :key="group.key"
          type="button"
          class="aid-map-marker"
          :class="[
            factionToneClass(markerFaction(group.rows)),
            {
              'aid-map-marker-selected': selected?.key === group.key,
              'aid-map-marker-category': categoryMatches(group.rows),
              'aid-map-marker-dimmed': highlightedCategory && !categoryMatches(group.rows),
            },
          ]"
          :style="{ left: `${group.left * 100}%`, top: `${group.top * 100}%` }"
          :aria-label="`${group.rows.length} tactical aid deployment${group.rows.length === 1 ? '' : 's'} at ${group.key}`"
          @click="selectMarker(group.rows)"
        >
          <span v-if="group.rows.length > 1">{{ group.rows.length }}</span>
        </button>
      </div>
      <p v-if="plotted?.omitted" class="aid-map-warning">
        {{ plotted.omitted }} deployment{{ plotted.omitted === 1 ? '' : 's' }} outside verified
        terrain bounds remain in the table below.
      </p>
      <p v-if="highlightedCategory" class="aid-map-hint">
        Highlighting {{ highlightedCategory.faction }} ·
        {{ highlightedCategory.support }} deployments.
      </p>
      <p v-else class="aid-map-hint">Select a marker to jump to its Tactical Aid timeline event.</p>
    </template>
  </section>
</template>
