<script setup lang="ts">
import type { DominationSegment } from '../format';

defineProps<{
  segments: DominationSegment[];
  title: string;
  context: string;
  note?: string | null;
}>();
</script>

<template>
  <div class="control-meter" :aria-label="title">
    <div class="control-meter-header">
      <span>{{ title }}</span>
      <small>{{ context }}</small>
    </div>
    <div class="control-meter-track">
      <div
        v-for="segment in segments"
        :key="segment.faction"
        v-show="segment.width > 0"
        class="control-meter-segment"
        :class="{
          'control-meter-segment-allied': segment.faction === 'USA' || segment.faction === 'NATO',
          'control-meter-segment-soviet': segment.faction === 'USSR',
        }"
        :style="{ width: `${segment.width}%` }"
      >
        <span>{{ segment.faction }}</span>
        <strong>{{ segment.label }}%</strong>
      </div>
      <i class="control-meter-midpoint" aria-hidden="true" />
    </div>
    <p v-if="note" class="control-meter-note">{{ note }}</p>
  </div>
</template>
