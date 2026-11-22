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

export interface EffectInfo {
  id: string;
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

/** A step as sent to the server. Seeds are strings: they are full u64s. */
export interface StepDto {
  effect: string;
  effect_version?: number;
  seed: string;
  params: Record<string, string>;
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
