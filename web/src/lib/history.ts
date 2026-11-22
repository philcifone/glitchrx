// Undo/redo over recipe states.
//
// The history stores whole snapshots (serialized recipes), not edit
// commands. A recipe is a few hundred bytes, so snapshots are cheap, and
// "undo" is simply "go back to the previous recipe": the server re-renders
// it from the original photo. Nothing ever has to be inverted.

export class History {
  private past: string[] = [];
  private future: string[] = [];

  constructor(
    private current: string,
    private readonly limit = 200,
  ) {}

  get present(): string {
    return this.current;
  }

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /** Record a new state. Identical consecutive states are ignored. */
  commit(state: string): boolean {
    if (state === this.current) return false;
    this.past.push(this.current);
    if (this.past.length > this.limit) this.past.shift();
    this.current = state;
    this.future = [];
    return true;
  }

  undo(): string | null {
    const prev = this.past.pop();
    if (prev === undefined) return null;
    this.future.push(this.current);
    this.current = prev;
    return prev;
  }

  redo(): string | null {
    const next = this.future.pop();
    if (next === undefined) return null;
    this.past.push(this.current);
    this.current = next;
    return next;
  }
}
