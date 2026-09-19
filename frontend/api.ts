import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';

import type {
  ArtSourceKind,
  DetailView,
  ImportEvent,
  InitialState,
  MapArtState,
  MapArtAsset,
  PlaybackView,
  PlaybackObjective,
  ReplayBatchExportResult,
  ReplayOperationResult,
  RemovedLocation,
} from './types';

export function getInitialState(): Promise<InitialState> {
  return invoke<InitialState>('initial_state');
}

export function importReplayFolders(roots: string[]): Promise<string[]> {
  return invoke<string[]>('start_import', { roots });
}

export function isImportRunning(): Promise<boolean> {
  return invoke<boolean>('import_running');
}

export function cancelReplayImport(): Promise<boolean> {
  return invoke<boolean>('cancel_import');
}

export function addReplayFolders(roots: string[]): Promise<string[]> {
  return invoke<string[]>('add_library_locations', { roots });
}

export function removeReplayFolder(path: string): Promise<RemovedLocation> {
  return invoke<RemovedLocation>('remove_library_location', { path });
}

export function getReplayDetail(path: string): Promise<DetailView> {
  return invoke<DetailView>('replay_detail', { path });
}

export function getReplayPlayback(path: string): Promise<PlaybackView> {
  return invoke<PlaybackView>('replay_playback', { path });
}

export function getReplayObjectives(path: string): Promise<PlaybackObjective[]> {
  return invoke<PlaybackObjective[]>('replay_objectives', { path });
}

export function renameReplayFile(path: string, fileName: string): Promise<ReplayOperationResult> {
  return invoke<ReplayOperationResult>('rename_replay_file', { path, fileName });
}

export function changeReplayInGameName(
  path: string,
  replayName: string,
): Promise<ReplayOperationResult> {
  return invoke<ReplayOperationResult>('change_replay_in_game_name', { path, replayName });
}

export function exportReplayCopy(
  path: string,
  destination: string,
): Promise<ReplayOperationResult> {
  return invoke<ReplayOperationResult>('export_replay_copy', { path, destination });
}

export function exportReplayCopies(
  paths: string[],
  destinationFolder: string,
  newFolderName: string | null,
): Promise<ReplayBatchExportResult> {
  return invoke<ReplayBatchExportResult>('export_replay_copies', {
    paths,
    destinationFolder,
    newFolderName,
  });
}

export function getMapArtState(): Promise<MapArtState> {
  return invoke<MapArtState>('map_art_state');
}

export function setArtSource(kind: ArtSourceKind, path: string): Promise<MapArtState> {
  return invoke<MapArtState>('set_art_source', { kind, path });
}

export function clearArtSource(kind: ArtSourceKind): Promise<MapArtState> {
  return invoke<MapArtState>('clear_art_source', { kind });
}

export function rescanMapArt(): Promise<MapArtState> {
  return invoke<MapArtState>('rescan_map_art');
}

/// Read the unique map images needed by visible rows with one IPC/database
/// operation. A `null` value is a normal cache miss.
export function getMapArtBatch(mapNames: string[]): Promise<Record<string, MapArtAsset | null>> {
  return invoke<Record<string, MapArtAsset | null>>('map_art_batch', { mapNames });
}

export async function chooseFolder(title: string, defaultPath?: string): Promise<string | null> {
  const selected = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof selected === 'string' ? selected : null;
}

export async function chooseReplayFolders(): Promise<string[]> {
  const selected = await open({
    directory: true,
    multiple: true,
    title: 'Add replay folders',
  });
  if (typeof selected === 'string') {
    return [selected];
  }
  return selected ?? [];
}

export async function chooseReplayExport(defaultPath: string): Promise<string | null> {
  const selected = await save({
    title: 'Export replay copy',
    defaultPath,
    filters: [{ name: 'World in Conflict replay', extensions: ['wicdemo'] }],
  });
  return typeof selected === 'string' ? selected : null;
}

export function listenForImportEvents(handler: (event: ImportEvent) => void): Promise<UnlistenFn> {
  return listen<ImportEvent>('replay-import', ({ payload }) => handler(payload));
}
