// Step names: the effect's name, numbered when the stack holds more than
// one of it (echo-0, echo-1, ...). A step keeps its number for life, so it
// stays "echo-1" when moved; a new step takes the next number after the
// highest in use, and numbers start again at 0 once none are left.

import type { StepState } from './types';

/** Give numbers to steps that have none. Returns the same array when nothing changed. */
export function numberSteps<T extends StepState>(steps: T[]): T[] {
  if (steps.every((s) => s.n !== undefined)) return steps;
  const next = new Map<string, number>();
  for (const s of steps) {
    if (s.n !== undefined) next.set(s.effect, Math.max(next.get(s.effect) ?? 0, s.n + 1));
  }
  return steps.map((s) => {
    if (s.n !== undefined) return s;
    const n = next.get(s.effect) ?? 0;
    next.set(s.effect, n + 1);
    return { ...s, n };
  });
}

/** The number shown after the effect's name: only when it is there more than once. */
export function stepNumber(step: StepState, steps: StepState[]): string {
  const twins = steps.reduce((k, s) => k + (s.effect === step.effect ? 1 : 0), 0);
  return twins > 1 ? String(step.n ?? '') : '';
}

/** How a step is labelled in the stack: echo, or echo-0, echo-1, ... */
export function stepName(step: StepState, steps: StepState[]): string {
  const n = stepNumber(step, steps);
  return n ? `${step.effect}-${n}` : step.effect;
}
