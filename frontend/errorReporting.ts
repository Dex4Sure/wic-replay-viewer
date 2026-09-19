import { invoke, isTauri } from '@tauri-apps/api/core';
import type { App } from 'vue';
import type { DetailTab } from './types';

type FrontendCode = 'frontendError' | 'unhandledRejection' | 'vueError' | 'callbackFailed';
export interface CodeLocation {
  asset: string;
  line: number;
  column: number;
}

let received = 0;
let ui = {
  view: 'unknown' as DetailTab | 'library' | 'unknown',
  detailPending: false,
  importPending: false,
  managementPending: false,
};
export function setDiagnosticUi(context: typeof ui): void {
  ui = context;
}

export const countImportEvent = (): void => {
  received += 1;
};

/** Deliberately ignore messages, names, arguments and all unrecognized stack text. */
export function safeCodeLocations(error: unknown): CodeLocation[] {
  const result: CodeLocation[] = [];
  try {
    if (!(error instanceof Error) || typeof error.stack !== 'string') {
      return result;
    }
    const stack = error.stack.slice(0, 8192);
    for (const line of stack.split('\n').slice(1, 65)) {
      const match = /\/(assets\/[A-Za-z0-9_-]+\.js):(\d{1,9}):(\d{1,9})\)?$/.exec(line);
      if (match) {
        result.push({ asset: match[1]!, line: Number(match[2]), column: Number(match[3]) });
      }
      if (result.length === 32) {
        break;
      }
    }
  } catch {
    /* A hostile error getter must not break the application. */
  }
  return result;
}

export function reportFrontendError(code: FrontendCode, error?: unknown): void {
  if (!isTauri()) {
    return;
  }
  void invoke('report_frontend_error', {
    failure: { code, locations: safeCodeLocations(error) },
  }).catch(() => {
    /* Reporting failures never recursively report. */
  });
}

/** Production heartbeat: one invoke and one frame probe in flight, no per-frame traffic. */
export function installErrorReporting(app: App): () => void {
  if (!isTauri()) {
    return () => {};
  }
  let stopped = false;
  let inFlight = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let animation: number | undefined;
  let frame = false;
  const onError = (event: ErrorEvent): void => reportFrontendError('frontendError', event.error);
  const onRejection = (event: PromiseRejectionEvent): void =>
    reportFrontendError('unhandledRejection', event.reason);
  const previous = app.config.errorHandler;
  app.config.errorHandler = (error, instance, info) => {
    reportFrontendError('vueError', error);
    previous?.(error, instance, info);
  };
  const check = async (): Promise<void> => {
    if (stopped || inFlight) {
      return;
    }
    inFlight = true;
    if (animation === undefined && !document.hidden) {
      animation = requestAnimationFrame(() => {
        frame = true;
        animation = undefined;
      });
    }
    const acknowledgedFrame = frame;
    frame = false;
    try {
      await invoke('diagnostic_heartbeat', {
        visible: !document.hidden,
        frame: acknowledgedFrame,
        eventsReceived: received,
        ui: { ...ui, focused: document.hasFocus() },
      });
    } catch {
      /* A stuck bridge is observed by the native watchdog. */
    } finally {
      inFlight = false;
      if (!stopped) {
        timer = setTimeout(() => void check(), 2000);
      }
    }
  };
  window.addEventListener('error', onError);
  window.addEventListener('unhandledrejection', onRejection);
  void check();
  const stop = (): void => {
    stopped = true;
    clearTimeout(timer);
    if (animation !== undefined) {
      cancelAnimationFrame(animation);
    }
    window.removeEventListener('error', onError);
    window.removeEventListener('unhandledrejection', onRejection);
    window.removeEventListener('pagehide', stop);
    app.config.errorHandler = previous;
  };
  window.addEventListener('pagehide', stop);
  return stop;
}
