import { afterEach, expect, it, vi } from 'vitest';
import { ImportRecovery } from './importRecovery';
import type { InitialState } from './types';
import { replaySummary } from './test/factories';

const state: InitialState = {
  summaries: [replaySummary()],
  locations: ['/replays'],
  importing: false,
  cachedMapArtCount: 0,
};
afterEach(() => vi.useRealTimers());

it('recovers missed events from SQLite only when idle and stops polling', async () => {
  vi.useFakeTimers();
  const running = vi.fn().mockResolvedValueOnce(true).mockResolvedValue(false);
  const snapshot = vi.fn(async () => state);
  const restore = vi.fn();
  const recovery = new ImportRecovery(running, snapshot, restore, vi.fn());
  recovery.start();
  await vi.advanceTimersByTimeAsync(2000);
  expect(snapshot).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(2000);
  expect(restore).toHaveBeenCalledWith(state);
  await vi.advanceTimersByTimeAsync(10000);
  expect(running).toHaveBeenCalledTimes(2);
});

it('retries failures and snapshots that race with a new backend import', async () => {
  vi.useFakeTimers();
  const report = vi.fn();
  const snapshot = vi
    .fn()
    .mockRejectedValueOnce(new Error('busy'))
    .mockResolvedValueOnce({ ...state, importing: true })
    .mockResolvedValue(state);
  const restore = vi.fn();
  const recovery = new ImportRecovery(async () => false, snapshot, restore, report);
  recovery.start();
  await vi.advanceTimersByTimeAsync(6000);
  expect(report).toHaveBeenCalledTimes(1);
  expect(restore).toHaveBeenCalledTimes(1);
});

it.each(['stop', 'start'] as const)('discards an old snapshot after %s', async (action) => {
  vi.useFakeTimers();
  let resolve!: (value: InitialState) => void;
  const snapshot = vi.fn(
    () =>
      new Promise<InitialState>((yes) => {
        resolve = yes;
      }),
  );
  const restore = vi.fn();
  const recovery = new ImportRecovery(async () => false, snapshot, restore, vi.fn());
  recovery.start();
  await vi.advanceTimersByTimeAsync(2000);
  recovery[action]();
  resolve(state);
  await vi.advanceTimersByTimeAsync(0);
  expect(restore).not.toHaveBeenCalled();
  recovery.stop();
});

it('does not overlap slow heartbeat requests', async () => {
  vi.useFakeTimers();
  let resolve!: (value: boolean) => void;
  const running = vi.fn(
    () =>
      new Promise<boolean>((yes) => {
        resolve = yes;
      }),
  );
  const recovery = new ImportRecovery(running, vi.fn(), vi.fn(), vi.fn());
  recovery.start();
  await vi.advanceTimersByTimeAsync(20000);
  expect(running).toHaveBeenCalledTimes(1);
  recovery.stop();
  resolve(false);
  await vi.advanceTimersByTimeAsync(0);
  expect(vi.getTimerCount()).toBe(0);
});
