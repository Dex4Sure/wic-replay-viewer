// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it, vi } from 'vitest';

import { mapArtAsset, playbackView, replayDetail, replaySummary } from '../test/factories';
import type { PlayerView } from '../types';
import ControlMeter from './ControlMeter.vue';
import MapTile from './MapTile.vue';
import OverviewTab from './OverviewTab.vue';
import ReplayLibrary from './ReplayLibrary.vue';
import ReplayMap from './ReplayMap.vue';
import TacticalAidMap from './TacticalAidMap.vue';

vi.mock('../mapArt', () => ({
  mapArtFor: vi.fn(() => null),
  requestMapArt: vi.fn(),
}));

const virtualListStub = {
  props: ['items'],
  template:
    '<div><template v-for="(item, index) in items"><slot :item="item" :index="index" /></template></div>',
  methods: { scrollToIndex: vi.fn() },
};

function player(overrides: Partial<PlayerView>): PlayerView {
  return {
    id: 1,
    name: 'Alice',
    team: 1,
    faction: 'USA',
    role: 'air',
    score: 100,
    scoreInfantry: 0,
    scoreSupport: 0,
    scoreArmor: 0,
    scoreAir: 100,
    scoreCapturing: 20,
    scoreFortification: 0,
    scoreTransportation: 0,
    scoreRepair: 0,
    scoreBridgeLaying: 0,
    scoreUnitDamage: 50,
    scoreTacticalAid: 10,
    scoreTotal: 180,
    ...overrides,
  };
}

it('renders accessible domination control semantics', () => {
  const wrapper = mount(ControlMeter, {
    props: {
      title: 'Final control',
      context: 'Timer expired',
      note: 'Partial',
      segments: [
        { faction: 'USA', pct: 0.6, width: 60, label: '60' },
        { faction: 'USSR', pct: 0.4, width: 40, label: '40' },
      ],
    },
  });
  expect(wrapper.get('[aria-label="Final control"]').text()).toContain('USA60%');
  expect(wrapper.text()).toContain('Partial');
});

it('renders the procedural map tile and follows prop changes', async () => {
  const wrapper = mount(MapTile, {
    props: { mapName: 'maps/ustown4/ustown4.ice', displayName: 'Seaside', faction: 'USA' },
  });
  expect(wrapper.get('.map-tile').classes()).toContain('map-tile-allied');
  expect(wrapper.get('.map-tile-initials').text()).toBe('SE');
  await wrapper.setProps({ displayName: '', faction: 'USSR' });
  expect(wrapper.get('.map-tile').classes()).toContain('map-tile-soviet');
});

it('renders overview teams, ranks, leaders, facts, and spectator grouping', () => {
  const detail = replayDetail();
  detail.overview.players = [
    player({ id: 1, name: 'Alice' }),
    player({
      id: 2,
      name: 'Boris',
      team: 3,
      faction: 'USSR',
      role: 'armor',
      score: 90,
      scoreArmor: 90,
      scoreTotal: 150,
    }),
    player({
      id: 3,
      name: 'Observer',
      team: 0,
      faction: 'Spectator',
      role: 'support',
      score: 8888,
      scoreTotal: 8888,
    }),
    player({
      id: 4,
      name: 'Unassigned scorer',
      team: null,
      faction: null,
      score: 9999,
      scoreTotal: 9999,
      scoreInfantry: 9999,
      scoreTacticalAid: 9999,
    }),
  ];
  const wrapper = mount(OverviewTab, { props: { detail }, global: { stubs: { ControlMeter } } });
  expect(wrapper.text()).toContain('Alice');
  expect(wrapper.text()).toContain('Boris');
  expect(wrapper.text()).toContain('Observer');
  expect(wrapper.findAll('.team-card')).toHaveLength(2);
  expect(wrapper.get('.spectator-roster').text()).toContain('Unassigned scorer');
  expect(wrapper.text()).not.toContain('Team unknown');
  expect(wrapper.get('.spectator-roster').text()).toBe('SpectatorsUnassigned scorerObserver');
  expect(wrapper.get('.spectator-roster').findAll('img, .player-score, .role-badge')).toHaveLength(
    0,
  );
  expect(wrapper.text()).not.toContain('9,999');
  expect(wrapper.text()).not.toContain('8,888');
  for (const row of wrapper.findAll('.match-leader-row')) {
    expect(row.text()).not.toContain('Unassigned scorer');
    expect(row.text()).not.toContain('Observer');
  }
  expect(wrapper.text()).toContain('Match leaders');
  expect(wrapper.text()).toContain('Seaside');
});

describe('ReplayLibrary', () => {
  function libraryProps() {
    return {
      rows: [
        replaySummary({ path: '/a/one.wicdemo' }),
        replaySummary({ path: '/a/two.wicdemo', fileName: 'two.wicdemo' }),
      ],
      locations: ['/a'],
      importing: false,
      cancellingImport: false,
      pendingScanRoots: [] as string[],
      selectedPath: null,
      exportSelection: new Set<string>(),
      search: '',
      mapArt: null,
      stats: { total: 2, parsed: 2, failed: 0, stale: 0 },
    };
  }

  it('groups, collapses, range-selects, searches, and emits scan controls', async () => {
    const wrapper = mount(ReplayLibrary, {
      props: libraryProps(),
      global: { stubs: { VirtualList: virtualListStub, MapTile: true } },
    });
    await wrapper.findAll('.locations-toggle')[1]!.trigger('click');
    expect(wrapper.text()).toContain('Checking…');
    const headings = wrapper.findAll('.replay-folder-heading');
    await headings[0]!.trigger('click');
    expect(wrapper.findAll('.replay-card')).toHaveLength(0);
    await headings[0]!.trigger('click');
    const cards = wrapper.findAll('.replay-card');
    await cards[0]!.trigger('click');
    await cards[1]!.trigger('click', { shiftKey: true });
    const selections = wrapper.emitted('update-export-selection')!;
    expect([...(selections.at(-1)![0] as Set<string>)]).toEqual([
      '/a/one.wicdemo',
      '/a/two.wicdemo',
    ]);
    await wrapper.get('input[type="search"]').setValue('query');
    expect(wrapper.emitted('update:search')?.at(-1)).toEqual(['query']);
    await wrapper.get('.library-scan-button').trigger('click');
    expect(wrapper.emitted('scan')).toBeTruthy();
  });

  const scanStates: Array<[Partial<ReturnType<typeof libraryProps>>, string, string]> = [
    [{ importing: true }, 'Cancel scan', 'cancel-scan'],
    [{ cancellingImport: true, importing: true }, 'Stopping…', 'cancel-scan'],
    [{ pendingScanRoots: ['/new'] }, 'Scan new folder', 'scan'],
    [{ stats: { total: 2, parsed: 1, failed: 0, stale: 1 } }, 'Refresh library', 'scan'],
  ];

  it.each(scanStates)('presents each scan state', async (overrides, label, emitted) => {
    const wrapper = mount(ReplayLibrary, {
      props: { ...libraryProps(), ...overrides },
      global: { stubs: { VirtualList: virtualListStub, MapTile: true } },
    });
    expect(wrapper.text()).toContain(label);
    if (!(overrides as { cancellingImport?: boolean }).cancellingImport) {
      await wrapper.get('.library-scan-button').trigger('click');
      expect(wrapper.emitted(emitted)).toBeTruthy();
    }
  });

  it('emits location, map-art, export, and clear-search actions', async () => {
    const wrapper = mount(ReplayLibrary, {
      props: {
        ...libraryProps(),
        search: 'Alice',
        exportSelection: new Set(['/a/one.wicdemo']),
        mapArt: {
          installPath: '/game',
          customMapsPath: '/maps',
          mapCount: 2,
          problem: null,
          cacheChanged: false,
        },
      },
      global: { stubs: { VirtualList: virtualListStub, MapTile: true } },
    });
    await wrapper.get('[aria-label="Clear search"]').trigger('click');
    await wrapper.get('.library-selection-actions button').trigger('click');
    await wrapper.findAll('.library-selection-actions button')[1]!.trigger('click');
    await wrapper.findAll('.locations-toggle')[1]!.trigger('click');
    expect(wrapper.text()).toContain('2 map images loaded');
    await wrapper.findAll('.location-remove')[0]!.trigger('click');
    await wrapper.findAll('.location-remove')[1]!.trigger('click');
    await wrapper.findAll('.location-remove')[2]!.trigger('click');
    await wrapper.findAll('.map-art-actions button')[0]!.trigger('click');
    await wrapper.findAll('.map-art-actions button')[1]!.trigger('click');
    await wrapper.findAll('.map-art-actions button').at(-1)!.trigger('click');
    expect(wrapper.emitted('update:search')).toContainEqual(['']);
    expect(wrapper.emitted('export-selected')).toBeTruthy();
    expect(wrapper.emitted('remove-location')).toEqual([['/a']]);
    expect(wrapper.emitted('clear-art-source')).toEqual([['install'], ['customMaps']]);
    expect(wrapper.emitted('rescan-map-art')).toBeTruthy();
    await (wrapper.vm as unknown as { focusReplay: (path: string) => Promise<void> }).focusReplay(
      '/a/one.wicdemo',
    );
    await (wrapper.vm as unknown as { focusReplay: (path: string) => Promise<void> }).focusReplay(
      '/missing',
    );
  });
});

it('plots tactical aid without filtering non-highlighted events', async () => {
  const rows = [
    {
      timeSeconds: 1,
      support: 'Nuclear Strike',
      player: 'Alice',
      faction: 'USA',
      playerAttribution: 'Exact player',
      honorsCost: 80,
      position: [100, 0, 100] as [number, number, number],
    },
    {
      timeSeconds: 2,
      support: 'Heavy Artillery',
      player: 'Boris',
      faction: 'USSR',
      playerAttribution: 'Team only',
      honorsCost: null,
      position: [200, 0, 200] as [number, number, number],
    },
  ];
  const wrapper = mount(TacticalAidMap, {
    props: {
      asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 1000 } }),
      rows,
      objectives: [],
      mapName: 'Seaside',
      selectedIndex: null,
      highlightedCategory: { faction: 'USA', support: 'Nuclear Strike' },
    },
  });
  expect(wrapper.findAll('.aid-map-marker')).toHaveLength(2);
  expect(wrapper.findAll('.aid-map-marker-dimmed')).toHaveLength(1);
  await wrapper.findAll('.aid-map-marker')[0]!.trigger('click');
  expect(wrapper.emitted('select')).toEqual([[0]]);
  await wrapper.setProps({ selectedIndex: 0 });
  await wrapper.findAll('.aid-map-marker')[0]!.trigger('click');
  expect(wrapper.emitted('select')?.at(-1)).toEqual([0]);
});

it.each(['explosion', 'napalm', 'chemical'] as const)(
  'renders a recorded %s footprint at its own radius and time',
  async (kind) => {
    const wrapper = mount(ReplayMap, {
      props: {
        asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 500 } }),
        playback: playbackView({
          areaEffects: [
            {
              timeSeconds: 10,
              position: [500, 0, 250],
              radius: 50,
              durationSeconds: kind === 'explosion' ? 1.5 : 25,
              kind,
            },
          ],
        }),
        tacticalAidRows: [],
        mapName: 'Seaside',
        modelValue: 9,
      },
    });
    expect(wrapper.find('.replay-area-effect').exists()).toBe(false);
    await wrapper.setProps({ modelValue: 10 });
    expect(wrapper.get(`.replay-area-${kind}`).attributes('style')).toContain('width: 10%');
    expect(wrapper.get(`.replay-area-${kind}`).attributes('style')).toContain('height: 20%');
    expect(wrapper.find('.replay-area-wave').exists()).toBe(kind === 'explosion');
    await wrapper.setProps({ modelValue: 35 });
    expect(wrapper.find('.replay-area-effect').exists()).toBe(false);
    wrapper.unmount();
  },
);

it('animates recorded nuclear effects independently of TA countdowns and seeking', async () => {
  const wrapper = mount(ReplayMap, {
    props: {
      asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 1000 } }),
      playback: playbackView({ nuclearEffects: [{ timeSeconds: 20, position: [500, 0, 500] }] }),
      tacticalAidRows: [],
      mapName: 'Seaside',
      modelValue: 19.99,
    },
  });
  expect(wrapper.find('.replay-nuclear-effect').exists()).toBe(false);
  await wrapper.setProps({ modelValue: 20 });
  expect(wrapper.get('.replay-nuclear-effect').attributes('style')).toContain('width: 44%');
  expect(wrapper.get('.replay-nuclear-flash').attributes('style')).toContain('opacity: 0.8');
  await wrapper.setProps({ modelValue: 21 });
  const wave = wrapper.get('.replay-nuclear-wave').attributes('style');
  expect(wrapper.get('.replay-nuclear-flash').attributes('style')).toContain('opacity: 0');
  await wrapper.setProps({ modelValue: 24 });
  expect(wrapper.find('.replay-nuclear-effect').exists()).toBe(false);
  await wrapper.setProps({ modelValue: 21 });
  expect(wrapper.get('.replay-nuclear-wave').attributes('style')).toBe(wave);
  wrapper.unmount();
});

it.each(['Airborne Infantry', 'Airdropped Light Tank', 'Airdropped Transport'])(
  'keeps %s separate from other TA and updates grouping when the support changes',
  async (support) => {
    const tacticalAidRows = [support, 'Airstrike', 'Aerial Recon', 'Repair Bridge'].map((name) => ({
      timeSeconds: 0,
      support: name,
      faction: 'USA',
      player: 'Alice',
      playerAttribution: 'Exact player',
      honorsCost: null,
      position: [500, 0, 500] as [number, number, number],
    }));
    const wrapper = mount(ReplayMap, {
      props: {
        asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 1000 } }),
        playback: playbackView(),
        tacticalAidRows,
        mapName: 'Seaside',
        modelValue: 1,
      },
    });
    expect(wrapper.get('.replay-effect-name').text()).toBe(support);
    expect(wrapper.get('.replay-ta-stack').text()).not.toContain(support);
    expect(wrapper.get('.replay-ta-stack').findAll('li')).toHaveLength(3);
    await wrapper.setProps({
      tacticalAidRows: tacticalAidRows.map((row, i) =>
        i === 0 ? { ...row, support: 'Airstrike' } : row,
      ),
    });
    expect(wrapper.find('.replay-effect').exists()).toBe(false);
    expect(wrapper.get('.replay-ta-stack').findAll('li')).toHaveLength(4);
    wrapper.unmount();
  },
);

it('automatically stacks crowded TAs with faction rows and updates timers while seeking', async () => {
  const wrapper = mount(ReplayMap, {
    props: {
      asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 1000 } }),
      playback: playbackView(),
      tacticalAidRows: Array.from({ length: 30 }, (_, index) => ({
        timeSeconds: 0,
        support: 'Aerial Recon',
        faction: ['USA', 'NATO', 'USSR'][index % 3]!,
        player: 'Alice',
        playerAttribution: 'Exact player',
        honorsCost: null,
        position: [500, 0, 500] as [number, number, number],
      })),
      mapName: 'Seaside',
      modelValue: 1,
    },
  });
  expect(wrapper.findAll('.replay-ta-anchor')).toHaveLength(30);
  expect(wrapper.findAll('.replay-ta-connectors line')).toHaveLength(30);
  const stack = wrapper.get('.replay-ta-stack');
  expect(wrapper.findAll('.replay-ta-card')).toHaveLength(30);
  expect(wrapper.findAll('.replay-ta-card.faction-tone-allied')).toHaveLength(20);
  expect(wrapper.findAll('.replay-ta-card.faction-tone-soviet')).toHaveLength(10);
  const initialPositions = wrapper.findAll('.replay-ta-stack').map((s) => s.attributes('style'));
  const rows = stack.findAll('li');
  await rows[0]!.trigger('focus');
  expect(rows[0]!.classes()).toContain('replay-ta-card-active');
  expect(wrapper.findAll('.replay-ta-line-active')).toHaveLength(1);
  expect(wrapper.findAll('.replay-ta-anchor-active')).toHaveLength(1);
  expect(wrapper.findAll('.replay-ta-line-dim')).toHaveLength(rows.length - 1);
  await rows[1]!.trigger('mouseenter');
  expect(rows[1]!.classes()).toContain('replay-ta-card-active');
  expect(rows[0]!.classes()).not.toContain('replay-ta-card-active');
  await rows[1]!.trigger('mouseleave');
  expect(rows[0]!.classes()).toContain('replay-ta-card-active');
  await rows[0]!.trigger('blur');
  expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
  await rows[1]!.trigger('mouseenter');
  await stack.trigger('scroll');
  expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
  await rows[0]!.trigger('focus');
  expect(stack.text()).toContain('Alice');
  expect(stack.text()).toContain('14s');
  expect(wrapper.text()).not.toContain('active TAs');
  await wrapper.get('section').trigger('mouseleave');
  expect(wrapper.find('.replay-ta-stack').exists()).toBe(true);
  await wrapper.setProps({ modelValue: 5 });
  expect(wrapper.get('.replay-ta-stack').text()).toContain('10s');
  expect(wrapper.findAll('.replay-ta-stack').map((s) => s.attributes('style'))).toEqual(
    initialPositions,
  );
  await wrapper.setProps({ modelValue: 15 });
  expect(wrapper.find('.replay-ta-stack').exists()).toBe(false);
  await wrapper.setProps({ modelValue: 1 });
  expect(wrapper.findAll('.replay-ta-anchor')).toHaveLength(30);
  expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
  await wrapper.get('.replay-ta-stack li').trigger('focus');
  await wrapper.setProps({ playback: playbackView() });
  expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
  wrapper.unmount();
});

it.each([
  [null, 'Map art is unavailable'],
  [mapArtAsset({ bounds: null }), 'no verified terrain bounds'],
] as const)('explains why tactical aid cannot be plotted', (asset, copy) => {
  const wrapper = mount(TacticalAidMap, {
    props: {
      asset,
      rows: [],
      objectives: [],
      mapName: 'Map',
      selectedIndex: null,
      highlightedCategory: null,
    },
  });
  expect(wrapper.text()).toContain(copy);
});

it('supports playback visibility and keyboard speed selection', async () => {
  const playback = playbackView({
    objectives: [{ kind: 'perimeterPoint', id: 2, parentId: 1, position: [300, 0, 300], team: 1 }],
    objectiveChanges: [{ timeSeconds: 1, kind: 'perimeterPoint', id: 2, team: 3 }],
    units: [
      {
        unitId: 1,
        generation: 0,
        createdSeconds: 0,
        spawnPosition: [100, 0, 100],
        team: 1,
        unitTypeId: 1,
        isInfantryMember: false,
        initialHealth: 1,
        frames: [],
        health: [],
        teams: [],
        terminal: null,
      },
      {
        unitId: 2,
        generation: 0,
        createdSeconds: 0,
        spawnPosition: [200, 0, 200],
        team: 3,
        unitTypeId: 2,
        isInfantryMember: false,
        initialHealth: 1,
        frames: [{ timeSeconds: 0.5, position: [210, 0, 210] }],
        health: [],
        teams: [],
        terminal: { timeSeconds: 1, kind: 'destroyed' },
      },
    ],
  });
  const wrapper = mount(ReplayMap, {
    props: {
      asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 1000 } }),
      playback,
      tacticalAidRows: [
        {
          timeSeconds: 1,
          support: 'Nuclear Strike',
          player: 'Alice',
          faction: 'USA',
          playerAttribution: 'Exact player',
          honorsCost: 80,
          position: [100, 0, 100],
        },
      ],
      mapName: 'Seaside',
      modelValue: 1.2,
    },
  });
  expect(wrapper.findAll('.replay-unit')).toHaveLength(1);
  expect(wrapper.get('.replay-unit').classes()).toContain('faction-tone-allied');
  expect(wrapper.findAll('.replay-objective')).toHaveLength(1);
  expect(wrapper.get('.replay-objective').classes()).toContain('faction-tone-soviet');
  expect(wrapper.findAll('.replay-death')).toHaveLength(1);
  expect(wrapper.findAll('.replay-effect')).toHaveLength(1);
  const trigger = wrapper.get('.replay-speed-trigger');
  await trigger.trigger('click');
  expect(wrapper.find('[role="menu"]').exists()).toBe(true);
  await trigger.trigger('click');
  await trigger.trigger('keydown', { key: 'ArrowDown' });
  const menu = wrapper.get('[role="menu"]');
  await menu.trigger('keydown', { key: 'End' });
  await menu.trigger('keydown', { key: 'Home' });
  await menu.trigger('keydown', { key: 'ArrowUp' });
  await wrapper.findAll('[role="menuitemradio"]').at(-1)!.trigger('click');
  expect(trigger.text()).toContain('4×');
  await wrapper.get('input[type="range"]').setValue(5);
  expect(wrapper.emitted('update:modelValue')).toBeTruthy();
  await wrapper.get('.replay-play-button').trigger('click');
  expect(wrapper.get('.replay-play-button').attributes('aria-label')).toBe('Pause replay');
  await wrapper.get('.replay-play-button').trigger('click');
  await trigger.trigger('click');
  document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
  await trigger.trigger('click');
  await wrapper.get('.replay-speed-control').trigger('focusout', { relatedTarget: document.body });
  wrapper.unmount();
});

it.each([
  ['Airborne Infantry', 35, 'USA'],
  ['Airborne Infantry', 35, 'NATO'],
  ['Airborne Infantry', 35, 'USSR'],
  ['Airdropped Light Tank', 35, 'USA'],
  ['Airdropped Light Tank', 35, 'NATO'],
  ['Airdropped Light Tank', 35, 'USSR'],
  ['Airdropped Transport', 35, 'USA'],
  ['Airdropped Transport', 35, 'NATO'],
  ['Airdropped Transport', 35, 'USSR'],
  ['Aerial Recon', 15, 'USA'],
  ['Aerial Recon', 15, 'NATO'],
  ['Aerial Recon', 15, 'USSR'],
  ['Nuclear Strike', 16, 'USA'],
  ['Nuclear Strike', 16, 'NATO'],
  ['Nuclear Strike', 16, 'USSR'],
  ['Carpet Bombing', 15, 'USA'],
  ['Carpet Bombing', 15, 'NATO'],
  ['Carpet Bombing', 15, 'USSR'],
  ['Repair Bridge', 15, 'USA'],
  ['Repair Bridge', 15, 'NATO'],
  ['Repair Bridge', 15, 'USSR'],
  ['Napalm Strike', 20, 'USA'],
  ['Napalm Strike', 20, 'NATO'],
  ['Napalm Strike', 20, 'USSR'],
  ['Tank Buster', 12, 'USA'],
  ['Tank Buster', 12, 'NATO'],
  ['Tank Buster', 12, 'USSR'],
  ['Laser Guided Bomb', 13, 'USA'],
  ['Laser Guided Bomb', 13, 'NATO'],
  ['Laser Guided Bomb', 11, 'USSR'],
  ['Air-to-Air Strike', 12, 'USA'],
  ['Air-to-Air Strike', 12, 'NATO'],
  ['Air-to-Air Strike', 12, 'USSR'],
  ['Chemical Strike', 15, 'USA'],
  ['Chemical Strike', 15, 'NATO'],
  ['Chemical Strike', 15, 'USSR'],
  ['Heavy Air Support', 15, 'USA'],
  ['Heavy Air Support', 15, 'NATO'],
  ['Heavy Air Support', 15, 'USSR'],
  ['Light Artillery Barrage', 10, 'USA'],
  ['Light Artillery Barrage', 10, 'NATO'],
  ['Light Artillery Barrage', 10, 'USSR'],
  ['Precision Artillery', 10, 'USA'],
  ['Precision Artillery', 10, 'NATO'],
  ['Precision Artillery', 10, 'USSR'],
  ['Heavy Artillery Barrage', 12, 'USA'],
  ['Heavy Artillery Barrage', 12, 'NATO'],
  ['Heavy Artillery Barrage', 12, 'USSR'],
  ['Airstrike', 20, 'USA'],
  ['Airstrike', 20, 'NATO'],
  ['Airstrike', 20, 'USSR'],
  ['Daisy Cutter Bomb', 18, 'USA'],
  ['Daisy Cutter Bomb', 18, 'NATO'],
  ['Daisy Cutter Bomb', 18, 'USSR'],
  ['Fuel Air Bomb', 18, 'USA'],
  ['Fuel Air Bomb', 18, 'NATO'],
  ['Fuel Air Bomb', 18, 'USSR'],
] as const)(
  'counts down %s for %i replay seconds without inventing a unit spawn',
  async (support, duration, faction) => {
    const wrapper = mount(ReplayMap, {
      props: {
        asset: mapArtAsset({ bounds: { minX: 0, minZ: 0, maxX: 1000, maxZ: 1000 } }),
        playback: playbackView({
          units: [
            {
              unitId: 1,
              generation: 0,
              createdSeconds: 46,
              spawnPosition: [100, 0, 100],
              team: 1,
              unitTypeId: null,
              isInfantryMember: false,
              initialHealth: 1,
              frames: [],
              health: [],
              teams: [],
              terminal: null,
            },
          ],
        }),
        tacticalAidRows: [
          {
            timeSeconds: 10,
            support,
            player: 'Alice',
            faction,
            playerAttribution: 'Exact player',
            honorsCost: null,
            position: [100, 0, 100],
          },
        ],
        mapName: 'Seaside',
        modelValue: 9.99,
      },
    });
    expect(wrapper.find('.replay-effect').exists()).toBe(false);
    await wrapper.setProps({ modelValue: 10 });
    expect(wrapper.get('.replay-effect-countdown').text()).toBe(`${duration}s`);
    expect(wrapper.get('.replay-effect').attributes('aria-label')).toBe(
      `${support}, Alice, ${duration} seconds remaining`,
    );
    expect(wrapper.get('.replay-effect-name').text()).toBe(support);
    const card = wrapper.get('.replay-effect');
    await card.trigger('mouseenter');
    expect(wrapper.findAll('.replay-ta-line-active')).toHaveLength(1);
    expect(wrapper.findAll('.replay-ta-anchor-active')).toHaveLength(1);
    expect(card.classes()).toContain('replay-ta-card-active');
    await card.trigger('mouseleave');
    expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
    await card.trigger('focus');
    expect(wrapper.find('.replay-ta-line-active').exists()).toBe(true);
    await card.trigger('blur');
    expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
    await card.trigger('mouseenter');
    expect(wrapper.get('.replay-effect .replay-ta-player').text()).toBe('Alice');
    expect(wrapper.find('.replay-effect svg').exists()).toBe(false);
    await wrapper.setProps({ modelValue: 17 });
    expect(wrapper.get('.replay-effect-countdown').text()).toBe(`${duration - 7}s`);
    expect(wrapper.get('.replay-effect').attributes('style')).toContain('opacity: 1;');
    await wrapper.setProps({ modelValue: 10 + duration - 0.01 });
    expect(wrapper.get('.replay-effect-countdown').text()).toBe('1s');
    await wrapper.setProps({ modelValue: 10 + duration });
    expect(wrapper.find('.replay-effect').exists()).toBe(false);
    expect(wrapper.find('.replay-ta-line-active').exists()).toBe(false);
    expect(wrapper.find('.replay-unit').exists()).toBe(false);
    await wrapper.setProps({ modelValue: 46 });
    expect(wrapper.findAll('.replay-unit')).toHaveLength(1);
    await wrapper.setProps({ modelValue: 15 });
    expect(wrapper.get('.replay-effect-countdown').text()).toBe(`${duration - 5}s`);
    expect(wrapper.find('.replay-unit').exists()).toBe(false);
    wrapper.unmount();
  },
);

it.each([
  [null, 'Map art is unavailable'],
  [mapArtAsset({ bounds: null }), 'no verified terrain bounds'],
] as const)('explains why playback cannot be plotted', async (asset, copy) => {
  const wrapper = mount(ReplayMap, {
    props: {
      asset,
      playback: playbackView(),
      tacticalAidRows: [],
      mapName: 'Map',
      modelValue: 0,
    },
  });
  expect(wrapper.text()).toContain(copy);
  await wrapper.get('input[aria-label="Replay time"]').setValue(5);
  expect(wrapper.emitted('update:modelValue')?.at(-1)).toEqual([5]);
  expect(wrapper.find('[aria-label="Play replay"]').exists()).toBe(true);
  wrapper.unmount();
});

it('marks departed scores without assigning event-derived roles', () => {
  const detail = replayDetail();
  detail.overview.players = [
    player({
      id: 3,
      name: 'Departed',
      team: 3,
      faction: 'USSR',
      score: 150,
      scoreTotal: null,
      role: null,
      scoreBeforeLeave: {
        score: 150,
        observedAtSeconds: 40,
        leftAtSeconds: 41,
      },
    }),
    player({ id: 6, name: 'HOTWINGS', team: 3, faction: 'USSR', score: 0, role: null }),
    player({ id: 7, name: 'Scored', team: 3, faction: 'USSR', score: 100, role: 'air' }),
  ];
  const wrapper = mount(OverviewTab, { props: { detail }, global: { stubs: { ControlMeter } } });
  expect(wrapper.find('.spectator-roster').exists()).toBe(false);
  expect(wrapper.get('.player-score').text()).toBe('150*');
  expect(wrapper.get('.player-score sup').attributes('aria-label')).toBe('Score before leaving');
  expect(wrapper.get('.player-score').attributes('title')).toContain(
    'Last recorded score before leaving',
  );
  expect(wrapper.text()).not.toContain('Final statistics unavailable');
  expect(wrapper.findAll('.player-row img[alt="air"]')).toHaveLength(1);
  expect(wrapper.get('.player-row img[alt="air"]').attributes('title')).toBe('air');
  expect(wrapper.text()).not.toContain(
    'Roles for these players show their last recorded selection',
  );
});

it('displays negative scores below zero and positive scores', () => {
  const detail = replayDetail();
  detail.overview.players = [
    player({
      id: 4,
      name: 'Negative',
      team: 1,
      faction: 'USA',
      score: -11,
      scoreAir: -12,
      scoreTotal: -12,
    }),
    player({ id: 2, name: 'Zero', team: 1, faction: 'USA', score: 0 }),
    player({ id: 3, name: 'Positive', team: 1, faction: 'USA', score: 28 }),
  ];
  const wrapper = mount(OverviewTab, { props: { detail }, global: { stubs: { ControlMeter } } });
  expect(wrapper.findAll('.player-score').map((row) => row.text())).toEqual(['28', '0', '-11']);
  expect(wrapper.text()).not.toContain('4,294,967');
});

it.each(['USA', 'NATO', 'USSR'])(
  'retains an empty opposing card when only %s has players',
  (faction) => {
    const detail = replayDetail();
    detail.overview.winner = faction;
    detail.overview.dominationShares = null;
    detail.overview.players = [
      player({ faction, team: faction === 'USSR' ? 3 : faction === 'NATO' ? 2 : 1 }),
    ];
    detail.timelineRows = [
      {
        timeSeconds: 1,
        kind: 'team',
        description: '',
        players: [{ name: 'Departed', faction: 'NATO' }],
        commandPointId: null,
        commandPointTeam: null,
      },
    ];
    const wrapper = mount(OverviewTab, { props: { detail } });
    const cards = wrapper.findAll('.team-card');
    expect(cards).toHaveLength(2);
    const empty = cards.find((card) => card.findAll('.player-row').length === 0)!;
    expect(empty.get('h4').text()).toBe(faction === 'USSR' ? 'NATO' : 'USSR');
    expect(empty.get('header > strong').text()).toBe('0');
    expect(empty.findAll('.player-row')).toHaveLength(0);
    expect(wrapper.findAll('.team-card-winner')).toHaveLength(1);
    expect(wrapper.findAll('.player-row')).toHaveLength(1);
    expect(wrapper.text()).not.toContain('Team unknown');
  },
);

it('uses recorded faction evidence for empty teams without creating extra teams', () => {
  const detail = replayDetail();
  detail.overview.winner = 'USSR';
  detail.overview.dominationShares = null;
  detail.overview.players = [];
  detail.timelineRows = [
    {
      timeSeconds: 1,
      kind: 'team',
      description: '',
      players: [{ name: 'Departed', faction: 'NATO' }],
      commandPointId: null,
      commandPointTeam: null,
    },
  ];
  const wrapper = mount(OverviewTab, { props: { detail } });
  expect(wrapper.findAll('.team-card h4').map((heading) => heading.text())).toEqual([
    'NATO',
    'USSR',
  ]);
  expect(wrapper.findAll('.player-row')).toHaveLength(0);
});
