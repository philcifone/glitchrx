// Shapes shared with the server (see crates/glitchrx-serve/src/dto.rs).

export type ParamKind = 'int' | 'float' | 'size' | 'region' | 'choice' | 'curve';

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
export type EffectGroup = 'audio' | 'data' | 'pixel' | 'adjust';

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
  /** The colour profile's name, if it has one. */
  profile: string | null;
  /** An AVIF export keeps these colours (no profile, sRGB or Display P3). */
  avif_keeps_profile: boolean;
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
  /** The Settings sheet can choose library and export folders. */
  settings: boolean;
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

/** One stroke of a brush component. */
export interface Stroke {
  /** Take back this brush's own paint instead of adding to it. */
  erase?: boolean;
  /** Diameter as a share of the picture's longer side. */
  size: number;
  hardness: number;
  /** Flat x, y list, as fractions of the picture. */
  points: number[];
}

/**
 * One mask component (see crates/glitchrx-core/src/mask.rs). Positions are
 * fractions of the picture (x of the width, y of the height); sizes are
 * fractions of its longer side.
 */
export type MaskOp = MaskCommon &
  (
    | { shape: 'brush'; stroke: Stroke[] }
    | { shape: 'linear'; from: [number, number]; to: [number, number] }
    /** A circle: centre `from`, edge through `to`. Hard-edged unless `hardness` < 1. */
    | { shape: 'radial'; from: [number, number]; to: [number, number]; hardness?: number }
    /** A rectangle: centre (fractions), size (shares of the longer side), clockwise turn in degrees. */
    | { shape: 'rect'; center: [number, number]; size: [number, number]; angle?: number }
    /** A closed pen outline: [x, y] corners and [x, y, hx, hy] smooth points (see lib/shapes.ts). */
    | { shape: 'path'; points: number[][] }
    | { shape: 'brightness'; low: number; high: number; soft: number }
  );

/** What every mask component has besides its shape. */
export interface MaskCommon {
  mode: MaskMode;
  /** How strongly it counts, 0..1 (default 1). */
  opacity?: number;
  /** Blur of its edges, as a share of the longer side (default 0). */
  feather?: number;
}

export type MaskShape = MaskOp['shape'];

/** Where a step's result shows. The server omits empty and default fields. */
export interface Mask {
  invert?: boolean;
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
  /** The longest side of the canvas the steps run on (none: full size). */
  work_size?: number;
  /** Enlarge the result back to full size, with hard pixel edges. */
  scale_back?: boolean;
  steps: StepDto[];
}

/** A step in the editor: the DTO plus UI-only fields. */
export interface StepState extends StepDto {
  /** Stable key for Svelte's keyed each-blocks. */
  uid: string;
  /** Disabled steps stay in the stack but are left out of the render. */
  enabled: boolean;
  /** The preset group this step came in with (see groups.ts). */
  group?: string;
  /** Its number among steps of the same effect (echo-0, echo-1; see stepnames.ts). */
  n?: number;
}

/**
 * Steps added from a preset, kept together under its name. `base` is the
 * preset's steps as added, written for its own `encoding` and `channels`.
 */
export interface PresetGroup {
  id: string;
  preset: string;
  encoding: string;
  channels: Channels;
  base: StepDto[];
}

export interface RecipeState {
  encoding: string;
  channels: Channels;
  /** See RecipeDto. */
  work_size?: number;
  scale_back?: boolean;
  steps: StepState[];
  /** Preset groups; UI only, never sent to the server or saved in recipes. */
  groups?: PresetGroup[];
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

export type ExportFormat = 'tiff' | 'png' | 'jpeg' | 'webp' | 'avif';

/** An export's format and, for the lossy ones, how to encode it. */
export interface ExportOptions {
  format: ExportFormat;
  /** JPEG, lossy WebP and AVIF, 1 to 100. */
  quality?: number;
  /** WebP: lossless. */
  lossless?: boolean;
  /** AVIF: 1 (slowest, smallest) to 10 (fastest). */
  speed?: number;
}

/** The Settings sheet's folders, relative to the server's home folder. */
export interface FolderSettings {
  library: string | null;
  export: string | null;
  /** Set when --root decides what Open browses (its path, for display). */
  fixed_root?: string | null;
}

/** Folders inside a folder under home, for the folder picker. */
export interface FolderListing {
  path: string;
  parent: string | null;
  dirs: { name: string; path: string }[];
}
