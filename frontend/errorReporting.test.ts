// @vitest-environment happy-dom
import { createApp } from 'vue';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), native: true }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke, isTauri: () => mocks.native }));
import {
  countImportEvent,
  installErrorReporting,
  reportFrontendError,
  safeCodeLocations,
  setDiagnosticUi,
} from './errorReporting';

let stop: (() => void) | undefined;
beforeEach(() => {
  vi.useFakeTimers();
  mocks.native = true;
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue(undefined);
});
afterEach(() => {
  stop?.();
  stop = undefined;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it('extracts only bounded code locations and never messages, paths or names', () => {
  const error = new Error('Alice: private replay and chat');
  error.stack =
    'Alice private\n at secret (/home/Alice/private.wicdemo:1:2)\n at playerName (tauri://localhost/assets/index-abc.js:42:7)\n at foo (https://example.com/assets/index-abc.js?token=secret:1:2)';
  expect(safeCodeLocations(error)).toEqual([{ asset: 'assets/index-abc.js', line: 42, column: 7 }]);
  expect(safeCodeLocations('secret')).toEqual([]);
  Object.defineProperty(error, 'stack', {
    get() {
      throw new Error('secret');
    },
  });
  expect(safeCodeLocations(error)).toEqual([]);
  const huge = new Error();
  huge.stack = 'message\n' + ' at x (/assets/index-abc.js:1:2)\n'.repeat(1000);
  expect(safeCodeLocations(huge)).toHaveLength(32);
});

it('catches Vue errors, window errors and rejections without forwarding payloads', async () => {
  const app = createApp({});
  const previous = vi.fn();
  app.config.errorHandler = previous;
  stop = installErrorReporting(app);
  await vi.advanceTimersByTimeAsync(1);
  const secret = new Error('/home/alice secret replay');
  app.config.errorHandler!(secret, null, 'render');
  window.dispatchEvent(new ErrorEvent('error', { error: secret }));
  const rejection = new Event('unhandledrejection');
  Object.defineProperty(rejection, 'reason', { value: secret });
  window.dispatchEvent(rejection);
  reportFrontendError('callbackFailed', secret);
  expect(previous).toHaveBeenCalledOnce();
  const calls = mocks.invoke.mock.calls.filter(([name]) => name === 'report_frontend_error');
  expect(calls).toHaveLength(4);
  expect(JSON.stringify(calls)).not.toContain('alice');
  mocks.invoke.mockRejectedValueOnce(secret);
  reportFrontendError('frontendError', secret);
  await Promise.resolve();
  stop();
  expect(app.config.errorHandler).toBe(previous);
});

it('keeps only one heartbeat pending and cancels timers on cleanup', async () => {
  let resolve!: (value: unknown) => void;
  mocks.invoke.mockImplementation(
    () =>
      new Promise((yes) => {
        resolve = yes;
      }),
  );
  countImportEvent();
  stop = installErrorReporting(createApp({}));
  await vi.advanceTimersByTimeAsync(60_000);
  expect(mocks.invoke.mock.calls.filter(([name]) => name === 'diagnostic_heartbeat')).toHaveLength(
    1,
  );
  resolve(undefined);
  await vi.advanceTimersByTimeAsync(2100);
  expect(mocks.invoke.mock.calls.filter(([name]) => name === 'diagnostic_heartbeat')).toHaveLength(
    2,
  );
  stop();
  resolve(undefined);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(mocks.invoke.mock.calls.filter(([name]) => name === 'diagnostic_heartbeat')).toHaveLength(
    2,
  );
});

it('does not install native reporting in a browser preview', () => {
  mocks.native = false;
  reportFrontendError('frontendError');
  expect(installErrorReporting(createApp({}))()).toBeUndefined();
  expect(mocks.invoke).not.toHaveBeenCalled();
});

it('does not recursively report heartbeat or initialization failures', async () => {
  mocks.invoke.mockRejectedValue(new Error('private path'));
  stop = installErrorReporting(createApp({}));
  await vi.advanceTimersByTimeAsync(5000);
  expect(mocks.invoke.mock.calls.some(([name]) => name === 'report_frontend_error')).toBe(false);
  window.dispatchEvent(new Event('pagehide'));
  await vi.advanceTimersByTimeAsync(5000);
});

it('includes only fixed UI context in the heartbeat', async () => {
  setDiagnosticUi({
    view: 'tacticalAid',
    detailPending: true,
    importPending: false,
    managementPending: false,
  });
  stop = installErrorReporting(createApp({}));
  await vi.advanceTimersByTimeAsync(1);
  expect(mocks.invoke).toHaveBeenCalledWith(
    'diagnostic_heartbeat',
    expect.objectContaining({
      ui: {
        view: 'tacticalAid',
        focused: document.hasFocus(),
        detailPending: true,
        importPending: false,
        managementPending: false,
      },
    }),
  );
});
