// @vitest-environment happy-dom

import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { playbackView, replayDetail, replaySummary } from '../test/factories';

const mocks = vi.hoisted(() => ({ getReplayPlayback: vi.fn(), getReplayObjectives: vi.fn() }));
vi.mock('../api', () => mocks);
vi.mock('../mapArt', () => ({
  mapArtFor: vi.fn(() => null),
  mapAssetFor: vi.fn(() => null),
  requestMapArt: vi.fn(),
}));

import ReplayDetail from './ReplayDetail.vue';

const virtualListStub = {
  props: ['items'],
  template:
    '<div><template v-for="(item, index) in items"><slot :item="item" :index="index" /></template></div>',
  methods: { scrollToIndex: vi.fn() },
};

function props(tab: 'overview' | 'replay' | 'chat' | 'tacticalAid' = 'overview') {
  return {
    summary: replaySummary(),
    detail: replayDetail(),
    loading: false,
    error: null,
    tab,
    locationCount: 1,
    libraryCount: 1,
  };
}

function mountDetail(overrides: Record<string, unknown> = {}, realReplayMap = false) {
  return mount(ReplayDetail, {
    props: { ...props(), ...overrides },
    global: {
      stubs: {
        VirtualList: virtualListStub,
        OverviewTab: true,
        ReplayMap: !realReplayMap,
        TacticalAidMap: true,
        ControlMeter: true,
      },
    },
  });
}

beforeEach(() => vi.clearAllMocks());

describe('ReplayDetail', () => {
  it.each([
    [{ summary: null, locationCount: 0, libraryCount: 0 }, 'Add replay folders'],
    [{ summary: null, locationCount: 2, libraryCount: 0 }, 'Scan your library'],
    [{ summary: null, locationCount: 2, libraryCount: 3 }, 'Select a replay'],
    [{ loading: true }, 'Decoding replay intelligence'],
    [{ error: 'broken' }, 'Unable to load replay details'],
  ])('renders empty/loading/error state semantics', (overrides, copy) => {
    expect(mountDetail(overrides).text()).toContain(copy);
  });

  it('emits navigation, management, and tab changes with visible counts', async () => {
    const detail = replayDetail();
    detail.chatRows.push({
      timeLabel: '0:01',
      stage: 'Match',
      player: 'Alice',
      channel: 'All',
      message: 'hi',
    });
    const wrapper = mountDetail({ detail });
    await wrapper.get('.detail-back-button').trigger('click');
    await wrapper.findAll('.replay-management-actions button')[0]!.trigger('click');
    await wrapper.findAll('.replay-management-actions button')[1]!.trigger('click');
    await wrapper.findAll('.detail-tabs button')[1]!.trigger('click');
    expect(wrapper.emitted('back')).toHaveLength(1);
    expect(wrapper.emitted('manage')).toEqual([['rename'], ['name']]);
    expect(wrapper.emitted('update:tab')).toContainEqual(['chat']);
    expect(wrapper.text()).toContain('Chat 1');
  });

  it('renders chat semantics through accessible visible text', () => {
    const detail = replayDetail();
    detail.chatRows = [
      { timeLabel: '0:01', stage: 'Match', player: 'Alice', channel: 'All', message: 'hello' },
    ];
    const wrapper = mountDetail({ detail, tab: 'chat' });
    expect(wrapper.text()).toContain('Visible messages1');
    expect(wrapper.text()).toContain('Alice');
    expect(wrapper.text()).toContain('hello');
  });

  it('lazy-loads playback and reports failures without crossing replay switches', async () => {
    mocks.getReplayPlayback.mockResolvedValueOnce(playbackView());
    const wrapper = mountDetail();
    await wrapper.setProps({ tab: 'replay' });
    await flushPromises();
    expect(mocks.getReplayPlayback).toHaveBeenCalledWith('/replays/example.wicdemo');
    expect(wrapper.findComponent({ name: 'ReplayMap' }).exists()).toBe(true);

    await wrapper.setProps({ summary: replaySummary({ path: '/second' }), tab: 'overview' });
    mocks.getReplayPlayback.mockRejectedValueOnce(new Error('decode failed'));
    await wrapper.setProps({ tab: 'replay' });
    await flushPromises();
    expect(wrapper.text()).toContain('decode failed');
  });

  it('loads tactical objectives independently and highlights categories without filtering rows', async () => {
    const detail = replayDetail();
    detail.tacticalAidRows = [
      {
        timeSeconds: 1,
        support: 'Nuclear Strike',
        player: 'Alice',
        faction: 'USA',
        playerAttribution: 'Exact player',
        honorsCost: 80,
        position: [1, 2, 3],
      },
      {
        timeSeconds: 2,
        support: 'Artillery',
        player: 'Team USSR',
        faction: 'USSR',
        playerAttribution: 'Team only',
        honorsCost: null,
        position: [4, 5, 6],
      },
    ];
    detail.coverageTacticalAid = 'Both factions; exact player where proven';
    detail.tacticalAidSummary = {
      player: 'Alice',
      totalPlacements: 2,
      supports: [
        { support: 'Nuclear Strike', faction: 'USA', placementCount: 1, observedCosts: [80] },
        { support: 'Artillery', faction: 'USSR', placementCount: 1, observedCosts: [] },
      ],
    };
    mocks.getReplayObjectives.mockResolvedValue([
      { kind: 'commandPoint', id: 1, parentId: null, position: [1, 0, 1], team: 1 },
    ]);
    const wrapper = mountDetail({ detail });
    await wrapper.setProps({ tab: 'tacticalAid' });
    await flushPromises();
    expect(mocks.getReplayObjectives).toHaveBeenCalled();
    expect(wrapper.get('.aid-summary-grid').text()).toContain('IncludesBoth teams');
    expect(wrapper.get('.aid-summary-grid > div').attributes('title')).toBe(
      detail.coverageTacticalAid,
    );
    expect(wrapper.findAll('.aid-event-row')).toHaveLength(2);
    await wrapper.getComponent({ name: 'TacticalAidMap' }).vm.$emit('select', 1);
    await wrapper.vm.$nextTick();
    expect(wrapper.findAll('.aid-event-row')[1]!.classes()).toContain('aid-event-row-selected');
    expect(wrapper.findAll('.aid-category-card h4').map((header) => header.text())).toEqual([
      'USA',
      'USSR',
    ]);
    const category = wrapper.get('[aria-label="USA · Nuclear Strike: 1 deployments"]');
    await category.trigger('click');
    expect(category.attributes('aria-pressed')).toBe('true');
    expect(wrapper.getComponent({ name: 'TacticalAidMap' }).props('highlightedCategory')).toEqual({
      faction: 'USA',
      support: 'Nuclear Strike',
    });
    expect(wrapper.findAll('.aid-event-row')).toHaveLength(2);
    await category.trigger('click');
    expect(category.attributes('aria-pressed')).toBe('false');
    await wrapper.findAll('.aid-event-row')[1]!.trigger('click');
    expect(wrapper.findAll('.aid-event-row')[1]!.classes()).toContain('aid-event-row-selected');
  });

  it('keeps the scoreboard on the map playback clock, including backward seeks', async () => {
    const detail = replayDetail({
      scoreParticipants: [
        { playerId: 0, sessionIndex: 0, playerName: 'Alice', startSeconds: 0, endSeconds: null },
      ],
    });
    mocks.getReplayPlayback.mockResolvedValueOnce(
      playbackView({
        scoreTeams: [{ playerId: 0, timeSeconds: 0, team: 1 }],
        scoreSamples: [{ playerId: 0, timeSeconds: 5, score: 12 }],
      }),
    );
    const wrapper = mountDetail({ detail });
    await wrapper.setProps({ tab: 'replay' });
    await flushPromises();
    const map = wrapper.findComponent({ name: 'ReplayMap' });
    expect(wrapper.get('.replay-scoreboard li').text()).toContain('—');
    map.vm.$emit('update:modelValue', 6);
    await flushPromises();
    expect(wrapper.get('.replay-scoreboard li').text()).toContain('12');
    map.vm.$emit('update:modelValue', 1);
    await flushPromises();
    expect(wrapper.get('.replay-scoreboard li').text()).toContain('—');
    wrapper.unmount();
  });

  it.each(['USA', 'NATO', 'USSR', null])(
    'reveals only the recorded %s winner at the result and clears it on rewind',
    async (winner) => {
      const detail = replayDetail({
        scoreParticipants: [1, 2, 3].map((team) => ({
          playerId: team,
          sessionIndex: 0,
          playerName: `Player ${team}`,
          startSeconds: 0,
          endSeconds: null,
        })),
        timelineRows: [
          {
            timeSeconds: 10,
            kind: 'Match result',
            description: winner ? `${winner} won the match` : 'Match ended without a valid winner',
            players: [],
            commandPointId: null,
            commandPointTeam: null,
          },
        ],
      });
      mocks.getReplayPlayback.mockResolvedValueOnce(
        playbackView({
          scoreTeams: [1, 2, 3].map((team) => ({ playerId: team, timeSeconds: 0, team })),
        }),
      );
      const wrapper = mountDetail({ detail });
      await wrapper.setProps({ tab: 'replay' });
      await flushPromises();
      const map = wrapper.findComponent({ name: 'ReplayMap' });
      expect(wrapper.find('.team-card-winner').exists()).toBe(false);
      for (const time of [9.9, 10, 15, 9.9]) {
        map.vm.$emit('update:modelValue', time);
        await flushPromises();
        const cards = wrapper.findAll('.team-card-winner');
        expect(cards).toHaveLength(time >= 10 && winner ? 1 : 0);
        if (cards.length) {
          expect(cards[0]!.get('h4').text()).toBe(winner);
        }
      }
      wrapper.unmount();
    },
  );

  it('pauses across detail tabs, preserves the position, and resets for a different replay', async () => {
    mocks.getReplayPlayback.mockResolvedValue(playbackView({ durationSeconds: 60 }));
    const wrapper = mountDetail({ tab: 'replay' }, true);
    await flushPromises();
    vi.useFakeTimers();
    try {
      await wrapper.get('[aria-label="Replay time"]').setValue(12);
      await wrapper.get('[aria-label="Play replay"]').trigger('click');
      await vi.advanceTimersByTimeAsync(200);
      const position = Number(
        (wrapper.get('[aria-label="Replay time"]').element as HTMLInputElement).value,
      );
      expect(position).toBeGreaterThan(12);
      for (const tab of ['overview', 'chat', 'tacticalAid'] as const) {
        await wrapper.setProps({ tab });
        await vi.advanceTimersByTimeAsync(1000);
        await wrapper.setProps({ tab: 'replay' });
        expect(
          Number((wrapper.get('[aria-label="Replay time"]').element as HTMLInputElement).value),
        ).toBe(position);
        expect(wrapper.find('[aria-label="Play replay"]').exists()).toBe(true);
      }
      expect(mocks.getReplayPlayback).toHaveBeenCalledTimes(1);
      vi.useRealTimers();
      await wrapper.setProps({
        summary: replaySummary({ path: '/other.wicdemo' }),
        detail: replayDetail(),
      });
      await flushPromises();
      expect(mocks.getReplayPlayback).toHaveBeenLastCalledWith('/other.wicdemo');
      expect((wrapper.get('[aria-label="Replay time"]').element as HTMLInputElement).value).toBe(
        '0',
      );
      expect(wrapper.find('[aria-label="Play replay"]').exists()).toBe(true);
    } finally {
      wrapper.unmount();
      vi.useRealTimers();
    }
  });

  it('swallows optional tactical-objective errors', async () => {
    mocks.getReplayObjectives.mockRejectedValue(new Error('optional'));
    const wrapper = mountDetail();
    await wrapper.setProps({ tab: 'tacticalAid' });
    await flushPromises();
    expect(wrapper.text()).toContain('Deployment events');
  });
});
