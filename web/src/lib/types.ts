// Shapes shared with the server (see crates/glitchrx-serve/src/dto.rs).

export type ParamKind = 'int' | 'float' | 'size' | 'region' | 'choice';

export interface ParamInfo {
  name: string;
  kind: ParamKind;
  min?: number;
  max?: number;
  choices?: string[];
  default: string;
  help: string;
  /** Shared placement parameter (region, hits, len). */
  placement: boolean;
}

/** Picker section. Pixel effects work on whole pixels: no encoding or channels. */
export type EffectGroup = 'audio' | 'data' | 'pixel';

export interface EffectInfo {
  id: string;
  group: EffectGroup;
  version: number;
  summary: string;
  details: string;
  /** The Audacity effect this one follows. */
  audacity: string;
  /** Draws random numbers inside each window, so the seed matters even with hits=0. */
  random_within: boolean;
  /** A scaled preview cannot match the export; render exactly when idle. */
  exact_preview: boolean;
  params: ParamInfo[];
}

export interface DocInfo {
  id: string;
  name: string;
  /** Root-relative path when opened from the server; null for uploads. */
  path: string | null;
  /** Upright size in pixels. */
  width: number;
  height: number;
  format: 'jpeg' | 'png' | 'tiff';
  /** EXIF orientation that was applied (1 = none). */
  orientation: number;
  has_icc: boolean;
  file_len: number;
  /** Raw RGB bytes the effects work on. */
  raw_len: number;
}

/** A sample encoding (Audacity's Import Raw Data choices). */
export interface EncodingInfo {
  id: string;
  /** Bytes per sample. */
  bytes: number;
  describe: string;
}

export interface ServerConfig {
  /** Running in the desktop app: native Open and Save dialogs. */
  desktop: boolean;
  /** Saved presets are available on this server. */
  presets: boolean;
  version: string;
  browse: boolean;
  root_name: string | null;
  max_preview_side: number;
  encodings: EncodingInfo[];
}

/**
 * How many interleaved channels the samples form: mono, stereo, or three
 * (with 8-bit samples, one per colour, so R, G and B are processed alone).
 */
export type Channels = 1 | 2 | 3;

/** How a mask operation combines with the mask so far. */
export type MaskMode = 'add' | 'subtract' | 'intersect';

/**
 * One mask operation (see crates/glitchrx-core/src/mask.rs). Positions are
 * fractions of the picture (x of the width, y of the height); sizes are
 * fractions of its longer side.
 */
export type MaskOp =
  | { shape: 'stroke'; mode: MaskMode; size: number; hardness: number; points: number[] }
  | { shape: 'linear'; mode: MaskMode; from: [number, number]; to: [number, number] }
  /** A circle: centre `from`, edge through `to`. Hard-edged unless `hardness` < 1. */
  | { shape: 'radial'; mode: MaskMode; from: [number, number]; to: [number, number]; hardness?: number }
  | { shape: 'brightness'; mode: MaskMode; low: number; high: number; soft: number };

/** Where a step's result shows. The server omits empty and default fields. */
export interface Mask {
  invert?: boolean;
  feather?: number;
  op?: MaskOp[];
}

/** A step as sent to the server. Seeds are strings: they are full u64s. */
export interface StepDto {
  effect: string;
  effect_version?: number;
  seed: string;
  /** Effect parameters, plus the step keys encoding, channels and opacity. */
  params: Record<string, string>;
  mask?: Mask;
}

export interface RecipeDto {
  /** Recipe-wide encoding and channels; a step may override them in its params. */
  encoding: string;
  channels: Channels;
  steps: StepDto[];
}

/** A step in the editor: the DTO plus UI-only fields. */
export interface StepState extends StepDto {
  /** Stable key for Svelte's keyed each-blocks. */
  uid: string;
  /** Disabled steps stay in the stack but are left out of the render. */
  enabled: boolean;
}

export interface RecipeState {
  encoding: string;
  channels: Channels;
  steps: StepState[];
}

/** A saved preset, as listed. */
export interface PresetInfo {
  name: string;
  steps: number;
  effects: string[];
  /** Seconds since 1970. */
  modified: number;
  /** Why this build cannot use it (an effect version that moved on, say). */
  error?: string;
}

export interface BrowseEntry {
  name: string;
  path: string;
  size: number;
}

export interface Listing {
  path: string;
  parent: string | null;
  dirs: BrowseEntry[];
  files: BrowseEntry[];
}

export type ExportFormat = 'tiff' | 'png' | 'jpeg';
