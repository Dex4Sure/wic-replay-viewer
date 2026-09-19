<script setup lang="ts" generic="T">
import { useVirtualizer } from '@tanstack/vue-virtual';
import { computed, ref } from 'vue';

const props = defineProps<{
  items: T[];
  estimateSize: number | ((item: T, index: number) => number);
  itemKey: (item: T, index: number) => string | number;
}>();

const scrollElement = ref<HTMLElement | null>(null);
const virtualizer = useVirtualizer(
  computed(() => ({
    count: props.items.length,
    getScrollElement: () => scrollElement.value,
    estimateSize: (index: number) =>
      typeof props.estimateSize === 'number'
        ? props.estimateSize
        : props.estimateSize(props.items[index]!, index),
    getItemKey: (index: number) => props.itemKey(props.items[index]!, index),
    overscan: 8,
  })),
);
const virtualItems = computed(() => virtualizer.value.getVirtualItems());

function scrollToIndex(index: number): void {
  if (index < 0 || index >= props.items.length) {
    return;
  }
  virtualizer.value.scrollToIndex(index, { align: 'center' });
}

defineExpose({ scrollToIndex });
</script>

<template>
  <div ref="scrollElement" class="virtual-scroll">
    <div class="relative w-full" :style="{ height: `${virtualizer.getTotalSize()}px` }">
      <div
        v-for="virtualRow in virtualItems"
        :key="String(virtualRow.key)"
        class="absolute left-0 top-0 w-full"
        :style="{ transform: `translateY(${virtualRow.start}px)`, height: `${virtualRow.size}px` }"
      >
        <slot :item="items[virtualRow.index]!" :index="virtualRow.index" />
      </div>
    </div>
  </div>
</template>
