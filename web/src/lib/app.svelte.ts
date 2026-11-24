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
import { ResultCache } from './results';
import {
  DEFAULT_TOOL,
  MASK_TINT,
  afterRemoval,
  cleanTool,
  emptyMask,
  insertOp,
  migrateMask,
  newComponent,
  pasteOps,
  withMode,
  photoFrame,
  toPhoto,
  type PhotoFrame,
  type ToolState,
} from './mask';
import { usesSamples } from './palette';
import { usesSeed } from './seed';
import { shakeParams } from './shake';
import { storageGet, storageSet } from './storage';
import * as groups from './groups';
import { numberSteps, stepName, stepNumber } from './stepnames';
import { cropRect, fitSize } from './canvas';
import { CHANNELS_KEY, ENCODING_KEY } from './groups';
import { toggleSmooth } from './shapes';
import { FIT, clampView, geometry, isZoomed, toggle, visible, type Geometry, type Point, type Rect, type Size, type View } from './view';
import type {
  Crop,
  SavedRecipe,
  Channels,
  DocInfo,
  EffectInfo,
  ExportOptions,
  FolderSettings,
  Mask,
  MaskMode,
  MaskOp,
  MaskShape,
  Stroke,
  PresetGroup,
  PresetInfo,
  RecipeDto,
  RecipeState,
  ServerConfig,
  StepDto,
  StepState,
} from './types';

/** Recipe-wide encoding for new recipes, matching the CLI default. */
export const DEFAULT_ENCODING = 'u8';
/** Step parameter keys that override the recipe's encoding and channels. */
export { ENCODING_KEY, CHANNELS_KEY };
/** Step key for how strongly the result is blended over the step's input. */
export const OPACITY_KEY = 'opacity';
/** How long the region band stays on the preview after a drag ends. */
const REGION_LINGER_MS = 900;
/** After the last change, wait this long before a sharper render (SPEC 10.2). */
const IDLE_MS = 250;
/** ...or this long before an exact one, for effects that need it. */
const EXACT_IDLE_MS = 600;
/**
 * After a one-off change (a step switched on or off) there is no stream
 * of changes to wait out: the exact render follows almost at once.
 */
const DISCRETE_IDLE_MS = 120;
/** A zoomed view that holds still this long asks for its sharp patch. */
const VIEW_SETTLE_MS = 150;
const RECIPE_KEY = 'glitchrx.recipe';
/** The open photo's server id, so a reload (iOS discards background tabs
 * often) can pick the same photo up again while the server still has it. */
const DOC_KEY = 'glitchrx.doc';
const TOOL_KEY = 'glitchrx.maskTool';
const NEW_PHOTO_KEY = 'glitchrx.newPhoto';

/** What opening another photo does with the steps already there. */
export type NewPhotoChoice = 'ask' | 'keep' | 'fresh';

function loadNewPhoto(): NewPhotoChoice {
  const v = storageGet(NEW_PHOTO_KEY);
  return v === 'keep' || v === 'fresh' ? v : 'ask';
}

function loadTool(): ToolState {
  try {
    return cleanTool(JSON.parse(storageGet(TOOL_KEY) ?? 'null'));
  } catch {
    return { ...DEFAULT_TOOL };
  }
}

let uidCounter = 0;
/** Unique within this page. (crypto.randomUUID needs HTTPS; LAN use is HTTP.) */
const newUid = () => `s${Date.now().toString(36)}${(uidCounter++).toString(36)}`;

/** A short, typeable seed, like the CLI's. */
export const randomSeed = () => String(Math.floor(Math.random() * 1_000_000));

const emptyRecipe = (): RecipeState => ({ encoding: DEFAULT_ENCODING, channels: 1, steps: [] });

/**
 * A recipe from storage or a file, in the current shape. Recipes saved by
 * the 2a GUI carry `mode` and `restart` and no encoding; those fields are
 * dropped and the defaults filled in. Masks from before components had one
 * operation per brush stroke; those become brushes.
 */
function normalize(r: Partial<RecipeState>, effect: (id: string) => EffectInfo | undefined): RecipeState {
  const steps = (r.steps ?? [])
    .filter((s) => effect(s.effect))
    .map((s) => upgrade(s, effect(s.effect)!))
    .map((s) => (s.mask ? { ...s, mask: migrateMask(s.mask) } : s));
  const work = typeof r.work_size === 'number' && r.work_size >= 64 ? Math.round(r.work_size) : undefined;
  return {
    encoding: typeof r.encoding === 'string' ? r.encoding : DEFAULT_ENCODING,
    channels: r.channels === 2 || r.channels === 3 ? r.channels : 1,
    ...(work ? { work_size: work } : {}),
    ...(work && r.scale_back === true ? { scale_back: true } : {}),
    ...(validCrop(r.crop) ? { crop: { ...r.crop } } : {}),
    ...groups.tidy(steps, groups.cleanGroups(r.groups)),
  };
}

/**
 * A step saved with an older version of its effect, made current the way
 * the server does it (D-56): defaults that moved are written in with their
 * old value, so the step keeps its picture.
 */
function upgrade<S extends { effect_version?: number; params: Record<string, string> }>(s: S, info: EffectInfo): S {
  let version = s.effect_version;
  if (version === undefined) return s; // made by this build: current
  let params = s.params;
  for (const up of info.upgrades ?? []) {
    if (version < up.to) {
      params = { ...up.fill, ...params };
      version = up.to;
    }
  }
  return params === s.params ? s : { ...s, params, effect_version: version };
}

interface Box {
  w: number;
  h: number;
  dpr: number;
}

/**
 * The zoomed view's sharp patch: part of a full-resolution render (or of
 * the original) covering what was on screen. `rect` places it, in
 * fractions of the photo; `rev` is the recipe revision it shows.
 */
export interface Patch {
  url: string;
  rect: Rect;
  rev: number;
}

/** A patch's pixel rectangle as fractions of the photo. */
/** A crop that lies inside the photo (from storage or a recipe file). */
function validCrop(c: unknown): c is Crop {
  if (!c || typeof c !== 'object') return false;
  const { x, y, w, h } = c as Record<string, unknown>;
  const ok = (v: unknown): v is number => typeof v === 'number' && v >= 0 && v <= 1;
  return ok(x) && ok(y) && ok(w) && ok(h) && w > 0 && h > 0 && x + w <= 1 + 1e-9 && y + h <= 1 + 1e-9;
}

function fractions(px: { x: number; y: number; w: number; h: number }, size: Size): Rect {
  return { x: px.x / size.w, y: px.y / size.h, w: px.w / size.w, h: px.h / size.h };
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
  /**
   * The photo on the whole screen: the bars and the step panel hidden.
   * Not while masking or cropping, which need the top bar.
   */
  fullView = $state(false);

  busy = $state(false);
  /** A long-running action (upload, export) in progress, for the UI to show. */
  working = $state<string | null>(null);
  renderMs = $state<number | null>(null);
  /** The picture on screen came from the results kept in the browser. */
  remembered = $state(false);
  /** How the whole-picture preview was rendered. */
  private baseQuality = $state<'fast' | 'full' | 'exact'>('fast');
  /** Zoom and pan (SPEC 9.6). Zoom 1 fits the picture in the stage. */
  view = $state<View>(FIT);
  /** Sharp patches for the zoomed view: the glitch, and the original. */
  crop = $state<Patch | null>(null);
  originalCrop = $state<Patch | null>(null);
  /** Set when the last preview ran over the time budget: the message. */
  heavy = $state<string | null>(null);
  /** A region being dragged, in percent, for the preview to draw. */
  regionHint = $state<{ s: number; e: number } | null>(null);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);
  /**
   * A finished export waiting for a tap to open the share sheet. Sharing
   * must start from a tap, and a full-resolution render outlasts the one
   * that started it, so the status bar offers a Save button instead.
   */
  ready = $state<File | null>(null);

  canUndo = $state(false);
  canRedo = $state(false);

  /** Saved presets, as last listed. */
  presets = $state<PresetInfo[]>([]);

  /** The step whose mask is being edited on the preview, by uid. */
  masking = $state<string | null>(null);

  /**
   * Histograms of the picture as it reaches a step, for the curves editor
   * (6b). Measured on the preview, so it is quick; null if it cannot be had.
   */
  async histogram(uid: string): Promise<api.Histogram | null> {
    const doc = this.doc;
    const at = this.recipe.steps.findIndex((s) => s.uid === uid);
    if (!doc || at < 0) return null;
    // The request holds the enabled steps only; count those before it.
    const index = this.recipe.steps.slice(0, at).filter((s) => s.enabled).length;
    const { w, h } = this.target('fast');
    try {
      return await api.histogram(doc.id, this.dto(), index, w, h);
    } catch {
      return null; // too heavy, or the photo went away: no histogram
    }
  }

  /** Step cards folded shut, by step uid (view state: not in history). */
  collapsed = $state<Record<string, boolean>>({});

  toggleCollapsed(uid: string): void {
    if (this.collapsed[uid]) delete this.collapsed[uid];
    else this.collapsed[uid] = true;
  }
  /**
   * The mask component being edited (its index in the mask), or null.
   * Its handles show on the photo, and the photo's gestures act on it.
   */
  activeOp = $state<number | null>(null);
  /**
   * Every selected component (the active one included). Opacity and
   * feather changes, copy and cut apply to all of them; a long press adds
   * to the selection.
   */
  selected = $state<number[]>([]);
  /** Components copied or cut, ready to paste into any step's mask. */
  maskClipboard = $state<MaskOp[]>([]);
  /**
   * The active pen path is still being drawn: taps on the photo add points
   * until the first one is tapped again (or Finish). A path with fewer
   * than three points starts out this way.
   */
  penPlacing = $state(false);
  /** The pen point last touched on the active path, for Remove and Corner / Smooth. */
  penPoint = $state<number | null>(null);
  /** Opening another photo: ask, keep the steps, or start fresh (per device). */
  newPhoto = $state<NewPhotoChoice>(loadNewPhoto());
  /** Steps carried onto a photo just opened, waiting for keep or start fresh (0: not asking). */
  askNewPhoto = $state(0);
  /** Brush and gradient settings for painting masks. */
  tool = $state<ToolState>(loadTool());
  /** Blob URL of the mask overlay for the step being painted. */
  maskUrl = $state<string | null>(null);
  /**
   * Revisions: `revision` counts recipe changes; the others say which
   * revision the shown preview and mask overlay were rendered from. The
   * painter keeps a fresh stroke on screen until a render includes it.
   */
  revision = $state(0);
  imageRevision = $state(0);
  maskRevision = $state(0);

  private history = new History(JSON.stringify(emptyRecipe()));
  private queue = new LatestOnly(() => this.idle());
  /** Exact pictures already seen, so going back to one is instant. */
  private results = new ResultCache();
  /** Crops have their own lane, so a pan never waits behind a render. */
  private cropQueue = new LatestOnly(() => this.idle());
  private maskQueue = new LatestOnly();
  private maskKey = '';
  private box = $state<Box>({ w: 0, h: 0, dpr: 1 });
  private idleTimer: ReturnType<typeof setTimeout> | undefined;
  private viewTimer: ReturnType<typeof setTimeout> | undefined;
  private originalKey = '';
  private cropKey = '';
  private originalCropKey = '';
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
        this.recipe = normalize(JSON.parse(saved), (e) => this.effect(e));
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
  /** Whether a step lands in bands (hits above 0) rather than on the whole picture. */
  banded(step: StepState): boolean {
    const spec = this.effect(step.effect)?.params.find((p) => p.name === 'hits');
    return Number(step.params.hits ?? spec?.default ?? '0') > 0;
  }

  /** Hits a step had before its bands were switched off, to restore them. */
  private lastHits: Record<string, string> = {};

  /**
   * Bands on or off (D-56). Every effect starts on the whole picture;
   * switching bands on scatters it in 4 seeded bands (or the number it had
   * before they were switched off).
   */
  setBands(uid: string, on: boolean): void {
    const s = this.step(uid);
    if (!s) return;
    if (on) {
      this.setParam(uid, 'hits', this.lastHits[uid] ?? '4');
    } else {
      if (s.params.hits) this.lastHits[uid] = s.params.hits;
      this.setParam(uid, 'hits', '0');
    }
  }

  seeded(step: StepState): boolean {
    return usesSeed(step.params, this.effect(step.effect));
  }

  /**
   * Whether the preview should settle to an exact render once changes
   * stop: whenever any step runs. A live preview glitches a screen-sized
   * copy, and pixel-level texture cannot scale with it, so only a
   * full-resolution render shows what the export will be (Phil,
   * 2026-11-22: exports did not match what he had liked on screen). On
   * 20-26 MP photos that render takes about 0.3-1.6 s.
   */
  get needsExact(): boolean {
    return this.recipe.steps.some((s) => s.enabled);
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

  /** Desktop app: pick a photo with the system's Open dialog. */
  async openNative(): Promise<void> {
    let doc: DocInfo | null;
    try {
      doc = await api.desktopOpen();
    } catch (e) {
      this.fail(e);
      return;
    }
    if (doc) await this.opening(doc.name, async () => doc);
  }

  /** Running inside the desktop app, with the system's file dialogs. */
  get desktop(): boolean {
    return this.config?.desktop ?? false;
  }

  private async opening(label: string, load: () => Promise<DocInfo>): Promise<void> {
    this.working = `Opening ${label}`;
    try {
      const doc = await load();
      this.doc = doc;
      this.continueFrom = doc.recipes ?? [];
      storageSet(DOC_KEY, doc.id);
      this.view = FIT;
      this.dropCrops();
      // Steps from the last photo: keep them, clear them, or ask.
      const carried = this.recipe.steps.length;
      if (carried && this.newPhoto === 'fresh') this.startFresh();
      else if (carried && this.newPhoto === 'ask') this.askNewPhoto = carried;
      this.setImage(null);
      this.setOriginal(null, '');
      this.setMaskUrl(null);
      this.maskKey = '';
      this.heavy = null;
      this.error = null;
      this.render('full');
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  /** Clear the steps for a new photo. Undo brings them back. */
  startFresh(): void {
    this.askNewPhoto = 0;
    if (!this.recipe.steps.length) return;
    this.stopMasking();
    this.recipe.steps = [];
    this.changed();
  }

  keepSteps(): void {
    this.askNewPhoto = 0;
  }

  setNewPhotoChoice(choice: NewPhotoChoice): void {
    this.newPhoto = choice;
    storageSet(NEW_PHOTO_KEY, choice);
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
      ...(r.work_size ? { work_size: r.work_size } : {}),
      ...(r.scale_back ? { scale_back: true } : {}),
      ...(r.crop ? { crop: { ...r.crop } } : {}),
      steps: r.steps
        .filter((s) => s.enabled)
        .map(({ effect, effect_version, seed, params, mask }) => ({
          effect,
          ...(effect_version === undefined ? {} : { effect_version }),
          seed,
          params,
          ...(mask ? { mask } : {}),
        })),
    };
  }

  private snapshot(): string {
    return JSON.stringify($state.snapshot(this.recipe));
  }

  /**
   * Call after every recipe mutation. `settle` is how long to wait for
   * more changes before the exact render.
   */
  changed(commit = true, settle = EXACT_IDLE_MS): void {
    this.tidyGroups();
    const numbered = numberSteps(this.recipe.steps);
    if (numbered !== this.recipe.steps) this.recipe.steps = numbered;
    this.revision++;
    this.render('fast', settle);
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
    // Undoing past "Add mask" ends editing; undoing an added component
    // moves the selection to the one before it.
    if (this.masking) {
      const n = this.step(this.masking)?.mask?.op?.length;
      if (n === undefined) this.stopMasking();
      else {
        if (this.activeOp !== null && this.activeOp >= n) this.activeOp = n ? n - 1 : null;
        this.selected = this.selected.filter((i) => i < n);
        if (this.activeOp !== null && !this.selected.includes(this.activeOp)) this.selected.push(this.activeOp);
        const op = this.activeMaskOp;
        if (this.penPoint !== null && (op?.shape !== 'path' || this.penPoint >= op.points.length)) this.penPoint = null;
      }
    }
    this.revision++;
    this.render('fast');
  }

  /** The step's label: its effect, numbered when there are several (echo-0, echo-1). */
  stepName(step: StepState): string {
    return stepName(step, this.recipe.steps);
  }

  /** Just the number part of the label ('' when the effect is there once). */
  stepNumber(step: StepState): string {
    return stepNumber(step, this.recipe.steps);
  }

  step(uid: string): StepState | undefined {
    return this.recipe.steps.find((s) => s.uid === uid);
  }

  addStep(effect: string): void {
    this.recipe.steps.push({ uid: newUid(), effect, seed: randomSeed(), params: {}, enabled: true });
    this.changed();
  }

  removeStep(uid: string): void {
    if (this.masking === uid) this.stopMasking();
    this.recipe.steps = this.recipe.steps.filter((s) => s.uid !== uid);
    this.changed();
  }

  /**
   * Move a step: inside its preset group, or past a neighbouring step or
   * whole group. `commit: false` while dragging; history records the drop.
   * Returns whether it moved.
   */
  moveStep(uid: string, by: -1 | 1, commit = true): boolean {
    const moved = groups.moveStep(this.recipe.steps, uid, by);
    if (!moved) return false;
    this.recipe.steps = moved;
    this.changed(commit);
    return true;
  }

  /** Remove several steps at once (Edit mode). One undo brings them all back. */
  removeSteps(uids: string[]): void {
    const gone = new Set(uids);
    if (this.masking && gone.has(this.masking)) this.stopMasking();
    this.recipe.steps = this.recipe.steps.filter((s) => !gone.has(s.uid));
    this.changed();
  }

  /** Duplicate several steps, each copy right after its original (Edit mode). */
  duplicateSteps(uids: string[]): void {
    const pick = new Set(uids);
    const out: StepState[] = [];
    for (const s of this.recipe.steps) {
      out.push(s);
      if (!pick.has(s.uid)) continue;
      const { n: _n, ...orig } = $state.snapshot(s);
      out.push({ ...orig, uid: newUid(), seed: randomSeed() });
    }
    this.recipe.steps = out;
    this.changed();
  }

  canMoveStep(uid: string, by: -1 | 1): boolean {
    return groups.moveStep(this.recipe.steps, uid, by) !== null;
  }

  duplicateStep(uid: string): void {
    const i = this.recipe.steps.findIndex((s) => s.uid === uid);
    if (i < 0) return;
    const { n: _n, ...orig } = $state.snapshot(this.recipe.steps[i]);
    const copy = { ...orig, uid: newUid(), seed: randomSeed() };
    this.recipe.steps.splice(i + 1, 0, copy);
    this.changed();
  }

  toggleStep(uid: string): void {
    const s = this.step(uid);
    if (!s) return;
    s.enabled = !s.enabled;
    this.changed(true, DISCRETE_IDLE_MS);
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

  /** A step's opacity, 0..1 (1 unless set). */
  opacity(step: StepState): number {
    const v = Number(step.params[OPACITY_KEY]);
    return Number.isFinite(v) && v >= 0 && v <= 1 ? v : 1;
  }

  /** Set a step's opacity; 1 removes the key. */
  setOpacity(uid: string, value: number, commit = true): void {
    const v = Math.round(Math.min(1, Math.max(0, value)) * 100) / 100;
    this.setParam(uid, OPACITY_KEY, v >= 1 ? null : String(v), commit);
  }

  // -------------------------------------------------------------------------
  // Masks. A mask is a stack of components; each stroke, each finished
  // handle drag and each added component is one history entry.

  /**
   * Add a component to a step's mask (creating the mask if needed), make
   * it the active one and start editing.
   */
  addComponent(uid: string, shape: MaskShape): void {
    const s = this.step(uid);
    if (!s) return;
    s.mask ??= emptyMask();
    const ops = s.mask.op ?? [];
    // Laid out on the canvas you see, then kept as shares of the photo.
    const c = this.canvasSize;
    const aspect = c ? c.w / c.h : 1;
    const made = newComponent(shape, aspect, ops.length);
    const f = this.maskFrame;
    const { ops: next, index } = insertOp(ops, f ? toPhoto(made, f) : made);
    s.mask.op = next;
    if (shape === 'brush') this.setTool({ tool: 'paint' });
    this.changed();
    this.startMasking(uid, index);
  }

  /** Make a component the active one, and the only one selected. */
  selectComponent(uid: string, index: number): void {
    if (this.masking !== uid) this.startMasking(uid, index);
    else {
      this.activeOp = index;
      this.selected = [index];
      this.resetPen();
    }
  }

  /** The component being edited, if any. */
  get activeMaskOp(): MaskOp | null {
    if (this.activeOp === null) return null;
    return this.maskTarget?.step.mask?.op?.[this.activeOp] ?? null;
  }

  /** A newly active path with too few points to enclose anything carries on being drawn. */
  private resetPen(): void {
    const op = this.activeMaskOp;
    this.penPoint = null;
    this.penPlacing = op?.shape === 'path' && op.points.length < 3;
  }

  /** Stop adding points to the path being drawn. */
  finishPen(): void {
    this.penPlacing = false;
  }

  /** Change the active path's points (a drag, or a point added). */
  setPenPoints(points: number[][], commit = true): void {
    if (!this.masking || this.activeOp === null || this.activeMaskOp?.shape !== 'path') return;
    this.updateMaskOp(this.masking, this.activeOp, { points }, commit);
  }

  /** Remove the picked point from the active path. */
  removePenPoint(): void {
    const op = this.activeMaskOp;
    const i = this.penPoint;
    if (op?.shape !== 'path' || i === null || !op.points[i]) return;
    this.setPenPoints(op.points.filter((_, k) => k !== i));
    this.penPoint = null;
  }

  /** Switch the picked point between a corner and a smooth point. */
  togglePenSmooth(): void {
    const op = this.activeMaskOp;
    const i = this.penPoint;
    if (op?.shape !== 'path' || i === null || !op.points[i] || !this.doc) return;
    // Handles are laid out in the photo's own proportions (masks are
    // shares of the whole photo, D-52).
    this.setPenPoints(toggleSmooth($state.snapshot(op.points) as number[][], i, this.doc.width, this.doc.height));
  }

  /**
   * Add a component to the selection, or take it out (long press, or
   * shift / cmd / ctrl click). The active one stays put unless it is the
   * one taken out.
   */
  toggleSelected(uid: string, index: number): void {
    if (this.masking !== uid) return this.startMasking(uid, index);
    if (!this.selected.includes(index)) {
      this.selected = [...this.selected, index].sort((a, b) => a - b);
      this.activeOp ??= index;
      return;
    }
    this.selected = this.selected.filter((i) => i !== index);
    if (this.activeOp === index) {
      this.activeOp = this.selected.at(-1) ?? null;
      this.resetPen();
    }
  }

  /** The components that copy, cut and the sliders act on. */
  private targets(uid: string): number[] {
    const n = this.step(uid)?.mask?.op?.length ?? 0;
    if (this.masking !== uid) return [];
    return this.selected.filter((i) => i < n);
  }

  /** Set opacity or feather on every selected component. */
  setOpsBlend(uid: string, patch: { opacity?: number; feather?: number }, commit = true): void {
    const ops = this.step(uid)?.mask?.op;
    if (!ops) return;
    for (const i of this.targets(uid)) {
      const op = ops[i];
      // Defaults are left out, so recipes stay short.
      if (patch.opacity !== undefined) {
        if (patch.opacity >= 1) delete op.opacity;
        else op.opacity = patch.opacity;
      }
      if (patch.feather !== undefined) {
        if (patch.feather <= 0) delete op.feather;
        else op.feather = patch.feather;
      }
    }
    this.changed(commit);
  }

  /** Copy the selected components (or, when not editing, the whole mask). */
  copyMaskOps(uid: string): void {
    const ops = this.step(uid)?.mask?.op ?? [];
    const pick = this.masking === uid ? this.targets(uid) : ops.map((_, i) => i);
    this.maskClipboard = $state.snapshot(pick.map((i) => ops[i])) as MaskOp[];
  }

  /** Copy the selected components, then remove them: with paste, a move. */
  cutMaskOps(uid: string): void {
    const pick = this.targets(uid);
    if (!pick.length) return;
    this.copyMaskOps(uid);
    this.removeMaskOps(uid, pick);
  }

  /** Paste copied components into a step's mask and select them. */
  pasteMaskOps(uid: string): void {
    const s = this.step(uid);
    if (!s || !this.maskClipboard.length) return;
    s.mask ??= emptyMask();
    // A plain copy: structuredClone cannot copy the reactive proxies.
    const { ops, indices } = pasteOps(s.mask.op ?? [], $state.snapshot(this.maskClipboard) as MaskOp[]);
    s.mask.op = ops;
    this.changed();
    this.startMasking(uid, indices[0]);
    this.selected = indices;
  }

  /** Change mask-wide settings (invert, feather). */
  updateMask(uid: string, patch: Partial<Mask>, commit = true): void {
    const s = this.step(uid);
    if (!s?.mask) return;
    Object.assign(s.mask, patch);
    this.changed(commit);
  }

  /** Add a stroke to a brush component. */
  addStroke(uid: string, index: number, stroke: Stroke): void {
    const op = this.step(uid)?.mask?.op?.[index];
    if (op?.shape !== 'brush') return;
    op.stroke.push(stroke);
    this.changed();
  }

  /** Change one component's values (a radial's ends, a brightness range). */
  updateMaskOp(uid: string, index: number, patch: Partial<MaskOp>, commit = true): void {
    const ops = this.step(uid)?.mask?.op;
    if (!ops?.[index]) return;
    Object.assign(ops[index], patch);
    this.changed(commit);
  }

  setMaskOpMode(uid: string, index: number, mode: MaskMode): void {
    const m = this.step(uid)?.mask;
    if (!m?.op?.[index]) return;
    const { ops, order } = withMode(m.op, index, mode);
    m.op = ops;
    if (this.masking === uid) {
      if (this.activeOp !== null) this.activeOp = order.indexOf(this.activeOp);
      this.selected = this.selected.map((i) => order.indexOf(i)).sort((a, b) => a - b);
    }
    this.changed();
  }

  /**
   * Remove components. Removing the last one removes the mask itself (an
   * empty mask would hide the step everywhere), which ends editing.
   */
  removeMaskOps(uid: string, indices: number[]): void {
    const s = this.step(uid);
    const m = s?.mask;
    if (!s || !m?.op) return;
    m.op = m.op.filter((_, i) => !indices.includes(i));
    if (m.op.length === 0) {
      if (this.masking === uid) this.stopMasking();
      delete s.mask;
    } else if (this.masking === uid) {
      const before = this.activeOp;
      this.activeOp = afterRemoval(this.activeOp, indices);
      if (this.activeOp === null || before === null || indices.includes(before)) this.resetPen();
      this.selected = this.selected.flatMap((i) => {
        const j = afterRemoval(i, indices);
        return j === null ? [] : [j];
      });
    }
    this.changed();
  }

  /** The step being edited and its number in the stack (from 1). */
  get maskTarget(): { n: number; step: StepState } | null {
    const i = this.recipe.steps.findIndex((s) => s.uid === this.masking);
    return i < 0 ? null : { n: i + 1, step: this.recipe.steps[i] };
  }

  /**
   * Edit this step's mask on the preview, with component `index` active
   * (by default the last one, usually the newest).
   */
  startMasking(uid: string, index?: number): void {
    const n = this.step(uid)?.mask?.op?.length ?? 0;
    this.masking = uid;
    this.activeOp = index ?? (n ? n - 1 : null);
    this.selected = this.activeOp === null ? [] : [this.activeOp];
    this.resetPen();
    this.maskKey = '';
    this.refreshMask();
  }

  stopMasking(): void {
    this.masking = null;
    this.activeOp = null;
    this.selected = [];
    this.penPlacing = false;
    this.penPoint = null;
    this.maskKey = '';
    this.setMaskUrl(null);
  }

  setTool(patch: Partial<ToolState>): void {
    Object.assign(this.tool, patch);
    storageSet(TOOL_KEY, JSON.stringify($state.snapshot(this.tool)));
    if (patch.show !== undefined) {
      this.maskKey = '';
      if (patch.show) this.refreshMask();
      else this.setMaskUrl(null);
    }
  }

  /** The revision the painter should compare its fresh strokes with. */
  get paintedRevision(): number {
    return this.tool.show && this.maskUrl ? this.maskRevision : this.imageRevision;
  }

  /**
   * Fetch the overlay for the step being painted. The mask comes from the
   * server, so what is shown is exactly what renders; the request shares
   * the preview's cache for the picture arriving at the step.
   */
  private refreshMask(): void {
    const doc = this.doc;
    const uid = this.masking;
    if (!doc || !uid || !this.tool.show) return;
    const recipe = this.dto();
    // The step's position among the steps sent (enabled ones only).
    const index = this.recipe.steps.filter((s) => s.enabled).findIndex((s) => s.uid === uid);
    if (index < 0) {
      this.setMaskUrl(null);
      return;
    }
    const { w, h } = this.target('fast');
    const key = JSON.stringify([doc.id, index, w, h, recipe]);
    if (key === this.maskKey) return;
    this.maskKey = key;
    const rev = this.revision;
    this.maskQueue.request(async () => {
      try {
        const blob = await api.maskView(doc.id, recipe, index, w, h, MASK_TINT);
        if (this.doc?.id !== doc.id || this.masking !== uid) return;
        const url = URL.createObjectURL(blob);
        await preload(url);
        this.setMaskUrl(url);
        this.maskRevision = rev;
      } catch (e) {
        // A mask too heavy to show live is not worth an error banner.
        if (!(e instanceof ApiError && e.status === 503)) this.fail(e);
        this.maskKey = '';
      }
    });
  }

  private setMaskUrl(url: string | null): void {
    if (this.maskUrl) URL.revokeObjectURL(this.maskUrl);
    this.maskUrl = url;
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

  /**
   * Replace the recipe with one loaded from a file, or with a preset
   * (`preset` names it; its steps then form a group).
   */
  loadRecipe(dto: RecipeDto, preset?: string): void {
    this.stopMasking(); // every step is replaced
    const group = preset ? this.newGroup(preset, dto) : undefined;
    this.recipe = normalize(
      {
        encoding: dto.encoding,
        channels: dto.channels,
        work_size: dto.work_size,
        scale_back: dto.scale_back,
        crop: dto.crop,
        steps: dto.steps.map((s) => ({
          ...s,
          params: { ...s.params },
          uid: newUid(),
          enabled: true,
          ...(group ? { group: group.id } : {}),
        })),
        groups: group ? [group] : [],
      },
      (e) => this.effect(e), // the server already checked and upgraded them
    );
    this.changed();
  }

  /** Add a recipe's steps after the current ones; from a preset, as a group. */
  appendRecipe(dto: RecipeDto, preset?: string): void {
    // The added steps keep the sample format they were saved with, even if
    // this recipe's differs. (Pixel effects have none to keep.)
    const group = preset ? this.newGroup(preset, dto) : undefined;
    for (const s of dto.steps) {
      this.recipe.steps.push({
        ...groups.stackForm(s, dto, this.recipe, this.samples),
        uid: newUid(),
        enabled: true,
        ...(group ? { group: group.id } : {}),
      });
    }
    if (group) this.recipe.groups = [...(this.recipe.groups ?? []), group];
    this.changed();
  }

  // -------------------------------------------------------------------------
  // Preset groups (SPEC 4e): see groups.ts.

  /** Whether an effect reads samples (and so can have its encoding pinned). */
  private samples = (effect: string) => usesSamples(this.effect(effect));

  /** A group for a preset's steps; its base is the preset as saved. */
  private newGroup(preset: string, dto: RecipeDto): PresetGroup {
    return {
      id: newUid(),
      preset,
      encoding: dto.encoding,
      channels: dto.channels,
      base: dto.steps.map((s) => groups.presetForm(s, dto, dto, this.samples)),
    };
  }

  /** Drop group marks that no longer hold (a step removed, the stack cleared). */
  private tidyGroups(): void {
    const t = groups.tidy(this.recipe.steps, this.recipe.groups);
    if (t.steps.some((s, i) => s !== this.recipe.steps[i])) this.recipe.steps = t.steps;
    if (t.groups.length !== (this.recipe.groups?.length ?? 0)) this.recipe.groups = t.groups;
  }

  groupOf(id: string): PresetGroup | undefined {
    return this.recipe.groups?.find((g) => g.id === id);
  }

  /** The group's steps differ from the preset's settings. */
  groupModified(id: string): boolean {
    const g = this.groupOf(id);
    return !!g && groups.isModified(g, this.recipe.steps, this.recipe, this.samples);
  }

  moveGroup(id: string, by: -1 | 1, commit = true): boolean {
    const moved = groups.moveGroup(this.recipe.steps, id, by);
    if (!moved) return false;
    this.recipe.steps = moved;
    this.changed(commit);
    return true;
  }

  canMoveGroup(id: string, by: -1 | 1): boolean {
    return groups.moveGroup(this.recipe.steps, id, by) !== null;
  }

  /** Put the preset's steps back as they were added, in place. Undoable. */
  revertGroup(id: string): void {
    const g = this.groupOf(id);
    const steps = this.recipe.steps;
    const i = steps.findIndex((s) => s.group === id);
    if (!g || i < 0) return;
    const [a, b] = groups.blockAt(steps, i);
    if (this.masking && steps.slice(a, b).some((s) => s.uid === this.masking)) this.stopMasking();
    const fresh = g.base.map((s) => ({
      ...groups.stackForm($state.snapshot(s) as StepDto, g, this.recipe, this.samples),
      uid: newUid(),
      enabled: true,
      group: id,
    }));
    this.recipe.steps = [...steps.slice(0, a), ...fresh, ...steps.slice(b)];
    this.changed();
  }

  /** Keep the steps but drop the label. Undoable. */
  ungroup(id: string): void {
    this.recipe.groups = (this.recipe.groups ?? []).filter((g) => g.id !== id);
    this.changed();
  }

  /**
   * Write the group's steps (the ones switched on) to its preset, after
   * the user confirms, and make them the group's new base.
   */
  async updatePreset(id: string): Promise<void> {
    const g = this.groupOf(id);
    if (!g) return;
    const steps = groups.currentForm(g, this.recipe.steps, this.recipe, this.samples);
    if (!steps.length) {
      this.flash('Turn a step on first: a preset needs at least one');
      return;
    }
    const n = steps.length === 1 ? '1 step' : `${steps.length} steps`;
    if (!confirm(`Replace the preset "${g.preset}" with these ${n}?`)) return;
    try {
      const recipe: RecipeDto = { encoding: g.encoding, channels: g.channels, steps: $state.snapshot(steps) as StepDto[] };
      await api.savePreset(g.preset, recipe, true);
      g.base = recipe.steps;
      this.changed();
      this.flash(`Updated preset ${g.preset}`);
      await this.loadPresets();
    } catch (e) {
      this.fail(e);
    }
  }

  // -------------------------------------------------------------------------
  // Presets: recipes saved on the server, shared by every device using it.

  async loadPresets(): Promise<void> {
    if (!this.config?.presets) return;
    try {
      this.presets = await api.listPresets();
    } catch (e) {
      this.fail(e);
    }
  }

  /** Use a preset: replace the recipe, or add its steps (`append`). Undoable. */
  async applyPreset(name: string, append = false): Promise<void> {
    try {
      const dto = await api.getPreset(name);
      if (append) this.appendRecipe(dto, name);
      else this.loadRecipe(dto, name);
      this.flash(`${append ? 'Added' : 'Loaded'} ${name}`);
    } catch (e) {
      this.fail(e);
    }
  }

  /**
   * Save the recipe as a preset. A taken name is replaced only after the
   * user confirms. Returns true once saved.
   */
  async savePreset(name: string): Promise<boolean> {
    const recipe = this.dto();
    try {
      let saved: string;
      try {
        saved = await api.savePreset(name, recipe, false);
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 409)) throw e;
        if (!confirm(`Replace the preset "${name.trim()}"?`)) return false;
        saved = await api.savePreset(name, recipe, true);
      }
      this.flash(`Saved preset ${saved}`);
      await this.loadPresets();
      return true;
    } catch (e) {
      this.fail(e);
      return false;
    }
  }

  async deletePreset(name: string): Promise<void> {
    try {
      await api.deletePreset(name);
      await this.loadPresets();
    } catch (e) {
      this.fail(e);
    }
  }

  // -------------------------------------------------------------------------
  // Preview

  /** The preview area's size in CSS pixels, and the device pixel ratio. */
  setViewport(w: number, h: number, dpr: number): void {
    const old = this.box;
    this.box = { w, h, dpr };
    const grew = w * dpr > old.w * old.dpr * 1.1 || h * dpr > old.h * old.dpr * 1.1;
    if (this.zoomed) this.setView(this.view); // keep it valid; a new patch follows
    else if (this.doc && grew) this.render('full');
  }

  // -------------------------------------------------------------------------
  // Zoom (SPEC 9.6)

  /** The stage and photo sizes the zoom works with, once both are known. */
  get geometry(): Geometry | null {
    const c = this.canvasSize;
    if (!c || this.box.w <= 0 || this.box.h <= 0) return null;
    return geometry(this.box, c, this.box.dpr);
  }

  // -------------------------------------------------------------------------
  // Work size (SPEC 5b): the canvas the steps run on.

  /** The size of the crop (the whole photo without one). */
  get croppedSize(): Size | null {
    const d = this.doc;
    if (!d) return null;
    const c = cropRect(this.recipe.crop, d.width, d.height);
    return { w: c.w, h: c.h };
  }

  /** How masks (shares of the whole photo) sit on the cropped canvas; null without a crop. */
  get maskFrame(): PhotoFrame | null {
    const d = this.doc;
    if (!d || !this.recipe.crop) return null;
    return photoFrame(cropRect(this.recipe.crop, d.width, d.height), d.width, d.height);
  }

  /** The size the steps run on: the crop, shrunk to the work size. 100% zoom is this. */
  get canvasSize(): Size | null {
    const c = this.croppedSize;
    if (!c) return null;
    const n = this.recipe.work_size;
    const [w, h] = n ? fitSize(c.w, c.h, n, n) : [c.w, c.h];
    return { w, h };
  }

  /** The exported file's size: the canvas, or scaled back to the crop's. */
  get outputSize(): Size | null {
    return this.recipe.scale_back ? this.croppedSize : this.canvasSize;
  }

  // -------------------------------------------------------------------------
  // Crop (SPEC 5d): a frame over the whole, unbent photo; Done applies it.

  /** The crop being edited, as shares of the photo, and the photo to edit it on. */
  cropping = $state<{ rect: Crop; url: string | null } | null>(null);

  async startCrop(): Promise<void> {
    const doc = this.doc;
    if (!doc || this.cropping) return;
    if (this.masking) this.stopMasking();
    if (this.zoomed) this.toggleZoom();
    this.cropping = { rect: { ...(this.recipe.crop ?? { x: 0, y: 0, w: 1, h: 1 }) }, url: null };
    // The whole photo, unbent: the steps run on the crop, so they cannot
    // be shown outside it.
    const { crop: _c, work_size: _w, ...rest } = this.dto();
    const { w, h } = this.target('full');
    try {
      const res = await api.preview(doc.id, rest, w, h, false, undefined, true);
      if (!res || !this.cropping) return;
      const url = URL.createObjectURL(res.blob);
      await preload(url);
      if (this.cropping) this.cropping.url = url;
      else URL.revokeObjectURL(url);
    } catch (e) {
      this.cancelCrop();
      this.fail(e);
    }
  }

  setCropRect(r: Crop): void {
    if (this.cropping) this.cropping.rect = r;
  }

  private endCrop(): void {
    if (this.cropping?.url) URL.revokeObjectURL(this.cropping.url);
    this.cropping = null;
  }

  cancelCrop(): void {
    this.endCrop();
  }

  /** Apply the frame (the whole photo removes the crop). One undo step. */
  applyCrop(): void {
    const c = this.cropping;
    const d = this.doc;
    if (!c || !d) return this.endCrop();
    const px = cropRect(c.rect, d.width, d.height);
    const whole = px.w === d.width && px.h === d.height;
    const old = JSON.stringify(this.recipe.crop ?? null);
    // Shares rounded to 6 places: plenty for 100 megapixels, and a tidy recipe.
    const r6 = (v: number) => Math.round(v * 1e6) / 1e6;
    const next = whole ? null : { x: r6(c.rect.x), y: r6(c.rect.y), w: r6(c.rect.w), h: r6(c.rect.h) };
    this.endCrop();
    if (JSON.stringify(next) === old) return;
    if (next) this.recipe.crop = next;
    else delete this.recipe.crop;
    this.changed();
  }

  /** Choose the work size (none: full). Undoable, like any recipe change. */
  setWorkSize(n: number | null): void {
    if ((this.recipe.work_size ?? null) === n) return;
    if (n === null) delete this.recipe.work_size;
    else this.recipe.work_size = n;
    this.changed();
  }

  setScaleBack(on: boolean): void {
    if (on) this.recipe.scale_back = true;
    else delete this.recipe.scale_back;
    this.changed();
  }

  get zoomed(): boolean {
    return isZoomed(this.view);
  }

  /** What is on screen is exactly the export: an exact render, or a current patch when zoomed. */
  get exact(): boolean {
    if (this.zoomed) return this.crop !== null && this.crop.rev >= this.imageRevision;
    return this.baseQuality === 'exact';
  }

  /**
   * Move or zoom the preview. Zoomed in, a sharp patch of what is on
   * screen is asked for once the view holds still; back at fit, the whole
   * picture settles as it would after a change.
   */
  setView(v: View): void {
    const g = this.geometry;
    if (!g) return;
    const was = this.zoomed;
    this.view = clampView(v, g);
    clearTimeout(this.viewTimer);
    if (this.zoomed) {
      this.viewTimer = setTimeout(() => this.requestCrop(), VIEW_SETTLE_MS);
    } else if (was) {
      this.dropCrops();
      const want = this.needsExact ? 'exact' : 'full';
      const current = this.imageRevision === this.revision;
      if (!current || (this.baseQuality !== 'exact' && this.baseQuality !== want)) this.render(want);
    }
  }

  /** The magnifier, a double tap or the Z key: fit to 100% and back, about `p` (default: the middle). */
  toggleZoom(p?: Point): void {
    const g = this.geometry;
    if (g) this.setView(toggle(this.view, p ?? { x: g.box.w / 2, y: g.box.h / 2 }, g));
  }

  /** The part of the photo on screen and the device-pixel box for its patch, when zoomed. */
  private cropTarget(): { crop: Rect; w: number; h: number } | null {
    const g = this.geometry;
    if (!g || !this.zoomed) return null;
    const { crop, screen } = visible(this.view, g);
    const max = this.config?.max_preview_side ?? 4096;
    const ratio = Math.min(this.box.dpr, 3);
    const side = (css: number) => Math.min(max, Math.max(16, Math.round(css * ratio)));
    return { crop, w: side(screen.w), h: side(screen.h) };
  }

  /** Ask for the sharp, exact patch covering the zoomed view. */
  private requestCrop(): void {
    clearTimeout(this.viewTimer);
    const doc = this.doc;
    const t = this.cropTarget();
    if (!doc || !t) return;
    if (this.comparing) void this.loadOriginalCrop();
    const recipe = this.dto();
    const key = JSON.stringify([doc.id, t, recipe]);
    if (key === this.cropKey) return;
    this.cropKey = key;
    const rev = this.revision;
    this.busy = true;
    this.cropQueue.request(() => this.runCrop(doc, recipe, t, rev));
  }

  private async runCrop(
    doc: DocInfo,
    recipe: RecipeDto,
    t: { crop: Rect; w: number; h: number },
    rev: number,
  ): Promise<void> {
    try {
      const res = await api.preview(doc.id, recipe, t.w, t.h, true, t.crop);
      if (!res?.crop || this.doc?.id !== doc.id) return;
      const url = URL.createObjectURL(res.blob);
      await preload(url);
      // Zoomed out meanwhile, or a newer change is already on screen.
      if (!this.zoomed || rev < this.imageRevision || this.doc?.id !== doc.id) {
        URL.revokeObjectURL(url);
        return;
      }
      const size = this.canvasSize;
      if (!size) return;
      this.setPatch('crop', { url, rect: fractions(res.crop, size), rev });
      this.renderMs = res.ms;
      this.heavy = null;
    } catch (e) {
      this.cropKey = '';
      if (e instanceof TooHeavyError) this.heavy = e.message;
      else this.fail(e);
    }
  }

  /** The original's patch, for comparing while zoomed. */
  private async loadOriginalCrop(): Promise<void> {
    const doc = this.doc;
    const t = this.cropTarget();
    if (!doc || !t) return;
    const key = JSON.stringify([doc.id, t, this.recipe.work_size, this.recipe.crop]);
    if (key === this.originalCropKey) return;
    this.originalCropKey = key;
    try {
      const res = await api.preview(doc.id, this.dto(), t.w, t.h, false, t.crop, true);
      if (!res?.crop || this.originalCropKey !== key || !this.canvasSize) return;
      const url = URL.createObjectURL(res.blob);
      await preload(url);
      this.setPatch('originalCrop', { url, rect: fractions(res.crop, this.canvasSize), rev: 0 });
    } catch (e) {
      this.originalCropKey = '';
      this.fail(e);
    }
  }

  private setPatch(which: 'crop' | 'originalCrop', patch: Patch | null): void {
    const old = this[which];
    if (old) URL.revokeObjectURL(old.url);
    this[which] = patch;
  }

  private dropCrops(): void {
    clearTimeout(this.viewTimer);
    this.setPatch('crop', null);
    this.setPatch('originalCrop', null);
    this.cropKey = '';
    this.originalCropKey = '';
  }

  private idle(): void {
    this.busy = this.queue.busy || this.cropQueue.busy;
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
  render(quality: 'fast' | 'full' | 'exact', settle = EXACT_IDLE_MS): void {
    const doc = this.doc;
    if (!doc) return;
    clearTimeout(this.idleTimer);
    // Zoomed in, settling means a sharp exact patch of what is on screen.
    if (quality === 'exact' && this.zoomed) return this.requestCrop();
    const recipe = this.dto();
    // A picture seen before (a step switched off and on again) is shown
    // at once, already exact: nothing to render.
    if (!this.zoomed && this.needsExact && this.showRemembered(doc.id, recipe)) return;
    const exact = quality === 'exact';
    const { w, h } = this.target(exact ? 'full' : quality);
    const rev = this.revision;
    // A canvas no larger than the preview (a small work size) is shown
    // whole, unshrunk: that is already exactly the export.
    const c = this.canvasSize;
    const whole = !!c && c.w <= w && c.h <= h;
    // The progress line shows while it works; no message, since settling
    // to exact happens after every change.
    this.busy = true;
    this.queue.request(() => this.runPreview(doc.id, recipe, w, h, quality, rev, whole));
    if (this.masking) this.refreshMask();
    if (!exact && this.zoomed) {
      // The whole picture stays approximate behind the patch.
      this.idleTimer = setTimeout(() => this.requestCrop(), EXACT_IDLE_MS);
    } else if (!exact && this.needsExact && !whole) {
      // Approximate while things change; exact once they stop.
      this.idleTimer = setTimeout(() => this.render('exact'), settle);
    } else if (quality === 'fast' && this.box.dpr > 1) {
      this.idleTimer = setTimeout(() => this.render('full'), IDLE_MS);
    }
    if (this.comparing) void this.loadOriginal();
  }

  private async runPreview(
    docId: string,
    recipe: RecipeDto,
    w: number,
    h: number,
    quality: 'fast' | 'full' | 'exact',
    rev: number,
    whole = false,
  ): Promise<void> {
    try {
      const res = await api.preview(docId, recipe, w, h, quality === 'exact');
      if (!res || this.doc?.id !== docId) return; // superseded or switched photo
      // A newer picture is already showing (one remembered, below).
      if (rev < this.imageRevision) return;
      if (quality === 'exact' || whole) this.results.put(this.resultKey(docId, recipe), res.blob);
      const url = URL.createObjectURL(res.blob);
      await preload(url);
      this.setImage(url);
      this.imageRevision = rev;
      this.renderMs = res.ms;
      this.remembered = false;
      this.baseQuality = whole ? 'exact' : quality;
      this.heavy = null;
      this.error = null;
    } catch (e) {
      // Too heavy is not an error: keep the last image and say so.
      if (e instanceof TooHeavyError) this.heavy = e.message;
      else this.fail(e);
    }
  }

  /**
   * What decides an exact picture of the whole view: the photo, the size
   * it is rendered at, and the recipe (with its framing and masks).
   */
  private resultKey(docId: string, recipe: RecipeDto): string {
    const { w, h } = this.target('full');
    return JSON.stringify([docId, w, h, recipe]);
  }

  /** Show a remembered exact picture of `recipe`, if there is one. */
  private showRemembered(docId: string, recipe: RecipeDto): boolean {
    const blob = this.results.get(this.resultKey(docId, recipe));
    if (!blob) return false;
    this.setImage(URL.createObjectURL(blob));
    this.imageRevision = this.revision;
    this.baseQuality = 'exact';
    this.remembered = true;
    this.heavy = null;
    this.error = null;
    if (this.masking) this.refreshMask();
    if (this.comparing) void this.loadOriginal();
    return true;
  }

  /** Start or stop showing the original. */
  compare(on: boolean): void {
    this.comparing = on;
    if (on) {
      void this.loadOriginal();
      if (this.zoomed) void this.loadOriginalCrop();
    }
  }

  private async loadOriginal(): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    const { w, h } = this.target('full');
    const key = JSON.stringify([doc.id, w, h, this.recipe.work_size, this.recipe.crop]);
    if (key === this.originalKey) return;
    this.originalKey = key;
    try {
      const res = await api.preview(doc.id, this.dto(), w, h, false, undefined, true);
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

  /** What the status bar says while an export renders and encodes. */
  private exporting(opts: ExportOptions): string {
    return opts.format === 'avif' ? 'Rendering and encoding AVIF (slow)' : 'Rendering full resolution';
  }

  async download(opts: ExportOptions): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    this.working = this.exporting(opts);
    try {
      const { blob, name } = await api.exportFile(doc.id, this.saved(), opts);
      this.deliver(blob, name);
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  /** Desktop app: the system's Save dialog, then a full-resolution render. */
  async saveAs(opts: ExportOptions, withRecipe: boolean): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    this.working = this.exporting(opts);
    try {
      const r = await api.saveAs(doc.id, this.saved(), opts, withRecipe);
      if (r) {
        const names = r.saved.map((p) => p.split('/').pop());
        this.flash(`Saved ${names.join(', ')}${r.skipped.length ? `; ${r.skipped.join('; ')}` : ''}`);
      }
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  async saveOnServer(opts: ExportOptions, withRecipe: boolean): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    this.working = this.exporting(opts);
    try {
      const saved = await api.saveOnServer(doc.id, this.saved(), opts, withRecipe);
      this.flash(`Saved ${saved.join(', ')}`);
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  /** Save As into a folder under the server's home folder (never overwrites). */
  async saveTo(opts: ExportOptions, folder: string, name: string, withRecipe: boolean): Promise<void> {
    const doc = this.doc;
    if (!doc) return;
    this.working = this.exporting(opts);
    try {
      const saved = await api.saveTo(doc.id, this.saved(), opts, folder, name, withRecipe);
      this.flash(`Saved ${saved.join(', ')}`);
    } catch (e) {
      this.fail(e);
    } finally {
      this.working = null;
    }
  }

  // -------------------------------------------------------------------------
  // Settings: the library and export folders (on the server, for every
  // device), loaded when the sheet opens.

  folderSettings = $state<FolderSettings | null>(null);

  async loadSettings(): Promise<void> {
    if (!this.config?.settings) return;
    try {
      this.folderSettings = await api.getSettings();
    } catch (e) {
      this.fail(e);
    }
  }

  /** Choose a folder. A new library is what Open browses from now on. */
  async setFolder(which: 'library' | 'export', path: string | null): Promise<void> {
    const cur = this.folderSettings ?? { library: null, export: null };
    try {
      this.folderSettings = await api.putSettings({ ...cur, [which]: path });
      if (which === 'library') this.config = await api.getConfig();
    } catch (e) {
      this.fail(e);
    }
  }

  async downloadRecipe(): Promise<void> {
    try {
      const text = await api.recipeToToml(this.saved());
      this.deliver(new Blob([text], { type: 'application/toml' }), `${this.stem()}.recipe.toml`);
    } catch (e) {
      this.fail(e);
    }
  }

  /**
   * The recipe as saved beside an export or downloaded: with the photo's
   * name, so opening that photo again offers to continue from it (D-51).
   * Not for presets, which belong to no photo.
   */
  private saved(): RecipeDto {
    return { ...this.dto(), ...(this.doc ? { source: this.doc.name } : {}) };
  }

  // -------------------------------------------------------------------------
  // Picking up old edits (SPEC 5e)

  /** Recipes made on the open photo, offered once it opens; [] once answered. */
  continueFrom = $state<SavedRecipe[]>([]);

  /** Load a recipe saved on the server (replacing the steps; undoable). */
  async loadSaved(r: SavedRecipe): Promise<void> {
    try {
      this.loadRecipe(await api.readRecipe(r));
      this.continueFrom = [];
      this.flash(`Loaded ${r.name}`);
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
      this.view = FIT;
      this.dropCrops();
    }
    this.error = e instanceof Error ? e.message : String(e);
  }

  /** Download a file, or offer it to the share sheet on a phone. */
  private deliver(blob: Blob, name: string): void {
    const file = new File([blob], name, { type: blob.type });
    if (api.shareFiles(file)) {
      this.ready = file;
    } else {
      api.saveBlob(blob, name);
      this.flash(`Downloaded ${name}`);
    }
  }

  /** Open the share sheet for the waiting export (call from a tap). */
  async share(): Promise<void> {
    const file = this.ready;
    if (!file) return;
    try {
      await navigator.share({ files: [file] });
      this.ready = null;
    } catch (e) {
      // Closing the sheet is not an error; keep the file for another try.
      if (!(e instanceof DOMException && e.name === 'AbortError')) this.fail(e);
    }
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
