<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watchEffect } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { documentDir } from '@tauri-apps/api/path';
import { getCurrentWebview } from '@tauri-apps/api/webview';

import packageMetadata from '../package.json';
import { countImportEvent, reportFrontendError, setDiagnosticUi } from './errorReporting';

import {
  addReplayFolders,
  cancelReplayImport,
  changeReplayInGameName,
  chooseFolder,
  chooseReplayExport,
  chooseReplayFolders,
  clearArtSource,
  exportReplayCopies,
  exportReplayCopy,
  getInitialState,
  getMapArtState,
  getReplayDetail,
  importReplayFolders,
  isImportRunning,
  listenForImportEvents,
  removeReplayFolder,
  renameReplayFile,
  rescanMapArt,
  setArtSource,
} from './api';
import { ImportRecovery } from './importRecovery';
import { DetailRequestController, ImportEventController, pruneReplayState } from './appController';
import ReplayDetail from './components/ReplayDetail.vue';
import ReplayLibrary from './components/ReplayLibrary.vue';
import ReplayManagementDialog from './components/ReplayManagementDialog.vue';
import { exportDefaultPath, parentDirectory } from './exportLocation';
import { errorMessage } from './format';
import { resetMapArt, setMapArtAvailable } from './mapArt';
import { filterReplaySearch, indexReplaySummaries, parseReplaySearch } from './replaySearch';
import { replayLibraryStats } from './libraryStats';
import { prepareLibrary, preparedLibrary } from './libraryPreparation';
import { mergeReplaySummaries, mergeReplaySummariesAsync } from './summaryBatch';
import type {
  ArtSourceKind,
  DetailTab,
  DetailView,
  ImportEvent,
  ImportProgress,
  InitialState,
  MapArtState,
  ReplaySummary,
} from './types';

const summaries = shallowRef<ReplaySummary[]>([]);
const selectedPath = ref<string | null>(null);
const exportSelection = ref(new Set<string>());
const selectedDetail = ref<DetailView | null>(null);
const detailLoading = ref(false);
const detailError = ref<string | null>(null);
const detailTab = ref<DetailTab>('overview');
const search = ref('');
const locations = ref<string[]>([]);
const pendingScanRoots = ref<string[]>([]);
const importing = ref(false);
const cancellingImport = ref(false);
const activeImportRoots = ref<string[]>([]);
const importProgress = ref<ImportProgress | null>(null);
const status = ref('Initializing local replay archive…');
const mapArt = ref<MapArtState | null>(null);
const managementMode = ref<'rename' | 'name' | 'export' | null>(null);
const managementBusy = ref(false);
const managementError = ref<string | null>(null);
const managementTargets = ref<ReplaySummary[]>([]);
watchEffect(() =>
  setDiagnosticUi({
    view: selectedPath.value ? detailTab.value : 'library',
    detailPending: detailLoading.value,
    importPending: importing.value,
    managementPending: managementBusy.value,
  }),
);
const appVersion = ref(packageMetadata.version);
let unlisten: (() => void) | null = null;
let unlistenDragDrop: (() => void) | null = null;
const dragActive = ref(false);
const libraryWidth = ref(520);
const libraryResizing = ref(false);
const LIBRARY_MIN_WIDTH = 460;
const LIBRARY_MAX_WIDTH = 720;
const LIBRARY_DETAIL_MIN_WIDTH = 520;
const LIBRARY_WIDTH_STORAGE_KEY = 'wic-replay-viewer.library-width';
const LAST_EXPORT_DIRECTORY_STORAGE_KEY = 'wic-replay-viewer.last-export-directory';
let resizeStartX = 0;
let resizeStartWidth = 0;
let lastExportDirectory: string | null = null;
const importController = new ImportEventController();
const detailController = new DetailRequestController();
const replayLibrary = ref<{ focusReplay: (path: string) => Promise<void> } | null>(null);

const selectedSummary = computed(
  () => summaries.value.find((row) => row.path === selectedPath.value) ?? null,
);
const libraryStats = computed(
  () => preparedLibrary(summaries.value)?.stats ?? replayLibraryStats(summaries.value),
);

const searchIndex = computed(() => indexReplaySummaries(summaries.value));
const searchQuery = computed(() => parseReplaySearch(search.value));
const filteredSummaries = computed(() => {
  return searchQuery.value.empty
    ? summaries.value
    : filterReplaySearch(searchIndex.value, searchQuery.value);
});

/// Map art is a bonus layer: a failure here must never disturb the library, so
/// it is reported through the map art card and never through the main status.
function applyMapArt(state: MapArtState): void {
  if (state.cacheChanged) {
    resetMapArt();
  }
  mapArt.value = state;
  setMapArtAvailable(state.mapCount > 0);
}

function reportMapArtProblem(error: unknown): void {
  mapArt.value = {
    installPath: mapArt.value?.installPath ?? null,
    customMapsPath: mapArt.value?.customMapsPath ?? null,
    mapCount: mapArt.value?.mapCount ?? 0,
    problem: errorMessage(error),
    cacheChanged: false,
  };
}

const ART_SOURCE_TITLES: Record<ArtSourceKind, string> = {
  install: 'Select your World in Conflict folder',
  customMaps: 'Select your downloaded maps folder',
};

async function chooseArtSource(kind: ArtSourceKind): Promise<void> {
  try {
    const chosen = await chooseFolder(ART_SOURCE_TITLES[kind]);
    if (!chosen) {
      return;
    }
    applyMapArt(await setArtSource(kind, chosen));
  } catch (error) {
    reportMapArtProblem(error);
  }
}

async function forgetArtSource(kind: ArtSourceKind): Promise<void> {
  try {
    applyMapArt(await clearArtSource(kind));
  } catch (error) {
    reportMapArtProblem(error);
  }
}

async function refreshMapArt(): Promise<void> {
  try {
    applyMapArt(await rescanMapArt());
  } catch (error) {
    reportMapArtProblem(error);
  }
}

function maxLibraryWidth(): number {
  return Math.max(
    LIBRARY_MIN_WIDTH,
    Math.min(LIBRARY_MAX_WIDTH, window.innerWidth - LIBRARY_DETAIL_MIN_WIDTH),
  );
}

function setLibraryWidth(width: number): void {
  libraryWidth.value = Math.min(maxLibraryWidth(), Math.max(LIBRARY_MIN_WIDTH, width));
}

function fitLibraryWidth(): void {
  setLibraryWidth(libraryWidth.value);
}

function saveLibraryWidth(): void {
  localStorage.setItem(LIBRARY_WIDTH_STORAGE_KEY, String(libraryWidth.value));
}

function resizeLibrary(event: PointerEvent): void {
  setLibraryWidth(resizeStartWidth + event.clientX - resizeStartX);
}

function stopLibraryResize(): void {
  if (!libraryResizing.value) {
    return;
  }
  libraryResizing.value = false;
  window.removeEventListener('pointermove', resizeLibrary);
  window.removeEventListener('pointerup', stopLibraryResize);
  window.removeEventListener('pointercancel', stopLibraryResize);
  saveLibraryWidth();
}

function startLibraryResize(event: PointerEvent): void {
  resizeStartX = event.clientX;
  resizeStartWidth = libraryWidth.value;
  libraryResizing.value = true;
  window.addEventListener('pointermove', resizeLibrary);
  window.addEventListener('pointerup', stopLibraryResize);
  window.addEventListener('pointercancel', stopLibraryResize);
  event.preventDefault();
}

function handleLibraryResizeKey(event: KeyboardEvent): void {
  const adjustment = event.key === 'ArrowLeft' ? -20 : event.key === 'ArrowRight' ? 20 : 0;
  if (!adjustment) {
    return;
  }
  event.preventDefault();
  setLibraryWidth(libraryWidth.value + adjustment);
  saveLibraryWidth();
}

let libraryGeneration = 0;
let mounted = true;

function restoreImport(state: InitialState): void {
  libraryGeneration += 1;
  const wasImporting = importing.value;
  const available = new Set(state.summaries.map((row) => row.path));
  const removed = summaries.value.filter((row) => !available.has(row.path)).map((row) => row.path);
  const pruned = pruneReplayState(
    state.summaries,
    exportSelection.value,
    selectedPath.value,
    removed,
  );
  importController.reset();
  summaries.value = pruned.summaries;
  exportSelection.value = pruned.exportSelection;
  selectedPath.value = pruned.selectedPath;
  if (pruned.selectedWasRemoved) {
    detailController.cancel();
    selectedDetail.value = null;
    detailError.value = null;
    detailLoading.value = false;
  }
  locations.value = state.locations;
  importing.value = false;
  cancellingImport.value = false;
  importProgress.value = null;
  activeImportRoots.value = [];
  if (wasImporting) {
    status.value = 'Scan finished; library recovered from saved results';
  }
}

const importRecovery = new ImportRecovery(
  isImportRunning,
  async () => {
    const previousRows = summaries.value;
    const previousLocations = locations.value;
    const state = await getInitialState();
    await prepareLibrary(state.summaries);
    if (summaries.value !== previousRows || locations.value !== previousLocations) {
      throw new Error('Library changed during recovery; reading a fresh snapshot');
    }
    return state;
  },
  restoreImport,
  (error) => {
    // eslint-disable-next-line no-console -- recovery diagnostics must survive a failed UI callback
    console.error('Replay import recovery failed; retrying', error);
    reportFrontendError('callbackFailed', error);
  },
);

function handleImportEvent(event: ImportEvent): void {
  countImportEvent();
  const processing = processImportEvent(event);
  const generation = libraryGeneration;
  void processing.catch((error: unknown) => {
    if (!mounted || generation !== libraryGeneration) {
      return;
    }
    // eslint-disable-next-line no-console -- diagnose callback failures without disrupting recovery
    console.error('Replay import callback failed', error);
    reportFrontendError('callbackFailed', error);
    status.value = 'Refreshing scan state from saved results…';
    importRecovery.start();
  });
}

async function processImportEvent(event: ImportEvent): Promise<void> {
  if (!mounted) {
    return;
  }
  if (event.type !== 'importFinished') {
    applyImportEvent(event);
    return;
  }
  const generation = ++libraryGeneration;
  status.value = 'Preparing replay library…';
  const previousRows = summaries.value;
  const rows = await mergeReplaySummariesAsync(
    summaries.value,
    importController.pendingSummaries(),
    event.payload.removedPaths ?? event.payload.removed_paths ?? [],
  );
  await prepareLibrary(rows);
  if (generation === libraryGeneration && mounted) {
    if (summaries.value === previousRows) {
      applyImportEvent(event, rows);
    } else {
      importRecovery.start();
    }
  }
}

function applyImportEvent(event: ImportEvent, completedSummaries?: ReplaySummary[]): void {
  const transition = importController.apply(
    {
      summaries: summaries.value,
      progress: importProgress.value,
      importing: importing.value,
      cancelling: cancellingImport.value,
      status: status.value,
    },
    event,
    completedSummaries,
  );
  summaries.value = transition.summaries;
  importProgress.value = transition.progress;
  importing.value = transition.importing;
  cancellingImport.value = transition.cancelling;
  status.value = transition.status;

  if (event.type === 'importFinished') {
    importRecovery.start();
    const removedPaths = event.payload.removedPaths ?? event.payload.removed_paths ?? [];
    const pruned = pruneReplayState(
      summaries.value,
      exportSelection.value,
      selectedPath.value,
      removedPaths,
    );
    summaries.value = pruned.summaries;
    exportSelection.value = pruned.exportSelection;
    selectedPath.value = pruned.selectedPath;
    if (pruned.selectedWasRemoved) {
      detailController.cancel();
      selectedDetail.value = null;
      detailError.value = null;
      detailLoading.value = false;
    }
    if (!event.payload.cancelled) {
      pendingScanRoots.value = pendingScanRoots.value.filter(
        (path) => !activeImportRoots.value.includes(path),
      );
    }
    activeImportRoots.value = [];
  }
}

async function browse(): Promise<void> {
  try {
    const selected = await chooseReplayFolders();
    if (!selected.length) {
      return;
    }
    const previous = new Set(locations.value);
    locations.value = await addReplayFolders(selected);
    pendingScanRoots.value = locations.value.filter((path) => !previous.has(path));
    status.value = pendingScanRoots.value.length
      ? `${pendingScanRoots.value.length} ${pendingScanRoots.value.length === 1 ? 'folder' : 'folders'} added; ready to scan`
      : 'Those replay folders are already in the library';
  } catch (error) {
    status.value = errorMessage(error);
  }
}

async function beginImport(roots: string[] = locations.value): Promise<void> {
  if (!roots.length || importing.value) {
    return;
  }
  libraryGeneration += 1;
  importing.value = true;
  importRecovery.start();
  activeImportRoots.value = [...roots];
  status.value = `Discovering replays across ${roots.length} ${roots.length === 1 ? 'location' : 'locations'}`;
  try {
    locations.value = await importReplayFolders(roots);
  } catch (error) {
    importing.value = false;
    activeImportRoots.value = [];
    status.value = errorMessage(error);
  }
}

async function cancelImport(): Promise<void> {
  if (!importing.value || cancellingImport.value) {
    return;
  }
  cancellingImport.value = true;
  status.value = 'Stopping scan after active parser work finishes';
  const generation = libraryGeneration;
  try {
    if (!(await cancelReplayImport())) {
      // The backend can already be idle if a completion event was missed or a
      // frontend callback failed. Reconcile from SQLite instead of retaining
      // stale local import state.
      const initial = await getInitialState();
      await prepareLibrary(initial.summaries);
      if (!mounted || generation !== libraryGeneration) {
        return;
      }
      libraryGeneration += 1;
      importController.reset();
      summaries.value = initial.summaries;
      locations.value = initial.locations;
      importing.value = initial.importing;
      cancellingImport.value = false;
      importProgress.value = null;
      pendingScanRoots.value = pendingScanRoots.value.filter(
        (path) => !activeImportRoots.value.includes(path),
      );
      activeImportRoots.value = [];
      if (selectedPath.value && !summaries.value.some((row) => row.path === selectedPath.value)) {
        selectedPath.value = null;
        selectedDetail.value = null;
        detailError.value = null;
      }
      status.value = initial.importing
        ? 'Scan state refreshed; stopping active parser work'
        : 'Scan already finished; library refreshed';
    }
  } catch (error) {
    cancellingImport.value = false;
    status.value = errorMessage(error);
  }
}

async function removeLocation(path: string): Promise<void> {
  try {
    const result = await removeReplayFolder(path);
    locations.value = result.locations;
    pendingScanRoots.value = pendingScanRoots.value.filter((root) => root !== path);
    const removed = new Set(result.removedReplayPaths);
    summaries.value = summaries.value.filter((row) => !removed.has(row.path));
    exportSelection.value = new Set(
      [...exportSelection.value].filter((replayPath) => !removed.has(replayPath)),
    );
    if (selectedPath.value && removed.has(selectedPath.value)) {
      selectedPath.value = null;
      selectedDetail.value = null;
      detailError.value = null;
    }
    status.value = `Location removed${removed.size ? ` with ${removed.size} indexed replays` : ''}`;
  } catch (error) {
    status.value = errorMessage(error);
  }
}

async function selectReplay(row: ReplaySummary): Promise<void> {
  if (selectedPath.value === row.path) {
    return;
  }
  selectedPath.value = row.path;
  selectedDetail.value = null;
  detailError.value = row.parseError;
  detailTab.value = 'overview';
  const generation = detailController.begin();
  if (row.parseError) {
    return;
  }

  detailLoading.value = true;
  try {
    const detail = await getReplayDetail(row.path);
    if (detailController.isCurrent(generation)) {
      selectedDetail.value = detail;
    }
  } catch (error) {
    if (detailController.isCurrent(generation)) {
      detailError.value = errorMessage(error);
    }
  } finally {
    if (detailController.isCurrent(generation)) {
      detailLoading.value = false;
    }
  }
}

function returnToLibrary(): void {
  const path = selectedPath.value;
  if (!path) {
    return;
  }
  detailController.cancel();
  selectedPath.value = null;
  selectedDetail.value = null;
  detailLoading.value = false;
  detailError.value = null;
  detailTab.value = 'overview';
  const nextExportSelection = new Set(exportSelection.value);
  nextExportSelection.delete(path);
  exportSelection.value = nextExportSelection;
  void nextTick(() => replayLibrary.value?.focusReplay(path));
}

function openReplayManagement(mode: 'rename' | 'name'): void {
  if (!selectedSummary.value) {
    return;
  }
  managementTargets.value = [selectedSummary.value];
  managementMode.value = mode;
  managementBusy.value = false;
  managementError.value = null;
}

function updateExportSelection(paths: Set<string>): void {
  exportSelection.value = paths;
}

function clearExportSelection(): void {
  exportSelection.value = new Set();
}

function openSelectedReplayExport(): void {
  const selected = summaries.value.filter((row) => exportSelection.value.has(row.path));
  if (!selected.length) {
    return;
  }
  managementTargets.value = selected;
  managementMode.value = 'export';
  managementBusy.value = false;
  managementError.value = null;
}

function closeReplayManagement(): void {
  if (managementBusy.value) {
    return;
  }
  managementMode.value = null;
  managementTargets.value = [];
  managementError.value = null;
}

function rememberExportDirectory(directory: string): void {
  lastExportDirectory = directory;
  try {
    localStorage.setItem(LAST_EXPORT_DIRECTORY_STORAGE_KEY, directory);
  } catch {
    // The successful export still counts even if the webview cannot persist preferences.
  }
}

async function exportStartDirectory(): Promise<string> {
  return lastExportDirectory ?? documentDir();
}

async function applyManagedSummary(oldPath: string, summary: ReplaySummary): Promise<void> {
  summaries.value = mergeReplaySummaries(
    summaries.value.filter((row) => row.path !== oldPath && row.path !== summary.path),
    [summary],
  );
  if (exportSelection.value.has(oldPath)) {
    const next = new Set(exportSelection.value);
    next.delete(oldPath);
    next.add(summary.path);
    exportSelection.value = next;
  }
  selectedPath.value = null;
  selectedDetail.value = null;
  detailError.value = null;
  await selectReplay(summary);
}

async function submitReplayManagement(payload: {
  fileName: string;
  replayName: string | null;
  newFolderName: string | null;
}): Promise<void> {
  const targets = managementTargets.value;
  const row = targets[0];
  const mode = managementMode.value;
  if (!row || !mode || managementBusy.value) {
    return;
  }

  managementBusy.value = true;
  managementError.value = null;
  try {
    if (mode === 'rename') {
      const result = await renameReplayFile(row.path, payload.fileName);
      if (!result.summary) {
        throw new Error('Renamed replay was not returned to the library');
      }
      await applyManagedSummary(row.path, result.summary);
      status.value = `Renamed replay to ${result.summary.fileName}`;
    } else if (mode === 'name') {
      if (payload.replayName === null) {
        throw new Error('Enter an in-game replay name');
      }
      const result = await changeReplayInGameName(row.path, payload.replayName);
      if (!result.summary) {
        throw new Error('Updated replay was not returned to the library');
      }
      await applyManagedSummary(row.path, result.summary);
      status.value = `In-game replay name changed to ${result.summary.replayName}`;
    } else if (targets.length === 1) {
      const directory = await exportStartDirectory();
      const destination = await chooseReplayExport(exportDefaultPath(row.path, directory));
      if (!destination) {
        return;
      }
      const result = await exportReplayCopy(row.path, destination);
      const exportedDirectory = parentDirectory(result.path);
      if (exportedDirectory) {
        rememberExportDirectory(exportedDirectory);
      }
      if (result.summary) {
        summaries.value = mergeReplaySummaries(summaries.value, [result.summary]);
      }
      status.value = `Replay exported to ${result.path}`;
    } else {
      const directory = await exportStartDirectory();
      const destination = await chooseFolder(
        payload.newFolderName
          ? `Select the parent for ${payload.newFolderName}`
          : `Select a folder for ${targets.length} replay copies`,
        directory,
      );
      if (!destination) {
        return;
      }
      const result = await exportReplayCopies(
        targets.map((target) => target.path),
        destination,
        payload.newFolderName,
      );
      rememberExportDirectory(destination);
      const exportedSummaries = result.exports.flatMap((item) =>
        item.summary ? [item.summary] : [],
      );
      if (exportedSummaries.length) {
        summaries.value = mergeReplaySummaries(summaries.value, exportedSummaries);
      }
      const exportedPaths = new Set(targets.map((target) => target.path));
      exportSelection.value = new Set(
        [...exportSelection.value].filter((path) => !exportedPaths.has(path)),
      );
      status.value = `${result.exports.length} replays exported to ${result.folder}`;
    }
    managementMode.value = null;
    managementTargets.value = [];
  } catch (error) {
    managementError.value = errorMessage(error);
  } finally {
    managementBusy.value = false;
  }
}

onMounted(async () => {
  void getVersion()
    .then((version) => {
      if (version.trim()) {
        appVersion.value = version;
      }
    })
    .catch(() => {
      // Plain-browser development has no Tauri runtime. The package metadata
      // fallback is generated from the same release-version source.
    });

  lastExportDirectory = localStorage.getItem(LAST_EXPORT_DIRECTORY_STORAGE_KEY) || null;
  const savedLibraryWidth = Number(localStorage.getItem(LIBRARY_WIDTH_STORAGE_KEY));
  if (Number.isFinite(savedLibraryWidth) && savedLibraryWidth > 0) {
    setLibraryWidth(savedLibraryWidth);
  } else {
    setLibraryWidth(libraryWidth.value);
  }
  window.addEventListener('resize', fitLibraryWidth);
  try {
    unlisten = await listenForImportEvents(handleImportEvent);
    unlistenDragDrop = await getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === 'enter' || payload.type === 'over') {
        dragActive.value = true;
      } else if (payload.type === 'leave') {
        dragActive.value = false;
      } else {
        dragActive.value = false;
        void beginImport(payload.paths);
      }
    });
    const generation = libraryGeneration;
    const initial = await getInitialState();
    await prepareLibrary(initial.summaries);
    if (!mounted || generation !== libraryGeneration) {
      return;
    }
    // SQLite art is usable immediately. Source reachability and first-run
    // archive decoding continue independently after the library is visible.
    mapArt.value = {
      installPath: null,
      customMapsPath: null,
      mapCount: initial.cachedMapArtCount,
      problem: null,
      cacheChanged: false,
    };
    setMapArtAvailable(initial.cachedMapArtCount > 0);
    summaries.value = initial.summaries;
    locations.value = initial.locations;
    importing.value = initial.importing;
    if (initial.importing) {
      importRecovery.start();
    }
    status.value = initial.locations.length
      ? `${initial.locations.length} ${initial.locations.length === 1 ? 'location' : 'locations'} ready`
      : 'Add folders or drop them into the window to begin';
    // Decoding art can take a moment on first run, so it is never awaited on
    // the path that puts the library on screen.
    void getMapArtState().then(applyMapArt).catch(reportMapArtProblem);
  } catch (error) {
    status.value = `Desktop backend unavailable: ${errorMessage(error)}`;
  }
});

onBeforeUnmount(() => {
  mounted = false;
  libraryGeneration += 1;
  importRecovery.stop();
  stopLibraryResize();
  window.removeEventListener('resize', fitLibraryWidth);
  unlisten?.();
  unlistenDragDrop?.();
});
</script>

<template>
  <div class="app-shell">
    <div
      class="workspace"
      :class="{ 'workspace-resizing': libraryResizing }"
      :style="{ '--library-width': `${libraryWidth}px` }"
      @keydown.esc="returnToLibrary"
    >
      <ReplayLibrary
        ref="replayLibrary"
        :rows="filteredSummaries"
        :prepared-groups="preparedLibrary(filteredSummaries)?.groups"
        :locations="locations"
        :importing="importing"
        :cancelling-import="cancellingImport"
        :pending-scan-roots="pendingScanRoots"
        :selected-path="selectedPath"
        :export-selection="exportSelection"
        :search="search"
        :map-art="mapArt"
        :stats="libraryStats"
        @select="selectReplay"
        @update-export-selection="updateExportSelection"
        @clear-export-selection="clearExportSelection"
        @export-selected="openSelectedReplayExport"
        @add-locations="browse"
        @scan="beginImport($event?.length ? $event : locations)"
        @cancel-scan="cancelImport"
        @remove-location="removeLocation"
        @update:search="search = $event"
        @choose-art-source="chooseArtSource"
        @clear-art-source="forgetArtSource"
        @rescan-map-art="refreshMapArt"
      />
      <div
        class="library-resizer"
        role="separator"
        aria-label="Resize replay library"
        aria-orientation="vertical"
        :aria-valuemin="LIBRARY_MIN_WIDTH"
        :aria-valuemax="maxLibraryWidth()"
        :aria-valuenow="libraryWidth"
        tabindex="0"
        @pointerdown="startLibraryResize"
        @keydown="handleLibraryResizeKey"
      />
      <ReplayDetail
        :summary="selectedSummary"
        :detail="selectedDetail"
        :loading="detailLoading"
        :error="detailError"
        :tab="detailTab"
        :location-count="locations.length"
        :library-count="summaries.length"
        @update:tab="detailTab = $event"
        @manage="openReplayManagement"
        @back="returnToLibrary"
      />
    </div>

    <ReplayManagementDialog
      v-if="managementMode && managementTargets[0]"
      :mode="managementMode"
      :summary="managementTargets[0]"
      :export-count="managementTargets.length"
      :busy="managementBusy"
      :error="managementError"
      @close="closeReplayManagement"
      @submit="submitReplayManagement"
    />

    <Transition name="drop-overlay">
      <div v-if="dragActive" class="drop-target" role="status" aria-live="polite">
        <div class="drop-target-card">
          <span class="drop-target-icon" aria-hidden="true">＋</span>
          <strong>Drop replay folders</strong>
          <p>They will be added to your library and scanned for .wicdemo files.</p>
        </div>
      </div>
    </Transition>

    <footer class="status-bar">
      <template v-if="importProgress">
        <div class="progress-track">
          <span
            :style="{
              width: `${importProgress.pending ? (importProgress.processed / importProgress.pending) * 100 : 0}%`,
            }"
          />
        </div>
        <strong>{{ importProgress.processed }} / {{ importProgress.pending }}</strong>
        <span>{{ importProgress.imported }} parsed</span>
        <span>{{ importProgress.failed }} failed</span>
        <span>{{ importProgress.skipped }} cached</span>
      </template>
      <template v-else>
        <span>{{ status }}</span>
        <span>{{ summaries.length }} indexed replays</span>
      </template>
      <span class="status-spacer" />
      <span class="status-version" :title="`WiC Replay Viewer ${appVersion}`">
        v. {{ appVersion }}
      </span>
    </footer>
  </div>
</template>
exportReplayCopy,
