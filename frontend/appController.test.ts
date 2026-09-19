import { describe, expect, it } from 'vitest';

import { DetailRequestController, ImportEventController, pruneReplayState } from './appController';
import { replaySummary } from './test/factories';

function initial(summaries = [replaySummary()]): Parameters<ImportEventController['apply']>[0] {
  return { summaries, progress: null, importing: true, cancelling: false, status: '' };
}

describe('ImportEventController', () => {
  it('tracks parsed and failed rows and commits them on completion', () => {
    const controller = new ImportEventController();
    let state = controller.apply(initial([]), {
      type: 'importPrepared',
      payload: { discovered: 3, pending: 2, skipped: 1 },
    });
    state = controller.apply(state, {
      type: 'summariesStored',
      payload: [replaySummary({ path: '/ok' })],
    });
    state = controller.apply(state, {
      type: 'summariesStored',
      payload: [replaySummary({ path: '/bad', parseError: 'invalid' })],
    });
    expect(state.progress).toMatchObject({ processed: 2, imported: 1, failed: 1, skipped: 1 });

    state = controller.apply(state, {
      type: 'importFinished',
      payload: { imported: 1, failed: 1, skipped: 1, removedPaths: [], cancelled: false },
    });
    expect(state.summaries.map((row) => row.path)).toEqual(['/bad', '/ok']);
    expect(state.progress).toBeNull();
    expect(state.importing).toBe(false);
    expect(state.status).toBe('Import complete: 1 parsed, 1 cached, 1 failed');
  });

  it.each(['removedPaths', 'removed_paths'] as const)(
    'accepts %s while pruning completed scans',
    (field) => {
      const controller = new ImportEventController();
      const state = controller.apply(initial([replaySummary({ path: '/gone' })]), {
        type: 'importFinished',
        payload: { imported: 0, failed: 0, skipped: 0, cancelled: false, [field]: ['/gone'] },
      });
      expect(state.summaries).toEqual([]);
      expect(state.status).toContain('1 missing removed');
    },
  );

  it('reports cancellation and problems without leaving scan state active', () => {
    const controller = new ImportEventController();
    const problem = controller.apply(initial(), { type: 'importProblem', payload: 'cannot read' });
    expect(problem.status).toBe('cannot read');
    const cancelled = controller.apply(problem, {
      type: 'importFinished',
      payload: { imported: 4, failed: 0, skipped: 0, cancelled: true },
    });
    expect(cancelled).toMatchObject({ importing: false, cancelling: false, progress: null });
    expect(cancelled.status).toBe('Scan cancelled: 4 parsed before stopping');
  });
});

it('prunes summaries, selected detail identity, and export selection together', () => {
  const rows = [replaySummary({ path: '/keep' }), replaySummary({ path: '/gone' })];
  const result = pruneReplayState(rows, new Set(['/keep', '/gone']), '/gone', ['/gone']);
  expect(result.summaries.map((row) => row.path)).toEqual(['/keep']);
  expect([...result.exportSelection]).toEqual(['/keep']);
  expect(result.selectedPath).toBeNull();
  expect(result.selectedWasRemoved).toBe(true);
});

it('invalidates stale and cancelled asynchronous detail requests', () => {
  const controller = new DetailRequestController();
  const first = controller.begin();
  const second = controller.begin();
  expect(controller.isCurrent(first)).toBe(false);
  expect(controller.isCurrent(second)).toBe(true);
  controller.cancel();
  expect(controller.isCurrent(second)).toBe(false);
});
