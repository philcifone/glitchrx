// Thin fetch wrappers around the glitchrx serve API. Same origin only, so the
// access-token cookie (if any) rides along automatically.

import type {
  DocInfo,
  EffectInfo,
  ExportFormat,
  ExportOptions,
  FolderListing,
  FolderSettings,
  Listing,
  PresetInfo,
  RecipeDto,
  SavedRecipe,
  ServerConfig,
} from './types';

/** An error from the server, with its HTTP status. */
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

/** A preview that ran over the server's time budget (SPEC 9.2). */
export class TooHeavyError extends ApiError {}

/** Save As found a project of that name, which it may update if asked. */
export class ReplaceableError extends ApiError {}

async function fail(res: Response): Promise<never> {
  let msg = `${res.status} ${res.statusText}`;
  try {
    const body = await res.json();
    if (body && typeof body.error === 'string') msg = body.error;
  } catch {
    // Not JSON (for example the token page); keep the status line.
  }
  throw new ApiError(res.status, msg);
}

async function json<T>(res: Response): Promise<T> {
  if (!res.ok) await fail(res);
  return (await res.json()) as T;
}

function postJson(url: string, body: unknown, signal?: AbortSignal): Promise<Response> {
  return fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
}

export const getConfig = async () => json<ServerConfig>(await fetch('/api/config'));
export const getEffects = async () => json<EffectInfo[]>(await fetch('/api/effects'));

export async function uploadPhoto(file: File): Promise<DocInfo> {
  const res = await fetch(`/api/docs?name=${encodeURIComponent(file.name)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/octet-stream' },
    body: file,
  });
  return json<DocInfo>(res);
}

/** Info about a photo that is still open on the server (404 if not). */
export async function getDoc(id: string): Promise<DocInfo> {
  return json<DocInfo>(await fetch(`/api/docs/${encodeURIComponent(id)}`));
}

export async function openServerPhoto(path: string): Promise<DocInfo> {
  return json<DocInfo>(await postJson('/api/docs/open', { path }));
}

export async function browse(path: string): Promise<Listing> {
  return json<Listing>(await fetch(`/api/browse?path=${encodeURIComponent(path)}`));
}

export const thumbUrl = (path: string) => `/api/thumb?path=${encodeURIComponent(path)}`;

export interface PreviewResult {
  blob: Blob;
  width: number;
  height: number;
  ms: number;
  /** For a crop: the full-resolution pixels it shows (x, y, w, h). */
  crop: { x: number; y: number; w: number; h: number } | null;
}

/**
 * Render a preview covering a `maxW` x `maxH` box (device pixels). With
 * `exact`, the server bends the full-resolution photo and then shrinks it,
 * which is exactly the export but slower. Resolves to null when the server
 * dropped it for a newer request; throws TooHeavyError over the budget.
 * With `crop` (fractions of the photo), only that part, cut from the
 * full-resolution render (or the original): the zoomed view's patch.
 */
function parseCrop(v: string | null): PreviewResult['crop'] {
  const n = (v ?? '').split(',').map(Number);
  return n.length === 4 && n.every(Number.isFinite) ? { x: n[0], y: n[1], w: n[2], h: n[3] } : null;
}

export async function preview(
  docId: string,
  recipe: RecipeDto,
  maxW: number,
  maxH: number,
  exact = false,
  crop?: { x: number; y: number; w: number; h: number },
  original = false,
): Promise<PreviewResult | null> {
  // The original comes with the recipe too: its work size frames it.
  const body = {
    recipe,
    ...(original ? { original: true } : { exact }),
    max_w: maxW,
    max_h: maxH,
    ...(crop ? { crop } : {}),
  };
  const res = await postJson(`/api/docs/${docId}/preview`, body);
  if (res.status === 204) return null;
  if (res.headers.get('x-too-heavy')) {
    let msg = 'too heavy to preview live';
    try {
      msg = (await res.json()).error ?? msg;
    } catch {
      // keep the default message
    }
    throw new TooHeavyError(res.status, msg);
  }
  if (!res.ok) await fail(res);
  const h = res.headers;
  return {
    blob: await res.blob(),
    width: Number(h.get('x-width') ?? 0),
    height: Number(h.get('x-height') ?? 0),
    ms: Number(h.get('x-render-ms') ?? 0),
    crop: parseCrop(h.get('x-crop')),
  };
}

/**
 * A step's mask at preview size, as a PNG of `tint` with the mask as alpha,
 * for laying over the preview. `step` counts the steps sent, from 0.
 */
export async function maskView(
  docId: string,
  recipe: RecipeDto,
  step: number,
  maxW: number,
  maxH: number,
  tint: [number, number, number],
): Promise<Blob> {
  const res = await postJson(`/api/docs/${docId}/mask`, { recipe, step, max_w: maxW, max_h: maxH, tint });
  if (!res.ok) await fail(res);
  return res.blob();
}

/** Counts per level (256 each) of the picture as it reaches a step. */
export interface Histogram {
  red: number[];
  green: number[];
  blue: number[];
  luma: number[];
}

export async function histogram(docId: string, recipe: RecipeDto, step: number, maxW: number, maxH: number): Promise<Histogram> {
  return json<Histogram>(await postJson(`/api/docs/${docId}/histogram`, { recipe, step, max_w: maxW, max_h: maxH }));
}

const presetUrl = (name: string) => `/api/presets/${encodeURIComponent(name)}`;

export const listPresets = async () => json<PresetInfo[]>(await fetch('/api/presets'));

export const getPreset = async (name: string) => json<RecipeDto>(await fetch(presetUrl(name)));

/** Save a preset. Throws ApiError 409 when the name is taken and not `replace`. */
export async function savePreset(name: string, recipe: RecipeDto, replace: boolean): Promise<string> {
  const res = await fetch(presetUrl(name), {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ recipe, replace }),
  });
  return (await json<{ name: string }>(res)).name;
}

export async function deletePreset(name: string): Promise<void> {
  const res = await fetch(presetUrl(name), { method: 'DELETE' });
  if (!res.ok) await fail(res);
}

/**
 * A finished file waiting on the server for a few minutes. Downloads go
 * through these rather than `blob:` URLs: Firefox on iOS names a blob
 * download after its random id and ignores the name asked for.
 */
export interface DownloadLink {
  url: string;
  name: string;
}

/** Full-resolution export, held behind a download link. */
export async function exportLink(docId: string, recipe: RecipeDto, opts: ExportOptions): Promise<DownloadLink> {
  return json<DownloadLink>(await postJson(`/api/docs/${docId}/export`, { recipe, ...opts, link: true }));
}

/** A small file made here (a recipe), held behind a download link. */
export async function linkFor(blob: Blob, name: string): Promise<DownloadLink> {
  const res = await fetch(`/api/downloads?name=${encodeURIComponent(name)}`, {
    method: 'POST',
    headers: { 'Content-Type': blob.type || 'application/octet-stream' },
    body: blob,
  });
  return json<DownloadLink>(res);
}

/** The file behind a link, for the share sheet. */
export async function fetchLink(link: DownloadLink): Promise<File> {
  const res = await fetch(link.url);
  if (!res.ok) await fail(res);
  const blob = await res.blob();
  return new File([blob], link.name, { type: blob.type });
}

/** What an export is, as a MIME type (to ask the share sheet before fetching). */
export function mimeOf(opts: ExportOptions): string {
  if (opts.project) return 'application/vnd.glitcharaxia.project';
  return `image/${opts.format}`;
}

/** Save next to the original on the server; returns the paths written. */
export async function saveOnServer(
  docId: string,
  recipe: RecipeDto,
  opts: ExportOptions,
  withRecipe: boolean,
): Promise<string[]> {
  const res = await postJson(`/api/docs/${docId}/save`, {
    recipe,
    ...opts,
    with_recipe: withRecipe,
  });
  return (await json<{ saved: string[] }>(res)).saved;
}

/**
 * Save As into a folder under the server's home folder; refuses to
 * overwrite (409). A project of the same name throws ReplaceableError,
 * and is updated when saved again with `replace`.
 */
export async function saveTo(
  docId: string,
  recipe: RecipeDto,
  opts: ExportOptions,
  folder: string,
  name: string,
  withRecipe: boolean,
): Promise<string[]> {
  const res = await postJson(`/api/docs/${docId}/save-to`, { recipe, ...opts, folder, name, with_recipe: withRecipe });
  if (res.status === 409 && res.headers.get('x-replaceable')) {
    let msg = 'already exists';
    try {
      msg = (await res.json()).error ?? msg;
    } catch {
      // keep the default
    }
    throw new ReplaceableError(409, msg);
  }
  return (await json<{ saved: string[] }>(res)).saved;
}

/**
 * The name to suggest (no extension): the next free -glitch name for this
 * photo in a folder, or with no folder, the name a download gets.
 */
export async function suggestName(docId: string, folder: string | null, format: ExportFormat, project = false): Promise<string> {
  const res = await postJson(`/api/docs/${docId}/suggest`, { ...(folder === null ? {} : { folder }), format, project });
  return (await json<{ name: string }>(res)).name;
}

export const getSettings = async () => json<FolderSettings>(await fetch('/api/settings'));

export async function putSettings(s: FolderSettings): Promise<FolderSettings> {
  const res = await fetch('/api/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ library: s.library, export: s.export }),
  });
  return json<FolderSettings>(res);
}

export const folders = async (path: string) =>
  json<FolderListing>(await fetch(`/api/folders?path=${encodeURIComponent(path)}`));

/** Make a folder inside `path` (relative to home); returns its listing. A taken name is a 409. */
export const newFolder = async (path: string, name: string) =>
  json<FolderListing>(await postJson('/api/folders', { path, name }));

/** Desktop app: what Save as wrote, or skipped (an existing recipe file). */
export interface SavedAs {
  saved: string[];
  skipped: string[];
}

/** Desktop app: the native Open dialog. `null` if the user cancelled. */
export async function desktopOpen(): Promise<DocInfo | null> {
  const res = await postJson('/api/desktop/open', {});
  if (res.status === 204) return null;
  return json<DocInfo>(res);
}

/** Desktop app: the file it was started with (a double-clicked project), once; else null. */
export async function desktopLaunch(): Promise<DocInfo | null> {
  const res = await postJson('/api/desktop/launch', {});
  if (res.status === 204) return null;
  return json<DocInfo>(res);
}

/** Desktop app: the native Save dialog, then render and write. */
export async function saveAs(
  docId: string,
  recipe: RecipeDto,
  opts: ExportOptions,
  withRecipe: boolean,
): Promise<SavedAs | null> {
  const res = await postJson(`/api/docs/${docId}/save-as`, { recipe, ...opts, with_recipe: withRecipe });
  if (res.status === 204) return null;
  return json<SavedAs>(res);
}

export async function recipeToToml(recipe: RecipeDto): Promise<string> {
  const res = await postJson('/api/recipe/to-toml', recipe);
  if (!res.ok) await fail(res);
  return res.text();
}

/** Recipes saved in the library and export folders, newest first. */
export async function listRecipes(): Promise<SavedRecipe[]> {
  return json<SavedRecipe[]>(await fetch('/api/recipes'));
}

/** Read one of them. */
export async function readRecipe(r: SavedRecipe): Promise<RecipeDto> {
  const q = new URLSearchParams({ place: r.place, path: r.path });
  return json<RecipeDto>(await fetch(`/api/recipe?${q}`));
}

export async function recipeFromToml(text: string): Promise<RecipeDto> {
  return json<RecipeDto>(await fetch('/api/recipe/from-toml', { method: 'POST', body: text }));
}

/** The file name from a Content-Disposition header (RFC 6266). */
function filenameOf(res: Response): string | null {
  const cd = res.headers.get('content-disposition') ?? '';
  const star = /filename\*=UTF-8''([^;]+)/i.exec(cd);
  if (star) return decodeURIComponent(star[1]);
  const plain = /filename="([^"]+)"/i.exec(cd);
  return plain ? plain[1] : null;
}

/**
 * Whether to hand files to the share sheet instead of downloading them:
 * touch devices that can share files (iOS, Android). On iOS this is the
 * native way to save (Save Image, Save to Files, AirDrop), and in an app
 * added to the home screen a download could open the file in place, with
 * no back button to return.
 */
export function shareFiles(file: File): boolean {
  return (
    matchMedia('(pointer: coarse)').matches &&
    typeof navigator.canShare === 'function' &&
    navigator.canShare({ files: [file] })
  );
}

/** Start downloading a link: a plain GET the browser saves under the server's name. */
export function follow(link: DownloadLink): void {
  const a = document.createElement('a');
  a.href = link.url;
  a.download = link.name;
  document.body.appendChild(a);
  a.click();
  a.remove();
}
