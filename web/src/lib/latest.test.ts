import { describe, expect, it } from 'vitest';
import { LatestOnly } from './latest';

/** A job that finishes when `release()` is called, recording its name. */
function gate(name: string, log: string[]) {
  let release!: () => void;
  const done = new Promise<void>((r) => (release = r));
  return {
    job: async () => {
      log.push(`start ${name}`);
      await done;
      log.push(`end ${name}`);
    },
    release,
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('LatestOnly', () => {
  it('runs one job at a time and only the newest waiting one', async () => {
    const log: string[] = [];
    const q = new LatestOnly();
    const a = gate('a', log);
    const b = gate('b', log);
    const c = gate('c', log);
    q.request(a.job);
    q.request(b.job); // waits
    q.request(c.job); // replaces b
    expect(q.busy).toBe(true);
    a.release();
    await tick();
    c.release();
    await tick();
    expect(log).toEqual(['start a', 'end a', 'start c', 'end c']);
    expect(q.busy).toBe(false);
  });

  it('keeps going after a failing job', async () => {
    const q = new LatestOnly();
    const ran: string[] = [];
    const orig = console.error;
    console.error = () => {};
    q.request(async () => {
      throw new Error('boom');
    });
    q.request(async () => {
      ran.push('second');
    });
    await tick();
    await tick();
    console.error = orig;
    expect(ran).toEqual(['second']);
  });
});
