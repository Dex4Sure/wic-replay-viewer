// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';

import { replaySummary } from '../test/factories';
import ReplayManagementDialog from './ReplayManagementDialog.vue';

function mountDialog(mode: 'rename' | 'name' | 'export', exportCount = 1, busy = false) {
  return mount(ReplayManagementDialog, {
    props: { mode, summary: replaySummary(), exportCount, busy, error: null },
  });
}

describe('ReplayManagementDialog', () => {
  it('validates and emits a normalized file rename without changing replay metadata', async () => {
    const wrapper = mountDialog('rename');
    expect(wrapper.get('[role="dialog"]').attributes('aria-label')).toBe('Rename replay file');
    const input = wrapper.get('input');
    await input.setValue('  renamed.wicdemo  ');
    await wrapper.get('form').trigger('submit');
    expect(wrapper.emitted('submit')).toEqual([
      [{ fileName: 'renamed.wicdemo', replayName: null, newFolderName: null }],
    ]);
  });

  it('requires an in-game name and preserves its entered text', async () => {
    const wrapper = mountDialog('name');
    const input = wrapper.get('input');
    await input.setValue('');
    expect(wrapper.get('button[type="submit"]').attributes()).toHaveProperty('disabled');
    await input.setValue('Replay title');
    await wrapper.get('form').trigger('submit');
    expect(wrapper.emitted('submit')?.[0]?.[0]).toEqual({
      fileName: 'example.wicdemo',
      replayName: 'Replay title',
      newFolderName: null,
    });
  });

  it('emits the optional batch-export folder and disables controls while busy', async () => {
    const wrapper = mountDialog('export', 3);
    expect(wrapper.text()).toContain('Export 3 replay copies');
    await wrapper.get('input').setValue('Tournament set');
    await wrapper.get('form').trigger('submit');
    expect(wrapper.emitted('submit')?.[0]?.[0]).toMatchObject({ newFolderName: 'Tournament set' });

    await wrapper.setProps({ busy: true });
    expect(wrapper.get('button[type="submit"]').attributes()).toHaveProperty('disabled');
    await wrapper.get('button[type="button"]').trigger('click');
    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('exposes backend failures as an accessible alert', () => {
    const wrapper = mount(ReplayManagementDialog, {
      props: {
        mode: 'export',
        summary: replaySummary(),
        exportCount: 1,
        busy: false,
        error: 'No space',
      },
    });
    expect(wrapper.get('[role="alert"]').text()).toBe('No space');
  });
});
