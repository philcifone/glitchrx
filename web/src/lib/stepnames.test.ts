import { describe, expect, it } from 'vitest';
import { numberSteps, stepName } from './stepnames';
import type { StepState } from './types';

const step = (uid: string, effect: string, n?: number): StepState => ({
  uid,
  effect,
  seed: '1',
  params: {},
  enabled: true,
  ...(n === undefined ? {} : { n }),
});

describe('step names', () => {
  it('numbers new steps after the highest in use, per effect', () => {
    const s = numberSteps([step('a', 'echo', 1), step('b', 'echo'), step('c', 'invert'), step('d', 'echo')]);
    expect(s.map((x) => x.n)).toEqual([1, 2, 0, 3]);
    const all = [s[0]];
    expect(numberSteps(all)).toBe(all);
  });

  it('shows the number only when an effect is there more than once', () => {
    const s = [step('a', 'echo', 0), step('b', 'echo', 1), step('c', 'invert', 4)];
    expect(s.map((x) => stepName(x, s))).toEqual(['echo0', 'echo1', 'invert']);
  });
});
