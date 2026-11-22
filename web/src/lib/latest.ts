// Latest-wins job runner (SPEC 10.2).
//
// While a slider moves, changes arrive far faster than the server can render.
// Queueing every one would make the preview lag further and further behind;
// cancelling the in-flight request would mean nothing ever finishes while
// the finger keeps moving. The middle ground: at most one job runs, and at
// most one waits. A new request *replaces* the waiting one, so when the
// running job finishes, the next thing rendered is always the newest state.
// Intermediate results still show, which keeps the preview feeling live.

export class LatestOnly {
  private running = false;
  private waiting: (() => Promise<void>) | null = null;

  /** `onIdle` is called each time the last job finishes. */
  constructor(private readonly onIdle?: () => void) {}

  /** True while a job is running. */
  get busy(): boolean {
    return this.running;
  }

  /** Run `job` now, or after the current one, replacing any waiting job. */
  request(job: () => Promise<void>): void {
    this.waiting = job;
    if (!this.running) void this.pump();
  }

  private async pump(): Promise<void> {
    this.running = true;
    try {
      while (this.waiting) {
        const job = this.waiting;
        this.waiting = null;
        try {
          await job();
        } catch (e) {
          // Jobs report their own errors; never let one stop the pump.
          console.error(e);
        }
      }
    } finally {
      this.running = false;
      this.onIdle?.();
    }
  }
}
