import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke, listen, open, save } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open, save }));

import * as api from './api';

beforeEach(() => vi.clearAllMocks());

describe('Tauri IPC contract', () => {
  it.each([
    ['isImportRunning', [], 'import_running', undefined],
    ['getInitialState', [], 'initial_state', undefined],
    ['importReplayFolders', [['/a']], 'start_import', { roots: ['/a'] }],
    ['cancelReplayImport', [], 'cancel_import', undefined],
    ['addReplayFolders', [['/a']], 'add_library_locations', { roots: ['/a'] }],
    ['removeReplayFolder', ['/a'], 'remove_library_location', { path: '/a' }],
    ['getReplayDetail', ['/a.wicdemo'], 'replay_detail', { path: '/a.wicdemo' }],
    ['getReplayPlayback', ['/a.wicdemo'], 'replay_playback', { path: '/a.wicdemo' }],
    ['getReplayObjectives', ['/a.wicdemo'], 'replay_objectives', { path: '/a.wicdemo' }],
    [
      'renameReplayFile',
      ['/a.wicdemo', 'b.wicdemo'],
      'rename_replay_file',
      { path: '/a.wicdemo', fileName: 'b.wicdemo' },
    ],
    [
      'changeReplayInGameName',
      ['/a.wicdemo', 'Title'],
      'change_replay_in_game_name',
      { path: '/a.wicdemo', replayName: 'Title' },
    ],
    [
      'exportReplayCopy',
      ['/a.wicdemo', '/b.wicdemo'],
      'export_replay_copy',
      { path: '/a.wicdemo', destination: '/b.wicdemo' },
    ],
    [
      'exportReplayCopies',
      [['/a'], '/out', 'set'],
      'export_replay_copies',
      { paths: ['/a'], destinationFolder: '/out', newFolderName: 'set' },
    ],
    ['getMapArtState', [], 'map_art_state', undefined],
    ['setArtSource', ['install', '/game'], 'set_art_source', { kind: 'install', path: '/game' }],
    ['clearArtSource', ['customMaps'], 'clear_art_source', { kind: 'customMaps' }],
    ['rescanMapArt', [], 'rescan_map_art', undefined],
    ['getMapArtBatch', [['a', 'b']], 'map_art_batch', { mapNames: ['a', 'b'] }],
  ] as const)(
    '%s invokes %s with the stable camelCase payload',
    async (name, args, command, payload) => {
      invoke.mockResolvedValueOnce(null);
      const call = api[name] as unknown as (...values: unknown[]) => Promise<unknown>;
      await call(...(args as unknown as unknown[]));
      expect(invoke).toHaveBeenCalledWith(command, ...(payload === undefined ? [] : [payload]));
    },
  );
});

it('normalizes folder picker cancellation and single/multiple results', async () => {
  open
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce('/one')
    .mockResolvedValueOnce(['/a', '/b']);
  await expect(api.chooseReplayFolders()).resolves.toEqual([]);
  await expect(api.chooseReplayFolders()).resolves.toEqual(['/one']);
  await expect(api.chooseReplayFolders()).resolves.toEqual(['/a', '/b']);
});

it('forwards import event payloads and dialog contracts', async () => {
  const handler = vi.fn();
  listen.mockImplementationOnce(
    async (_name: string, callback: (event: { payload: unknown }) => void) => {
      callback({ payload: { type: 'importProblem', payload: 'bad' } });
      return vi.fn();
    },
  );
  await api.listenForImportEvents(handler);
  expect(listen).toHaveBeenCalledWith('replay-import', expect.any(Function));
  expect(handler).toHaveBeenCalledWith({ type: 'importProblem', payload: 'bad' });

  save.mockResolvedValueOnce('/export.wicdemo');
  await expect(api.chooseReplayExport('/default.wicdemo')).resolves.toBe('/export.wicdemo');
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: '/default.wicdemo' }));
});
