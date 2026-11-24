// Finished pictures, remembered by the exact request that made them.
//
// Renders are deterministic (SPEC D-25): the same photo, recipe and size
// always give the same picture. So a picture seen once can come back
// without asking the server again, which is what makes switching a step
// off and on again instant after the first time. Steps run in a chain,
// each on the output of the one before, so there is no per-step layer to
// keep: what is kept is whole results, keyed by everything that decides
// them.
//
// Least recently used first out, within a count and a byte budget. Blobs,
// not object URLs, are kept: the caller makes a URL for each showing and
// may revoke it freely.

export class ResultCache {
  private items = new Map<string, Blob>();
  private bytes = 0;

  constructor(
    private readonly maxItems = 24,
    private readonly maxBytes = 64 << 20,
  ) {}

  get size(): number {
    return this.items.size;
  }

  /** The picture for `key`, now the most recently used, or undefined. */
  get(key: string): Blob | undefined {
    const b = this.items.get(key);
    if (b) {
      this.items.delete(key);
      this.items.set(key, b);
    }
    return b;
  }

  put(key: string, blob: Blob): void {
    const old = this.items.get(key);
    if (old) {
      this.bytes -= old.size;
      this.items.delete(key);
    }
    // A picture larger than the whole budget is not worth keeping.
    if (blob.size > this.maxBytes) return;
    this.items.set(key, blob);
    this.bytes += blob.size;
    // Maps iterate in insertion order: the first key is the oldest.
    while (this.items.size > this.maxItems || this.bytes > this.maxBytes) {
      const [k, b] = this.items.entries().next().value as [string, Blob];
      this.items.delete(k);
      this.bytes -= b.size;
    }
  }

  clear(): void {
    this.items.clear();
    this.bytes = 0;
  }
}
