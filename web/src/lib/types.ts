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
  /** "always", "never", or what it depends on. */
  reversible: string;
  params: ParamInfo[];
}

export interface DocInfo {
  id: string;
  name: string;
  /** Root-relative path when opened from the server; null for uploads. */
  path: string | null;
  width: number;
  height: number;
  encoding: string;
  subsampling: string;
  file_len: number;
  payload_len: number;
  restart_interval: number;
  has_exif_thumbnail: boolean;
  trailing_len: number;
}

export interface ServerConfig {
  version: string;
  browse: boolean;
  root_name: string | null;
  max_preview_side: number;
}

export type Mode = 'safe' | 'raw';

/** A step as sent to the server. Seeds are strings: they are full u64s. */
export interface StepDto {
  effect: string;
  effect_version?: number;
  seed: string;
  params: Record<string, string>;
}

export interface RecipeDto {
  mode: Mode;
  restart: string | null;
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
  mode: Mode;
  restart: string | null;
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

export type ExportFormat = 'jpeg' | 'png' | 'jpeg-baked';

export type DecodeWord = 'clean' | 'recovered' | 'partial' | 'failed';
