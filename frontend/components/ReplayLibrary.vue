<script setup lang="ts">
import { displayLocationPath } from '../locationPath';
import { computed, nextTick, ref, watch } from 'vue';

import { displayDate, formatDuration, replayTitle } from '../format';
import { groupReplaysByFolder, type ReplayFolderGroup } from '../libraryGroups';
import { updateReplaySelection } from '../replaySelection';
import type { ReplayLibraryStats } from '../libraryStats';
import type { ArtSourceKind, MapArtState, ReplaySummary } from '../types';
import MapTile from './MapTile.vue';
import VirtualList from './VirtualList.vue';

const props = defineProps<{
  rows: ReplaySummary[];
  preparedGroups?: ReplayFolderGroup[];
  locations: string[];
  importing: boolean;
  cancellingImport: boolean;
  pendingScanRoots: string[];
  selectedPath: string | null;
  exportSelection: Set<string>;
  search: string;
  mapArt: MapArtState | null;
  stats: ReplayLibraryStats;
}>();

const emit = defineEmits<{
  select: [row: ReplaySummary];
  'update-export-selection': [paths: Set<string>];
  'clear-export-selection': [];
  'export-selected': [];
  'add-locations': [];
  scan: [roots?: string[]];
  'cancel-scan': [];
  'remove-location': [path: string];
  'update:search': [value: string];
  'choose-art-source': [kind: ArtSourceKind];
  'clear-art-source': [kind: ArtSourceKind];
  'rescan-map-art': [];
}>();

const staleCount = computed(() => props.stats.stale);
const locationsOpen = ref(true);
const mapArtOpen = ref(false);
const collapsedFolders = ref(new Set<string>());
const selectionAnchor = ref<string | null>(null);
const libraryPanel = ref<HTMLElement | null>(null);
const replayList = ref<{ scrollToIndex: (index: number) => void } | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const emptyTitle = computed(() =>
  props.search.trim() ? 'No matching replays' : 'Nothing indexed yet',
);
const emptyMessage = computed(() => {
  if (props.search.trim()) {
    return 'Try a different map, player, or recorder.';
  }
  return 'Scan your saved locations to discover .wicdemo files.';
});
const scanCopy = computed(() => {
  if (props.cancellingImport) {
    return { label: 'Stopping…', hint: 'Finishing active parser work, then stopping safely.' };
  }
  if (props.importing) {
    return { label: 'Cancel scan', hint: 'Discovering and parsing replay files…' };
  }
  if (props.pendingScanRoots.length) {
    const folders = props.pendingScanRoots.length === 1 ? 'Folder added' : 'Folders added';
    return {
      label: props.pendingScanRoots.length === 1 ? 'Scan new folder' : 'Scan new folders',
      hint: `${folders}. Scan ${props.pendingScanRoots.length === 1 ? 'it' : 'them'} when you are ready.`,
    };
  }
  if (staleCount.value) {
    return {
      label: 'Refresh library',
      hint: `${staleCount.value} indexed replays need refreshing.`,
    };
  }
  if (!props.stats.total) {
    return { label: 'Scan library', hint: 'Scan your saved locations to discover replays.' };
  }
  return {
    label: 'Scan all folders',
    hint: 'Check every saved location for new or changed replays.',
  };
});

type LibraryItem =
  | { kind: 'folder'; path: string; label: string; count: number }
  | { kind: 'replay'; row: ReplaySummary };

const libraryItems = computed<LibraryItem[]>(() =>
  (props.preparedGroups ?? groupReplaysByFolder(props.rows)).flatMap((group) => [
    { kind: 'folder' as const, path: group.path, label: group.label, count: group.rows.length },
    ...(collapsedFolders.value.has(group.path)
      ? []
      : group.rows.map((row) => ({ kind: 'replay' as const, row }))),
  ]),
);

const visibleReplayPaths = computed(() =>
  libraryItems.value.flatMap((item) => (item.kind === 'replay' ? [item.row.path] : [])),
);

watch(
  () => props.exportSelection.size,
  (size) => {
    if (!size) {
      selectionAnchor.value = null;
    }
  },
);

function selectReplay(row: ReplaySummary, event: MouseEvent): void {
  const update = updateReplaySelection(
    visibleReplayPaths.value,
    props.exportSelection,
    row.path,
    selectionAnchor.value,
    { toggle: event.ctrlKey || event.metaKey, range: event.shiftKey },
  );
  selectionAnchor.value = update.anchor;
  emit('update-export-selection', update.paths);
  emit('select', row);
}

function clearReplaySelection(): void {
  selectionAnchor.value = null;
  emit('clear-export-selection');
}

function toggleFolder(path: string): void {
  const next = new Set(collapsedFolders.value);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  collapsedFolders.value = next;
}

async function focusReplay(path: string): Promise<void> {
  const index = libraryItems.value.findIndex(
    (item) => item.kind === 'replay' && item.row.path === path,
  );
  if (index < 0) {
    searchInput.value?.focus();
    return;
  }
  replayList.value?.scrollToIndex(index);
  await nextTick();
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  const replayButton = Array.from(
    libraryPanel.value?.querySelectorAll<HTMLButtonElement>('[data-replay-path]') ?? [],
  ).find((button) => button.dataset.replayPath === path);
  (replayButton ?? searchInput.value)?.focus();
}

defineExpose({ focusReplay });

function locationLabel(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

/// Map art is optional. With no folders configured every row simply keeps its
/// procedural tile, so this reads as an offer rather than a warning.
const mapArtSummary = computed(() => {
  const state = props.mapArt;
  if (!state) {
    return 'Checking…';
  }
  if (state.problem) {
    return state.problem;
  }
  if (state.mapCount) {
    return `${state.mapCount} map ${state.mapCount === 1 ? 'image' : 'images'} loaded`;
  }
  if (state.installPath || state.customMapsPath) {
    return 'No map images found';
  }
  return 'Not set up';
});

const artSources = computed(() => {
  const state = props.mapArt;
  return [
    {
      kind: 'install' as ArtSourceKind,
      label: 'Game folder',
      path: state?.installPath ?? null,
      hint: 'The folder holding wic.exe and the wic*.sdf archives.',
    },
    {
      kind: 'customMaps' as ArtSourceKind,
      label: 'Downloaded maps',
      path: state?.customMapsPath ?? null,
      hint: 'Documents/World in Conflict/Downloaded/maps, for community maps.',
    },
  ];
});
</script>

<template>
  <aside ref="libraryPanel" class="library-panel">
    <div class="library-heading">
      <div class="library-brand-lockup" aria-label="WiC Replay Viewer">
        <img
          class="library-brand-logo"
          src="/wicgate-logo.svg"
          alt="WICGATE"
          width="211"
          height="40"
        />
        <span>WiC Replay Viewer</span>
      </div>
    </div>

    <section class="locations-card" :class="{ 'locations-card-open': locationsOpen }">
      <button
        type="button"
        class="locations-toggle"
        :aria-expanded="locationsOpen"
        aria-controls="library-locations"
        @click="locationsOpen = !locationsOpen"
      >
        <span> Library locations </span>
        <span class="locations-count">{{ locations.length }}</span>
        <span class="locations-chevron" aria-hidden="true">⌄</span>
      </button>
      <div v-if="locationsOpen" id="library-locations" class="locations-body">
        <ul v-if="locations.length" class="locations-list">
          <li v-for="path in locations" :key="path">
            <span class="location-name" :title="displayLocationPath(path)">
              <strong>{{ locationLabel(path) }}</strong>
              <span>{{ displayLocationPath(path) }}</span>
            </span>
            <button
              type="button"
              class="location-remove"
              :disabled="importing"
              :aria-label="`Remove ${locationLabel(path)} from library`"
              @click="emit('remove-location', path)"
            >
              ×
            </button>
          </li>
        </ul>
        <p v-else class="locations-hint">
          No folders added yet. Add one below, or drop folders into the window.
        </p>
        <button
          class="locations-add"
          type="button"
          :disabled="importing"
          @click="emit('add-locations')"
        >
          {{ locations.length ? 'Add another location' : 'Add replay folder' }}
        </button>
      </div>
    </section>

    <section v-if="locations.length" class="library-scan-card">
      <p>{{ scanCopy.hint }}</p>
      <button
        :class="[importing ? 'button-secondary' : 'button-primary', 'library-scan-button']"
        type="button"
        :disabled="cancellingImport"
        @click="
          importing
            ? emit('cancel-scan')
            : emit('scan', pendingScanRoots.length ? pendingScanRoots : undefined)
        "
      >
        <span v-if="importing" class="spinner" aria-hidden="true" />
        {{ scanCopy.label }}
      </button>
    </section>

    <section class="locations-card" :class="{ 'locations-card-open': mapArtOpen }">
      <button
        type="button"
        class="locations-toggle"
        :aria-expanded="mapArtOpen"
        aria-controls="library-map-art"
        @click="mapArtOpen = !mapArtOpen"
      >
        <span>Map art</span>
        <span class="locations-count">{{ mapArt?.mapCount || 0 }}</span>
        <span class="locations-chevron" aria-hidden="true">⌄</span>
      </button>
      <div v-if="mapArtOpen" id="library-map-art" class="locations-body">
        <p class="locations-hint" :class="{ 'warning-text': mapArt?.problem }">
          {{ mapArtSummary }}
        </p>
        <ul class="locations-list">
          <li v-for="source in artSources" :key="source.kind">
            <span class="location-name" :title="source.path ?? source.hint">
              <strong>{{ source.label }}</strong>
              <span>{{ source.path ?? 'Not set' }}</span>
            </span>
            <button
              v-if="source.path"
              type="button"
              class="location-remove"
              :aria-label="`Stop using this ${source.label.toLowerCase()}`"
              @click="emit('clear-art-source', source.kind)"
            >
              ×
            </button>
          </li>
        </ul>
        <p class="locations-hint">
          Overviews are read from your own installation at runtime and cached locally. Nothing is
          copied and your game folder is only ever read.
        </p>
        <div class="map-art-actions">
          <button type="button" class="locations-add" @click="emit('choose-art-source', 'install')">
            {{ mapArt?.installPath ? 'Change game folder' : 'Select game folder' }}
          </button>
          <button
            type="button"
            class="locations-add"
            @click="emit('choose-art-source', 'customMaps')"
          >
            {{ mapArt?.customMapsPath ? 'Change maps folder' : 'Select maps folder' }}
          </button>
          <button
            v-if="mapArt?.installPath || mapArt?.customMapsPath"
            type="button"
            class="locations-add"
            @click="emit('rescan-map-art')"
          >
            Rescan
          </button>
        </div>
      </div>
    </section>

    <label class="search-field">
      <span class="sr-only">Search replays</span>
      <span aria-hidden="true">⌕</span>
      <input
        ref="searchInput"
        :value="search"
        type="search"
        placeholder="Player, faction, map…"
        @input="emit('update:search', ($event.target as HTMLInputElement).value)"
      />
      <button
        v-if="search"
        type="button"
        aria-label="Clear search"
        @click="emit('update:search', '')"
      >
        ×
      </button>
    </label>

    <details class="search-help">
      <summary>Search tips</summary>
      <p v-if="stats.stale">
        Refresh the library to enable player/faction matching for older entries.
      </p>
      <p>Search players, factions, and maps together, in any order.</p>
      <p><code>generalx nato riviera</code> finds GeneralX playing NATO on Riviera.</p>
      <p>Factions use USA, NATO, or USSR. Without a player name, either team matches.</p>
      <p>
        Use <code>player:</code>, <code>map:</code>, or <code>faction:</code> to specify a field.
      </p>
      <p>Quote names with spaces: <code>player:"Weed Queen"</code>.</p>
    </details>

    <div v-if="stats.total || search" class="library-meta">
      <span v-if="search">{{ rows.length }} matching</span>
      <span v-else class="library-health">
        <span>{{ stats.parsed }} parsed</span>
        <span :class="{ 'error-text': stats.failed }">{{ stats.failed }} failed</span>
        <span v-if="stats.stale" class="warning-text">{{ stats.stale }} need refresh</span>
      </span>
      <div v-if="exportSelection.size" class="library-selection-actions">
        <span>{{ exportSelection.size }} selected</span>
        <button type="button" @click="emit('export-selected')">Export selected</button>
        <button type="button" @click="clearReplaySelection">Clear</button>
      </div>
    </div>

    <div class="library-results">
      <VirtualList
        v-if="rows.length"
        ref="replayList"
        :items="libraryItems"
        :estimate-size="(item) => (item.kind === 'folder' ? 42 : 112)"
        :item-key="
          (item) => (item.kind === 'folder' ? `folder:${item.path}` : `replay:${item.row.path}`)
        "
      >
        <template #default="{ item }">
          <button
            v-if="item.kind === 'folder'"
            type="button"
            class="replay-folder-heading"
            :title="item.path"
            :aria-expanded="!collapsedFolders.has(item.path)"
            @click="toggleFolder(item.path)"
          >
            <span class="replay-folder-chevron" aria-hidden="true">⌄</span>
            <strong>{{ item.label }}</strong>
            <span>{{ item.count }}</span>
          </button>
          <div v-else class="replay-card-shell">
            <button
              type="button"
              class="replay-card"
              :data-replay-path="item.row.path"
              :class="{
                'replay-card-active': selectedPath === item.row.path,
                'replay-card-selected': exportSelection.has(item.row.path),
                'replay-card-error': item.row.parseError,
              }"
              :aria-pressed="exportSelection.has(item.row.path)"
              @click="selectReplay(item.row, $event)"
            >
              <span class="replay-card-accent" />
              <span class="replay-selection-mark" aria-hidden="true">✓</span>
              <MapTile
                :map-name="item.row.mapName"
                :display-name="item.row.mapDisplayName"
                :faction="item.row.recorderFaction"
              />
              <span class="replay-card-main">
                <span class="replay-title-row">
                  <strong>{{ replayTitle(item.row.fileName) }}</strong>
                  <span v-if="item.row.serverModes" class="replay-server-modes">{{
                    item.row.serverModes
                  }}</span>
                  <span v-if="item.row.format" class="replay-format">{{ item.row.format }}</span>
                  <span v-if="item.row.incomplete" class="status-chip status-muted"
                    >Incomplete</span
                  >
                </span>
                <span v-if="item.row.parseError" class="error-text">Parse failed</span>
                <span v-else-if="item.row.replayName" class="replay-in-game-name">
                  {{ item.row.replayName }}
                </span>
                <span v-if="!item.row.parseError" class="replay-card-data">
                  <span v-if="item.row.recorder" class="replay-recorder-name">
                    {{ item.row.recorder }}
                  </span>
                  <span>{{ item.row.mapDisplayName || item.row.mapName || 'Unknown map' }}</span>
                  <span v-if="item.row.recorderFaction">{{ item.row.recorderFaction }}</span>
                  <span>{{ formatDuration(item.row.recordingSeconds) }}</span>
                </span>
                <span class="replay-card-date">{{ displayDate(item.row.dateTime) }}</span>
              </span>
            </button>
          </div>
        </template>
      </VirtualList>
      <div v-else-if="search || locations.length" class="library-empty-inline">
        <strong>{{ emptyTitle }}</strong>
        <p>{{ emptyMessage }}</p>
        <button v-if="search" type="button" @click="emit('update:search', '')">Clear search</button>
      </div>
    </div>
  </aside>
</template>
