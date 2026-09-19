// @vitest-environment happy-dom
import { mount } from '@vue/test-utils';
import { expect, it } from 'vitest';
import ReplayScoreboard from './ReplayScoreboard.vue';

it('renders team totals and unknown scores, updates and rewinds without final-result fallback', async () => {
  const wrapper = mount(ReplayScoreboard, {
    props: {
      time: 0,
      participants: [
        { playerId: 0, sessionIndex: 0, playerName: 'Alice', startSeconds: 1, endSeconds: null },
      ],
      teams: [{ playerId: 0, timeSeconds: 1, team: 1 }],
      samples: [{ playerId: 0, timeSeconds: 5, score: -2 }],
    },
  });
  expect(wrapper.text()).toContain('Waiting for recorded players');
  await wrapper.setProps({ time: 1 });
  expect(wrapper.text()).toContain('USA');
  expect(wrapper.get('[aria-label="Score not recorded yet"]').text()).toBe('—');
  await wrapper.setProps({ time: 5 });
  expect(wrapper.get('li').text()).toContain('-2');
  await wrapper.setProps({ time: 2 });
  expect(wrapper.get('li').text()).toContain('—');
  wrapper.unmount();
});
