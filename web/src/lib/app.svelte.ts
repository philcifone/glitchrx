// Application state: the open photo, the recipe being edited, its undo
// history, and the live preview.
//
// Everything the UI shows is reactive ($state). Components call the methods
// here instead of mutating state themselves, so every edit goes through one
// place that schedules a re-render and records history.

import * as api from './api';
import { ApiError, TooHeavyError } from './api';
import { History } from './history';
import { LatestOnly } from './latest';
import { usesSeed } from './seed';
import { shakeParams } from './shake';
import type {
  Channels,
  DocInfo,
  EffectInfo,
  ExportFormat,
  RecipeDto,
  RecipeState,
  ServerConfig,
  StepState,
} from './types';

/** Recipe-wide encoding for new recipes, matching the CLI default. */
export const DEFAULT_ENCODING = 'u8';
/** Step parameter keys that override the recipe's encoding and channels. */
export const ENCODING_KEY = 'encoding';
export const CHANNELS_KEY = 'channels';
/** How long the region band stays on the preview after a drag ends. */
const REGION_LINGER_MS = 900;
/** After the last change, wait this long before a sharper render (SPEC 10.2). */
const IDLE_MS = 250;
const RECIPE_KEY = 'glitchrx.recipe';
/** The open photo's server id, so a reload (iOS discards background tabs
 * often) can pick the same photo up again while the server still has it. */
const DOC_KEY = 'glitchrx.doc';

let uidCounter = 0;
/** Unique within this page. (crypto.randomUUID needs HTTPS; LAN use is HTTP.) */
const newUid = () => `s${Date.now().toString(36)}${(uidCounter++).toString(36)}`;

/** A short, typeable seed, like the CLI's. */
export const randomSeed = () => String(Math.floor(Math.random() * 1_000_000));

const emptyRecipe = (): RecipeState => ({ encoding: DEFAULT_ENCODING, channels: 1, steps: [] });

/**
 * A recipe from storage or a file, in the current shape. Recipes saved by
 * the 2a GUI carry `mode` and `restart` and no encoding; those fields are
 * dropped and the defaults filled in.
 */
function normalize(r: Partial<RecipeState>, known: (effect: string) => boolean): RecipeState {
  return {
    encoding: typeof r.encoding === 'string' ? r.encoding : DEFAULT_ENCODING,
    channels: r.channels === 2 ? 2 : 1,
    steps: (r.steps ?? []).filter((s) => known(s.effect)),
  };
}

/** localStorage can throw (private mode, blocked storage); never let it break the app. */
function storageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function storageSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Persistence is a convenience only.
  }
}

interface Box {
  w: number;
  h: number;
  dpr: number;
}

export class App {
  config = $state<ServerConfig | null>(null);
  effects = $state<EffectInfo[]>([]);
  doc = $state<DocInfo | null>(null);
  recipe = $state<RecipeState>(emptyRecipe());

  /** Blob URLs of the latest glitched preview and of the original. */
  imageUrl = $state<string | null>(null);
  originalUrl = $state<string | null>(null);
  /** Showing the original (before/after). */
  comparing = $state(false);

  busy = $state(false);
  /** A long-running action (upload, export) in progress, for the UI to show. */
  working = $state<string | null>(null);
  renderMs = $state<number | null>(null);
  /** The shown preview is an exact (full-resolution) render. */
  exact = $state(false);
  /** Set when the last preview ran over the time budget: the message. */
  heavy = $state<string | null>(null);
  /** A region being dragged, in percent, for the preview to draw. */
  regionHint = $state<{ s: number; e: number } | null>(null);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);

  canUndo = $state(false);
  canRedo = $state(false);

  private history = new History(JSON.stringify(emptyRecipe()));
  private queue = new LatestOnly(() => (this.busy = false));
  private box: Box = { w: 0, h: 0, dpr: 1 };
  private idleTimer: ReturnType<typeof setTimeout> | undefined;
  private originalKey = '';
  private noticeTimer: ReturnType<typeof setTimeout> | undefined;
  private regionTimer: ReturnType<typeof setTimeout> | undefined;

  /** Load server config and the effect registry, and restore the last recipe. */
  async init(): Promise<void> {
    try {
      const [config, effects] = await Promise.all([api.getConfig(), api.getEffects()]);
      this.config = config;
      this.effects = effects;
    } catch (e) {
      this.fail(e);
      return;
    }
    const saved = storageGet(RECIPE_KEY);
    if (saved) {
      try {
        this.recipe = normalize(JSON.parse(saved), (e) => !!this.effect(e));
        this.history = new History(this.snapshot());
      } catch {
        // A stale or corrupt saved recipe is simply ignored.
      }
    }
    const docId = storageGet(DOC_KEY);
    if (docId && !this.doc) {
      try {
        this.doc = await api.getDoc(docId);
        this.render('full');
      } catch {
        // Closed or the server restarted: start empty.
      }
    }
  }

  effect(id: string): EffectInfo | undefined {
    return this.effects.find((e) => e.id === id);
  }

  /** Whether a new seed would change this step's result. */
  seeded(step: StepState): boolean {
    return usesSeed(step.params, this.effect(step.effect));
  }

  /** Whether "re-roll all" would change anything. */
  get anySeeded(): boolean {
    return this.recipe.steps.some((s) => s.enabled && this.seeded(s));
  }

  // -------------------------------------------------------------------------
  // Opening photos

  async openFile(file: File): Promise<void> {
    await this.opening(file.name, () => api.uploadPhoto(file));
  }

  async openPath(path: string): Promise<void> {
    await this.opening(path, () => api.openServerPhoto(path));
  }

  private async opening(label: string, load: () => Promise<DocInfo>): Promise<void> {
    this.working = `Opening ${label}`;
    try {
      const doc = await load();
      this.doc = doc;
      storageSet(DOC_KEY, doc.id);
      this.setImage(null);
      this.setOriginal(null, '');
      this.heavy = null;
      this.error = null;
      this.render('full');
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  // -------------------------------------------------------------------------
  // Recipe edits. `commit: false` is for live slider drags: the preview
  // follows, but history only records the value where the drag ends.

  /** The recipe as the server wants it: enabled steps only, UI fields dropped. */
  dto(): RecipeDto {
    const r = $state.snapshot(this.recipe);
    return {
      encoding: r.encoding,
      channels: r.channels,
      steps: r.steps
        .filter((s) => s.enabled)
        .map(({ effect, effect_version, seed, params }) => ({
          effect,
          ...(effect_version === undefined ? {} : { effect_version }),
          seed,
          params,
        })),
    };
  }

  private snapshot(): string {
    return JSON.stringify($state.snapshot(this.recipe));
  }

  /** Call after every recipe mutation. */
  changed(commit = true): void {
    this.render('fast');
    if (commit) this.commit();
  }

  /** Record the current recipe in history (no-op if unchanged). */
  commit(): void {
    const snap = this.snapshot();
    this.history.commit(snap);
    this.canUndo = this.history.canUndo;
    this.canRedo = this.history.canRedo;
    storageSet(RECIPE_KEY, snap);
  }

  undo(): void {
    this.restore(this.history.undo());
  }

  redo(): void {
    this.restore(this.history.redo());
  }

  private restore(snap: string | null): void {
    if (snap === null) return;
    this.recipe = JSON.parse(snap) as RecipeState;
    this.canUndo = this.history.canUndo;
    this.canRedo = this.history.canRedo;
    storageSet(RECIPE_KEY, snap);
    this.render('fast');
  }

  private step(uid: string): StepState | undefined {
    return this.recipe.steps.find((s) => s.uid === uid);
  }

  addStep(effect: string): void {
    this.recipe.steps.push({ uid: newUid(), effect, seed: randomSeed(), params: {}, enabled: true });
    this.changed();
  }

  removeStep(uid: string): void {
    this.recipe.steps = this.recipe.steps.filter((s) => s.uid !== uid);
    this.changed();
  }

  moveStep(uid: string, by: -1 | 1): void {
    const steps = this.recipe.steps;
    const i = steps.findIndex((s) => s.uid === uid);
    const j = i + by;
    if (i < 0 || j < 0 || j >= steps.length) return;
    [steps[i], steps[j]] = [steps[j], steps[i]];
    this.changed();
  }

  duplicateStep(uid: string): void {
    const i = this.recipe.steps.findIndex((s) => s.uid === uid);
    if (i < 0) return;
    const copy = { ...$state.snapshot(this.recipe.steps[i]), uid: newUid(), seed: randomSeed() };
    this.recipe.steps.splice(i + 1, 0, copy);
    this.changed();
  }

  toggleStep(uid: string): void {
    const s = this.step(uid);
    if (!s) return;
    s.enabled = !s.enabled;
    this.changed();
  }

  reroll(uid: string): void {
    const s = this.step(uid);
    if (!s) return;
    s.seed = randomSeed();
    this.changed();
  }

  rerollAll(): void {
    // Only steps where the seed matters, so the others keep their seeds
    // (and the recipe does not change for nothing).
    for (const s of this.recipe.steps) if (this.seeded(s)) s.seed = randomSeed();
    this.changed();
  }

  setSeed(uid: string, seed: string): void {
    const s = this.step(uid);
    if (!s || !/^\d+$/.test(seed.trim())) return;
    s.seed = seed.trim();
    this.changed();
  }

  /** Set a parameter; `null` (or the default value) removes the override. */
  setParam(uid: string, name: string, value: string | null, commit = true): void {
    const s = this.step(uid);
    if (!s) return;
    const spec = this.effect(s.effect)?.params.find((p) => p.name === name);
    if (value === null || value.trim() === '' || value.trim() === spec?.default) {
      delete s.params[name];
    } else {
      s.params[name] = value.trim();
    }
    this.changed(commit);
  }

  /** New settings for a step's own parameters; see shake.ts. */
  shake(uid: string): void {
    const s = this.step(uid);
    const info = s && this.effect(s.effect);
    if (!s || !info) return;
    s.params = { ...s.params, ...shakeParams(info, Math.random) };
    this.changed();
  }

  setEncoding(encoding: string): void {
    this.recipe.encoding = encoding;
    this.changed();
  }

  setChannels(channels: Channels): void {
    this.recipe.channels = channels;
    this.changed();
  }

  /** Override a step's encoding (null follows the recipe). */
  setStepEncoding(uid: string, encoding: string | null): void {
    this.setParam(uid, ENCODING_KEY, encoding);
  }

  /** Override a step's channels (null follows the recipe). */
  setStepChannels(uid: string, channels: Channels | null): void {
    this.setParam(uid, CHANNELS_KEY, channels === null ? null : String(channels));
  }

  /** Draw a region band on the preview while it is being dragged. */
  showRegion(s: number, e: number): void {
    clearTimeout(this.regionTimer);
    this.regionHint = { s, e };
  }

  /** Fade the band out a moment after the drag ends. */
  hideRegion(): void {
    clearTimeout(this.regionTimer);
    this.regionTimer = setTimeout(() => (this.regionHint = null), REGION_LINGER_MS);
  }

  clearSteps(): void {
    this.recipe.steps = [];
    this.changed();
  }

  /** Replace the recipe with one loaded from a file. */
  loadRecipe(dto: RecipeDto): void {
    this.recipe = normalize(
      {
        encoding: dto.encoding,
        channels: dto.channels,
        steps: dto.steps.map((s) => ({ ...s, params: { ...s.params }, uid: newUid(), enabled: true })),
      },
      () => true, // the server already checked the effects
    );
    this.changed();
  }

  // -------------------------------------------------------------------------
  // Preview

  /** The preview area's size in CSS pixels, and the device pixel ratio. */
  setViewport(w: number, h: number, dpr: number): void {
    const old = this.box;
    this.box = { w, h, dpr };
    const grew = w * dpr > old.w * old.dpr * 1.1 || h * dpr > old.h * old.dpr * 1.1;
    if (this.doc && grew) this.render('full');
  }

  /** Device-pixel box to render for; `fast` uses CSS pixels (smaller on phones). */
  private target(quality: 'fast' | 'full'): { w: number; h: number } {
    const max = this.config?.max_preview_side ?? 4096;
    const ratio = quality === 'fast' ? 1 : Math.min(this.box.dpr, 3);
    return {
      w: Math.min(max, Math.max(64, Math.round(this.box.w * ratio))),
      h: Math.min(max, Math.max(64, Math.round(this.box.h * ratio))),
    };
  }

  /**
   * Schedule a render. A `fast` render is followed, once things are idle
   * for IDLE_MS, by a `full` one at device resolution (when that differs).
   */
  render(quality: 'fast' | 'full' | 'exact'): void {
    const doc = this.doc;
    if (!doc) return;
    clearTimeout(this.idleTimer);
    const recipe = this.dto();
    const exact = quality === 'exact';
    const { w, h } = this.target(exact ? 'full' : quality);
    this.busy = true;
    if (exact) this.working = 'Rendering at full resolution';
    this.queue.request(() => this.runPreview(doc.id, recipe, w, h, exact));
    if (quality === 'fast' && this.box.dpr > 1) {
      this.idleTimer = setTimeout(() => this.render('full'), IDLE_MS);
    }
    if (this.comparing) void this.loadOriginal();
  }

  private async runPreview(
    docId: string,
    recipe: RecipeDto,
    w: number,
    h: number,
    exact: boolean,
  ): Promise<void> {
    try {
      const res = await api.preview(docId, recipe, w, h, exact);
      if (!res || this.doc?.id !== docId) return; // superseded or switched photo
      const url = URL.createObjectURL(res.blob);
      await preload(url);
      this.setImage(url);
      this.renderMs = res.ms;
      this.exact = exact;
      this.heavy = null;
      this.error = null;
    } catch (e) {
      // Too heavy is not an error: keep the last image and say so.
      if (e instanceof TooHeavyError) this.heavy = e.message;
      else this.fail(e);
    } finally {
      if (exact) this.working = null;
    }
  }

  /** Start or stop showing the original. */
  compare(on: boolean): void {
    this.comparing = on;
    if (on) void this.loadOriginal();
  }

  private async loadOriginal(): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    const { w, h } = this.target('full');
    const key = `${doc.id}:${w}x${h}`;
    if (key === this.originalKey) return;
    this.originalKey = key;
    try {
      const res = await api.preview(doc.id, null, w, h);
      if (!res || this.originalKey !== key) return;
      const url = URL.createObjectURL(res.blob);
      await preload(url);
      this.setOriginal(url, key);
    } catch (e) {
      this.originalKey = '';
      this.fail(e);
    }
  }

  private setImage(url: string | null): void {
    if (this.imageUrl) URL.revokeObjectURL(this.imageUrl);
    this.imageUrl = url;
  }

  private setOriginal(url: string | null, key: string): void {
    if (this.originalUrl) URL.revokeObjectURL(this.originalUrl);
    this.originalUrl = url;
    this.originalKey = key;
  }

  // -------------------------------------------------------------------------
  // Export

  /** Stem used for recipe file names: `photo.jpg` -> `photo-glitch`. */
  private stem(): string {
    const name = this.doc?.name ?? 'glitch';
    return `${name.replace(/\.[^.]*$/, '')}-glitch`;
  }

  async download(format: ExportFormat): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    this.working = 'Rendering full resolution';
    try {
      const { blob, name } = await api.exportFile(doc.id, this.dto(), format);
      api.saveBlob(blob, name);
      this.flash(`Downloaded ${name}`);
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  async saveOnServer(format: ExportFormat, withRecipe: boolean): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    this.working = 'Saving on the server';
    try {
      const saved = await api.saveOnServer(doc.id, this.dto(), format, withRecipe);
      this.flash(`Saved ${saved.join(', ')}`);
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  async downloadRecipe(): Promise<void> {
    try {
      const text = await api.recipeToToml(this.dto());
      api.saveBlob(new Blob([text], { type: 'application/toml' }), `${this.stem()}.recipe.toml`);
    } catch (e) {
      this.fail(e);
    }
  }

  async importRecipe(file: File): Promise<void> {
    try {
      this.loadRecipe(await api.recipeFromToml(await file.text()));
      this.flash(`Loaded ${file.name}`);
    } catch (e) {
      this.fail(e);
    }
  }

  // -------------------------------------------------------------------------
  // Messages

  private fail(e: unknown): void {
    if (e instanceof ApiError && e.status === 404 && this.doc && /no longer open/.test(e.message)) {
      // The server restarted or closed the photo to make room.
      this.doc = null;
      this.setImage(null);
    }
    this.error = e instanceof Error ? e.message : String(e);
  }

  flash(msg: string): void {
    this.notice = msg;
    clearTimeout(this.noticeTimer);
    this.noticeTimer = setTimeout(() => (this.notice = null), 5000);
  }

  dismissError(): void {
    this.error = null;
  }
}

/** Resolve once the image is decoded, so swapping `src` never flashes blank. */
async function preload(url: string): Promise<void> {
  const img = new Image();
  img.src = url;
  try {
    await img.decode();
  } catch {
    // Show it anyway; the <img> will report its own failure.
  }
}

export const app = new App();
