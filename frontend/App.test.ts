// @vitest-environment happy-dom

import { shallowMount, flushPromises, enableAutoUnmount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import packageMetadata from '../package.json';

import { replayDetail, replaySummary } from './test/factories';

const mocks = vi.hoisted(() => ({
  addReplayFolders: vi.fn(),
  cancelReplayImport: vi.fn(),
  changeReplayInGameName: vi.fn(),
  chooseFolder: vi.fn(),
  chooseReplayExport: vi.fn(),
  chooseReplayFolders: vi.fn(),
  clearArtSource: vi.fn(),
  exportReplayCopies: vi.fn(),
  exportReplayCopy: vi.fn(),
  getVersion: vi.fn(),
  getInitialState: vi.fn(),
  getMapArtState: vi.fn(),
  getReplayDetail: vi.fn(),
  importReplayFolders: vi.fn(),
  isImportRunning: vi.fn(async () => true),
  removeReplayFolder: vi.fn(),
  renameReplayFile: vi.fn(),
  rescanMapArt: vi.fn(),
  setArtSource: vi.fn(),
  importHandler: null as null | ((event: unknown) => void),
  dragHandler: null as null | ((event: { payload: { type: string; paths: string[] } }) => void),
}));

vi.mock('@tauri-apps/api/app', () => ({ getVersion: mocks.getVersion }));
vi.mock('./api', () => ({
  ...mocks,
  listenForImportEvents: vi.fn(async (handler) => {
    mocks.importHandler = handler;
    return vi.fn();
  }),
}));
vi.mock('@tauri-apps/api/path', () => ({ documentDir: vi.fn(async () => '/documents') }));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn(async (handler) => {
      mocks.dragHandler = handler;
      return vi.fn();
    }),
  }),
}));
vi.mock('./mapArt', () => ({ resetMapArt: vi.fn(), setMapArtAvailable: vi.fn() }));

import App from './App.vue';
import ReplayDetail from './components/ReplayDetail.vue';
import ReplayLibrary from './components/ReplayLibrary.vue';
import ReplayManagementDialog from './components/ReplayManagementDialog.vue';

const artState = {
  installPath: null,
  customMapsPath: null,
  mapCount: 0,
  problem: null,
  cacheChanged: false,
};
const replayLibraryStub = {
  props: [
    'rows',
    'locations',
    'importing',
    'cancellingImport',
    'pendingScanRoots',
    'selectedPath',
    'exportSelection',
    'search',
    'mapArt',
    'stats',
  ],
  emits: [
    'select',
    'update-export-selection',
    'clear-export-selection',
    'export-selected',
    'add-locations',
    'scan',
    'cancel-scan',
    'remove-location',
    'update:search',
    'choose-art-source',
    'clear-art-source',
    'rescan-map-art',
  ],
  template: '<div />',
  methods: { focusReplay: vi.fn() },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

async function mountApp(rows = [replaySummary()]) {
  mocks.getInitialState.mockResolvedValue({
    summaries: rows,
    locations: ['/replays'],
    importing: false,
    cachedMapArtCount: 0,
  });
  mocks.getMapArtState.mockResolvedValue(artState);
  const wrapper = shallowMount(App, {
    global: {
      stubs: {
        ReplayLibrary: replayLibraryStub,
      },
    },
  });
  await flushPromises();
  return wrapper;
}

enableAutoUnmount(afterEach);
afterEach(() => vi.useRealTimers());

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getVersion.mockResolvedValue(packageMetadata.version);
  mocks.isImportRunning.mockResolvedValue(true);
  localStorage.clear();
  mocks.importHandler = null;
  mocks.dragHandler = null;
});

describe('App orchestration', () => {
  it('shows the packaged application version in the status bar', async () => {
    mocks.getVersion.mockResolvedValue('1.2.3');
    const wrapper = await mountApp();

    const version = wrapper.get('.status-version');
    expect(version.text()).toBe('v. 1.2.3');
    expect(version.attributes('title')).toBe('WiC Replay Viewer 1.2.3');
  });

  it('keeps the synchronized package version when the Tauri runtime is unavailable', async () => {
    mocks.getVersion.mockRejectedValue(new Error('not running in Tauri'));
    const wrapper = await mountApp();

    expect(wrapper.get('.status-version').text()).toBe(`v. ${packageMetadata.version}`);
  });

  it('loads the library before delayed map art and contains map-art failures', async () => {
    mocks.getInitialState.mockResolvedValue({
      summaries: [],
      locations: [],
      importing: false,
      cachedMapArtCount: 2,
    });
    mocks.getMapArtState.mockRejectedValue(new Error('archive unavailable'));
    const wrapper = shallowMount(App, {
      global: {
        stubs: {
          ReplayLibrary: replayLibraryStub,
        },
      },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Add folders or drop them into the window to begin');
    expect(wrapper.getComponent(ReplayLibrary).props('mapArt')).toMatchObject({
      mapCount: 2,
      problem: 'archive unavailable',
    });
  });

  it('handles complete, cancelled, failed, and stale import event sequences', async () => {
    const selected = replaySummary({ path: '/gone' });
    const wrapper = await mountApp([selected]);
    const library = wrapper.getComponent(ReplayLibrary);
    await library.vm.$emit('select', selected);
    mocks.importHandler!({
      type: 'importPrepared',
      payload: { discovered: 2, pending: 2, skipped: 0 },
    });
    mocks.importHandler!({ type: 'summariesStored', payload: [replaySummary({ path: '/new' })] });
    mocks.importHandler!({ type: 'importProblem', payload: 'one replay failed' });
    mocks.importHandler!({
      type: 'importFinished',
      payload: { imported: 1, failed: 1, skipped: 0, removedPaths: ['/gone'], cancelled: false },
    });
    await flushPromises();
    expect(library.props('rows')).toHaveLength(1);
    expect(wrapper.text()).toContain('1 missing removed');
    expect(wrapper.getComponent(ReplayDetail).props('summary')).toBeNull();

    mocks.importHandler!({
      type: 'importFinished',
      payload: { imported: 3, failed: 0, skipped: 0, cancelled: true },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('Scan cancelled: 3 parsed before stopping');
  });

  it('prevents an older detail request from overwriting a newer replay', async () => {
    const first = replaySummary({ path: '/first', fileName: 'first.wicdemo' });
    const second = replaySummary({ path: '/second', fileName: 'second.wicdemo' });
    const firstLoad = deferred<ReturnType<typeof replayDetail>>();
    const secondLoad = deferred<ReturnType<typeof replayDetail>>();
    mocks.getReplayDetail
      .mockReturnValueOnce(firstLoad.promise)
      .mockReturnValueOnce(secondLoad.promise);
    const wrapper = await mountApp([first, second]);
    const library = wrapper.getComponent(ReplayLibrary);
    await library.vm.$emit('select', first);
    await library.vm.$emit('select', second);
    const secondDetail = replayDetail({ recorderView: 'second' });
    secondLoad.resolve(secondDetail);
    await flushPromises();
    firstLoad.resolve(replayDetail({ recorderView: 'stale' }));
    await flushPromises();
    expect(wrapper.getComponent(ReplayDetail).props('detail')).toEqual(secondDetail);
  });

  it('coordinates add, scan, cancel reconciliation, remove, and drag-drop states', async () => {
    const row = replaySummary({ path: '/replays/example.wicdemo' });
    const wrapper = await mountApp([row]);
    const library = wrapper.getComponent(ReplayLibrary);
    mocks.chooseReplayFolders.mockResolvedValue(['/new']);
    mocks.addReplayFolders.mockResolvedValue(['/replays', '/new']);
    await library.vm.$emit('add-locations');
    await flushPromises();
    expect(library.props('pendingScanRoots')).toEqual(['/new']);

    mocks.importReplayFolders.mockResolvedValue(['/replays', '/new']);
    await library.vm.$emit('scan', ['/new']);
    await flushPromises();
    expect(library.props('importing')).toBe(true);
    mocks.cancelReplayImport.mockResolvedValue(false);
    mocks.getInitialState.mockResolvedValue({
      summaries: [row],
      locations: ['/replays'],
      importing: false,
      cachedMapArtCount: 0,
    });
    await library.vm.$emit('cancel-scan');
    await flushPromises();
    expect(wrapper.text()).toContain('Scan already finished; library refreshed');

    mocks.removeReplayFolder.mockResolvedValue({ locations: [], removedReplayPaths: [row.path] });
    await library.vm.$emit('remove-location', '/replays');
    await flushPromises();
    expect(library.props('rows')).toEqual([]);

    mocks.dragHandler!({ payload: { type: 'enter', paths: [] } });
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[role="status"]').exists()).toBe(true);
    mocks.dragHandler!({ payload: { type: 'drop', paths: ['/drop'] } });
    await flushPromises();
    expect(mocks.importReplayFolders).toHaveBeenCalledWith(['/drop']);
  });

  it('runs rename, in-game name, single export, and batch export workflows', async () => {
    const row = replaySummary();
    mocks.getReplayDetail.mockResolvedValue(replayDetail());
    const wrapper = await mountApp([row]);
    const library = wrapper.getComponent(ReplayLibrary);
    const detail = wrapper.getComponent(ReplayDetail);
    await library.vm.$emit('select', row);
    await flushPromises();

    const renamed = replaySummary({
      path: '/replays/renamed.wicdemo',
      fileName: 'renamed.wicdemo',
    });
    mocks.renameReplayFile.mockResolvedValue({ path: renamed.path, summary: renamed });
    await detail.vm.$emit('manage', 'rename');
    await wrapper.vm.$nextTick();
    let dialog = wrapper.getComponent(ReplayManagementDialog);
    await dialog.vm.$emit('submit', {
      fileName: 'renamed.wicdemo',
      replayName: null,
      newFolderName: null,
    });
    await flushPromises();
    expect(mocks.renameReplayFile).toHaveBeenCalledWith(row.path, 'renamed.wicdemo');

    await wrapper.getComponent(ReplayDetail).vm.$emit('manage', 'name');
    await wrapper.vm.$nextTick();
    dialog = wrapper.getComponent(ReplayManagementDialog);
    mocks.changeReplayInGameName.mockResolvedValue({
      path: renamed.path,
      summary: { ...renamed, replayName: 'Named' },
    });
    await dialog.vm.$emit('submit', {
      fileName: renamed.fileName,
      replayName: 'Named',
      newFolderName: null,
    });
    await flushPromises();
    expect(mocks.changeReplayInGameName).toHaveBeenCalledWith(renamed.path, 'Named');

    await library.vm.$emit('update-export-selection', new Set([renamed.path]));
    await library.vm.$emit('export-selected');
    await wrapper.vm.$nextTick();
    mocks.chooseReplayExport.mockResolvedValue('/exports/renamed.wicdemo');
    mocks.exportReplayCopy.mockResolvedValue({ path: '/exports/renamed.wicdemo', summary: null });
    await wrapper
      .getComponent(ReplayManagementDialog)
      .vm.$emit('submit', { fileName: renamed.fileName, replayName: null, newFolderName: null });
    await flushPromises();
    expect(mocks.exportReplayCopy).toHaveBeenCalled();
  });

  it('coordinates optional map art, resizing, focus return, and management cancellation', async () => {
    const row = replaySummary();
    mocks.getReplayDetail.mockResolvedValue(replayDetail());
    const wrapper = await mountApp([row]);
    const library = wrapper.getComponent(ReplayLibrary);

    mocks.chooseFolder.mockResolvedValueOnce(null).mockResolvedValueOnce('/game');
    await library.vm.$emit('choose-art-source', 'install');
    await flushPromises();
    expect(mocks.setArtSource).not.toHaveBeenCalled();
    mocks.setArtSource.mockResolvedValue({
      ...artState,
      installPath: '/game',
      mapCount: 3,
      cacheChanged: true,
    });
    await library.vm.$emit('choose-art-source', 'install');
    await flushPromises();
    expect(library.props('mapArt')).toMatchObject({ mapCount: 3 });

    mocks.clearArtSource.mockResolvedValue(artState);
    mocks.rescanMapArt.mockResolvedValue({ ...artState, problem: 'none found' });
    await library.vm.$emit('clear-art-source', 'install');
    await library.vm.$emit('rescan-map-art');
    await flushPromises();
    expect(mocks.clearArtSource).toHaveBeenCalledWith('install');
    expect(mocks.rescanMapArt).toHaveBeenCalled();

    const resizer = wrapper.get('[role="separator"]');
    await resizer.trigger('keydown', { key: 'ArrowRight' });
    await resizer.trigger('keydown', { key: 'Enter' });
    await resizer.trigger('pointerdown', { clientX: 100 });
    window.dispatchEvent(new PointerEvent('pointermove', { clientX: 140 }));
    window.dispatchEvent(new PointerEvent('pointerup'));
    expect(localStorage.getItem('wic-replay-viewer.library-width')).toBeTruthy();

    await library.vm.$emit('select', row);
    await flushPromises();
    await wrapper.getComponent(ReplayDetail).vm.$emit('back');
    await flushPromises();
    expect(wrapper.getComponent(ReplayDetail).props('summary')).toBeNull();

    await library.vm.$emit('update-export-selection', new Set([row.path]));
    await library.vm.$emit('clear-export-selection');
    await library.vm.$emit('select', row);
    await flushPromises();
    await wrapper.getComponent(ReplayDetail).vm.$emit('manage', 'rename');
    await wrapper.vm.$nextTick();
    await wrapper.getComponent(ReplayManagementDialog).vm.$emit('close');
    await wrapper.vm.$nextTick();
    expect(wrapper.findComponent(ReplayManagementDialog).exists()).toBe(false);
  });

  it('batch-exports selected summaries and retains failures in the dialog', async () => {
    const first = replaySummary({ path: '/first', fileName: 'first.wicdemo' });
    const second = replaySummary({ path: '/second', fileName: 'second.wicdemo' });
    const wrapper = await mountApp([first, second]);
    const library = wrapper.getComponent(ReplayLibrary);
    await library.vm.$emit('update-export-selection', new Set(['/first', '/second']));
    await library.vm.$emit('export-selected');
    await wrapper.vm.$nextTick();
    mocks.chooseFolder.mockResolvedValue('/exports');
    mocks.exportReplayCopies.mockResolvedValue({
      folder: '/exports/set',
      exports: [
        { path: '/exports/set/first.wicdemo', summary: null },
        { path: '/exports/set/second.wicdemo', summary: null },
      ],
    });
    await wrapper.getComponent(ReplayManagementDialog).vm.$emit('submit', {
      fileName: first.fileName,
      replayName: null,
      newFolderName: 'set',
    });
    await flushPromises();
    expect(mocks.exportReplayCopies).toHaveBeenCalledWith(['/first', '/second'], '/exports', 'set');
    expect(wrapper.text()).toContain('2 replays exported to /exports/set');

    await library.vm.$emit('select', first);
    await wrapper.getComponent(ReplayDetail).vm.$emit('manage', 'rename');
    await wrapper.vm.$nextTick();
    mocks.renameReplayFile.mockRejectedValue(new Error('occupied'));
    await wrapper.getComponent(ReplayManagementDialog).vm.$emit('submit', {
      fileName: 'occupied.wicdemo',
      replayName: null,
      newFolderName: null,
    });
    await flushPromises();
    expect(wrapper.getComponent(ReplayManagementDialog).props('error')).toBe('occupied');
  });
});

it('recovers all persisted rows and prunes selection when completion and row events are lost', async () => {
  vi.useFakeTimers();
  const gone = replaySummary({ path: '/gone' });
  const wrapper = await mountApp([gone]);
  const library = wrapper.getComponent(ReplayLibrary);
  await library.vm.$emit('select', gone);
  await library.vm.$emit('update-export-selection', new Set(['/gone']));
  await library.vm.$emit('scan');
  mocks.getInitialState.mockResolvedValue({
    summaries: [replaySummary({ path: '/persisted' })],
    locations: ['/replays'],
    importing: false,
    cachedMapArtCount: 0,
  });
  mocks.isImportRunning.mockResolvedValue(false);
  await vi.advanceTimersByTimeAsync(2000);
  expect(library.props('rows').map((row: { path: string }) => row.path)).toEqual(['/persisted']);
  expect(library.props('importing')).toBe(false);
  expect(library.props('exportSelection').size).toBe(0);
  expect(wrapper.getComponent(ReplayDetail).props('summary')).toBeNull();
  expect(wrapper.text()).toContain('recovered from saved results');
});

it('keeps bulk completion responsive and stops monitoring after unmount', async () => {
  vi.useFakeTimers();
  const wrapper = await mountApp([]);
  const library = wrapper.getComponent(ReplayLibrary);
  await library.vm.$emit('scan');
  mocks.importHandler!({
    type: 'importPrepared',
    payload: { discovered: 4096, pending: 4096, skipped: 0 },
  });
  for (let start = 0; start < 4096; start += 64) {
    mocks.importHandler!({
      type: 'summariesStored',
      payload: Array.from({ length: 64 }, (_, offset) =>
        replaySummary({ path: `/bulk/${start + offset}.wicdemo` }),
      ),
    });
  }
  mocks.importHandler!({
    type: 'importFinished',
    payload: { imported: 4096, failed: 0, skipped: 0, cancelled: false },
  });
  await wrapper.vm.$nextTick();
  expect(library.props('rows')).toEqual([]);
  expect(library.props('importing')).toBe(true);
  await vi.advanceTimersByTimeAsync(1000);
  expect(library.props('rows')).toHaveLength(4096);
  expect(library.props('importing')).toBe(false);
  const reads = mocks.getInitialState.mock.calls.length;
  wrapper.unmount();
  await vi.advanceTimersByTimeAsync(5000);
  expect(mocks.getInitialState).toHaveBeenCalledTimes(reads);
});

it('discards a bulk completion still preparing when the view unmounts', async () => {
  vi.useFakeTimers();
  const wrapper = await mountApp([]);
  mocks.importHandler!({
    type: 'summariesStored',
    payload: Array.from({ length: 1024 }, (_, index) => replaySummary({ path: `/bulk/${index}` })),
  });
  mocks.importHandler!({
    type: 'importFinished',
    payload: { imported: 1024, failed: 0, skipped: 0, cancelled: false },
  });
  wrapper.unmount();
  await vi.advanceTimersByTimeAsync(5000);
  expect(mocks.isImportRunning).not.toHaveBeenCalled();
});

it('does not overwrite a newly added location with a slow recovery snapshot', async () => {
  vi.useFakeTimers();
  const report = vi.spyOn(console, 'error').mockImplementation(() => {});
  try {
    const wrapper = await mountApp([]);
    const library = wrapper.getComponent(ReplayLibrary);
    mocks.importReplayFolders.mockResolvedValue(['/replays']);
    await library.vm.$emit('scan');
    await flushPromises();
    const oldSnapshot = deferred<{
      summaries: ReturnType<typeof replaySummary>[];
      locations: string[];
      importing: boolean;
      cachedMapArtCount: number;
    }>();
    mocks.isImportRunning.mockResolvedValue(false);
    mocks.getInitialState.mockReturnValueOnce(oldSnapshot.promise);
    await vi.advanceTimersByTimeAsync(2000);
    mocks.chooseReplayFolders.mockResolvedValue(['/another']);
    mocks.addReplayFolders.mockResolvedValue(['/replays', '/another']);
    await library.vm.$emit('add-locations');
    await flushPromises();
    oldSnapshot.resolve({
      summaries: [],
      locations: ['/replays'],
      importing: false,
      cachedMapArtCount: 0,
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(library.props('locations')).toEqual(['/replays', '/another']);
    expect(library.props('importing')).toBe(true);
    expect(report).toHaveBeenCalledOnce();
    mocks.getInitialState.mockResolvedValue({
      summaries: [],
      locations: ['/replays', '/another'],
      importing: false,
      cachedMapArtCount: 0,
    });
    await vi.advanceTimersByTimeAsync(2000);
    expect(library.props('importing')).toBe(false);
  } finally {
    report.mockRestore();
  }
});
