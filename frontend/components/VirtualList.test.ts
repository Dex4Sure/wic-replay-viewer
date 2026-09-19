// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { ref } from 'vue';
import { expect, it, vi } from 'vitest';

const scrollToIndex = vi.fn();
interface VirtualizerOptions {
  value: {
    count: number;
    getItemKey: (index: number) => string | number;
    estimateSize: (index: number) => number;
  };
}
vi.mock('@tanstack/vue-virtual', () => ({
  useVirtualizer: (options: VirtualizerOptions) =>
    ref({
      getVirtualItems: () =>
        Array.from({ length: options.value.count }, (_, index) => ({
          index,
          key: options.value.getItemKey(index),
          start: index * options.value.estimateSize(index),
          size: options.value.estimateSize(index),
        })),
      getTotalSize: () => 100,
      scrollToIndex,
    }),
}));

import VirtualList from './VirtualList.vue';

it('renders typed rows and bounds exposed scrolling', () => {
  const wrapper = mount(VirtualList, {
    props: {
      items: ['a', 'b'],
      estimateSize: (_item: unknown, index: number) => 20 + index,
      itemKey: (item: unknown) => String(item),
    },
    slots: { default: '<span class="row">{{ item }}</span>' },
  });
  expect(wrapper.findAll('.row')).toHaveLength(2);
  (wrapper.vm as unknown as { scrollToIndex: (index: number) => void }).scrollToIndex(-1);
  (wrapper.vm as unknown as { scrollToIndex: (index: number) => void }).scrollToIndex(2);
  (wrapper.vm as unknown as { scrollToIndex: (index: number) => void }).scrollToIndex(1);
  expect(scrollToIndex).toHaveBeenCalledWith(1, { align: 'center' });
});
