import type { InitialState } from './types';

/** One cheap heartbeat at a time; reload durable results only after Rust is idle. */
export class ImportRecovery {
  private generation = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private readonly running: () => Promise<boolean>,
    private readonly snapshot: () => Promise<InitialState>,
    private readonly restore: (state: InitialState) => void,
    private readonly report: (error: unknown) => void,
  ) {}

  start(): void {
    this.stop();
    this.schedule(this.generation);
  }

  stop(): void {
    this.generation += 1;
    clearTimeout(this.timer);
  }

  private schedule(generation: number): void {
    this.timer = setTimeout(() => void this.check(generation), 2000);
  }

  private async check(generation: number): Promise<void> {
    let restored = false;
    try {
      if (!(await this.running()) && generation === this.generation) {
        const state = await this.snapshot();
        if (generation === this.generation && !state.importing) {
          this.restore(state);
          restored = true;
        }
      }
    } catch (error) {
      if (generation === this.generation) {
        this.report(error);
      }
    }
    if (!restored && generation === this.generation) {
      this.schedule(generation);
    }
  }
}
