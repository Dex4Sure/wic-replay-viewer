<script setup lang="ts">
import { computed, ref, watch } from 'vue';

import { replayFileName, replayTitle } from '../format';
import type { ReplaySummary } from '../types';

export type ReplayManagementMode = 'rename' | 'name' | 'export';

const props = defineProps<{
  mode: ReplayManagementMode;
  summary: ReplaySummary;
  exportCount: number;
  busy: boolean;
  error: string | null;
}>();

const emit = defineEmits<{
  close: [];
  submit: [payload: { fileName: string; replayName: string | null; newFolderName: string | null }];
}>();

const fileNameInput = ref('');
const fileName = computed({
  get: () => fileNameInput.value,
  set: (value: string) => {
    fileNameInput.value = replayTitle(value);
  },
});
const replayName = ref('');
const newFolderName = ref('');

watch(
  () => [props.mode, props.summary.path] as const,
  () => {
    fileName.value = replayTitle(props.summary.fileName);
    replayName.value = props.summary.replayName ?? '';
    newFolderName.value = '';
  },
  { immediate: true },
);

const title = computed(
  () =>
    ({
      rename: 'Rename replay file',
      name: 'Edit in-game replay name',
      export:
        props.exportCount === 1
          ? 'Export replay copy'
          : `Export ${props.exportCount} replay copies`,
    })[props.mode],
);

const action = computed(
  () =>
    ({
      rename: 'Rename file',
      name: 'Save in-game name',
      export:
        props.exportCount === 1
          ? 'Choose destination'
          : newFolderName.value.trim()
            ? 'Choose parent folder'
            : 'Choose folder',
    })[props.mode],
);

const canSubmit = computed(() => {
  if (props.busy) {
    return false;
  }
  if (props.mode === 'rename') {
    return Boolean(fileName.value.trim());
  }
  if (props.mode === 'name') {
    return Boolean(replayName.value.trim());
  }
  return props.exportCount > 0;
});

function submit(): void {
  if (!canSubmit.value) {
    return;
  }
  emit('submit', {
    fileName: replayFileName(fileName.value.trim()),
    replayName: props.mode === 'name' ? replayName.value : null,
    newFolderName:
      props.mode === 'export' && props.exportCount > 1 && newFolderName.value.trim()
        ? newFolderName.value.trim()
        : null,
  });
}
</script>

<template>
  <div class="management-overlay" role="presentation" @mousedown.self="!busy && emit('close')">
    <form
      class="management-dialog"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
      @submit.prevent="submit"
    >
      <div>
        <p class="eyebrow">Replay management</p>
        <h2>{{ title }}</h2>
      </div>

      <p v-if="mode === 'rename'" class="management-copy">
        This renames the original .wicdemo file in its current folder. The name stored inside the
        replay is left untouched.
      </p>
      <p v-else-if="mode === 'name'" class="management-copy">
        The replay is rebuilt into a verified temporary file before the original is replaced.
      </p>
      <p v-else class="management-copy">
        <template v-if="exportCount === 1">
          Export creates an exact, validated .wicdemo copy and never changes the source replay.
        </template>
        <template v-else>
          Each replay keeps its file name and contents. Existing files are never overwritten.
        </template>
      </p>

      <label v-if="mode === 'rename'" class="management-field">
        <span>File name</span>
        <input v-model="fileName" autofocus autocomplete="off" spellcheck="false" />
        <small
          >The replay extension is kept automatically. Rename stops if another file already uses
          that name.</small
        >
      </label>

      <label v-if="mode === 'name'" class="management-field">
        <span>In-game name</span>
        <input v-model="replayName" autofocus autocomplete="off" maxlength="49" />
        <small>{{ replayName.length }} / 49 characters</small>
      </label>

      <label v-if="mode === 'export' && exportCount > 1" class="management-field">
        <span>New folder name <small>(optional)</small></span>
        <input v-model="newFolderName" autocomplete="off" maxlength="120" />
        <small>
          Leave empty to use the selected folder directly, or enter a name to create it inside the
          selected parent folder.
        </small>
      </label>

      <p v-if="error" class="management-error" role="alert">{{ error }}</p>

      <div class="management-actions">
        <button type="button" class="button-secondary" :disabled="busy" @click="emit('close')">
          Cancel
        </button>
        <button type="submit" class="button-primary" :disabled="!canSubmit">
          <span v-if="busy" class="spinner" aria-hidden="true" />
          {{ busy ? 'Working…' : action }}
        </button>
      </div>
    </form>
  </div>
</template>
