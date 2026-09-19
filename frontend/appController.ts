import { mergeReplaySummaries, removeReplaySummaries } from './summaryBatch';
import type { ImportEvent, ImportProgress, ReplaySummary } from './types';

export interface ReplayPruneResult {
  summaries: ReplaySummary[];
  exportSelection: Set<string>;
  selectedPath: string | null;
  selectedWasRemoved: boolean;
}

/** Apply backend removal notices consistently to every piece of library selection state. */
export function pruneReplayState(
  summaries: ReplaySummary[],
  exportSelection: ReadonlySet<string>,
  selectedPath: string | null,
  removedPaths: Iterable<string>,
): ReplayPruneResult {
  const removed = new Set(removedPaths);
  const selectedWasRemoved = selectedPath !== null && removed.has(selectedPath);
  return {
    summaries: removeReplaySummaries(summaries, removed),
    exportSelection: new Set([...exportSelection].filter((path) => !removed.has(path))),
    selectedPath: selectedWasRemoved ? null : selectedPath,
    selectedWasRemoved,
  };
}

export interface ImportTransitionState {
  summaries: ReplaySummary[];
  progress: ImportProgress | null;
  importing: boolean;
  cancelling: boolean;
  status: string;
}

/**
 * Small state machine for streamed imports. Summary rows are buffered so a busy
 * scan does not repeatedly rebuild the visible library.
 */
export class ImportEventController {
  private readonly pending = new Map<string, ReplaySummary>();

  pendingSummaries(): Iterable<ReplaySummary> {
    return [...this.pending.values()];
  }

  reset(): void {
    this.pending.clear();
  }

  apply(
    state: ImportTransitionState,
    event: ImportEvent,
    completedSummaries?: ReplaySummary[],
  ): ImportTransitionState {
    switch (event.type) {
      case 'importPrepared': {
        this.pending.clear();
        return {
          ...state,
          progress: { ...event.payload, processed: 0, imported: 0, failed: 0 },
          status: event.payload.pending
            ? `Parsing ${event.payload.pending} of ${event.payload.discovered} discovered replays`
            : `All ${event.payload.discovered} replay summaries are current`,
        };
      }
      case 'summariesStored': {
        const progress = state.progress ? { ...state.progress } : null;
        for (const summary of event.payload) {
          this.pending.set(summary.path, summary);
          if (progress) {
            progress.processed += 1;
            if (summary.parseError) {
              progress.failed += 1;
            } else {
              progress.imported += 1;
            }
          }
        }
        return { ...state, progress };
      }
      case 'importProblem':
        return { ...state, status: event.payload };
      case 'importFinished': {
        const merged =
          completedSummaries ?? mergeReplaySummaries(state.summaries, this.pending.values());
        this.pending.clear();
        const removedPaths = event.payload.removedPaths ?? event.payload.removed_paths ?? [];
        return {
          summaries: completedSummaries ?? removeReplaySummaries(merged, new Set(removedPaths)),
          progress: null,
          importing: false,
          cancelling: false,
          status: event.payload.cancelled
            ? `Scan cancelled: ${event.payload.imported} parsed before stopping`
            : `Import complete: ${event.payload.imported} parsed, ${event.payload.skipped} cached, ${event.payload.failed} failed${removedPaths.length ? `, ${removedPaths.length} missing removed` : ''}`,
        };
      }
    }
  }
}

/** Generation tokens make stale async replay loads unable to overwrite a newer selection. */
export class DetailRequestController {
  private generation = 0;

  begin(): number {
    this.generation += 1;
    return this.generation;
  }

  cancel(): void {
    this.generation += 1;
  }

  isCurrent(token: number): boolean {
    return token === this.generation;
  }
}
