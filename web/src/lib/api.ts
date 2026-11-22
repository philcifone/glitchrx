// Thin fetch wrappers around the glitchrx serve API. Same origin only, so the
// access-token cookie (if any) rides along automatically.

import type {
  DocInfo,
  EffectInfo,
  ExportFormat,
  Listing,
  RecipeDto,
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
}

/**
 * Render a preview covering a `maxW` x `maxH` box (device pixels). With
 * `exact`, the server bends the full-resolution photo and then shrinks it,
 * which is exactly the export but slower. Resolves to null when the server
 * dropped it for a newer request; throws TooHeavyError over the budget.
 */
export async function preview(
  docId: string,
  recipe: RecipeDto | null,
  maxW: number,
  maxH: number,
  exact = false,
): Promise<PreviewResult | null> {
  const body = recipe
    ? { recipe, max_w: maxW, max_h: maxH, exact }
    : { original: true, max_w: maxW, max_h: maxH };
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

/** Full-resolution export; returns the file and its suggested name. */
export async function exportFile(
  docId: string,
  recipe: RecipeDto,
  format: ExportFormat,
): Promise<{ blob: Blob; name: string }> {
  const res = await postJson(`/api/docs/${docId}/export`, { recipe, format });
  if (!res.ok) await fail(res);
  return { blob: await res.blob(), name: filenameOf(res) ?? 'glitch' };
}

/** Save next to the original on the server; returns the paths written. */
export async function saveOnServer(
  docId: string,
  recipe: RecipeDto,
  format: ExportFormat,
  withRecipe: boolean,
): Promise<string[]> {
  const res = await postJson(`/api/docs/${docId}/save`, {
    recipe,
    format,
    with_recipe: withRecipe,
  });
  return (await json<{ saved: string[] }>(res)).saved;
}

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

/** Desktop app: the native Save dialog, then render and write. */
export async function saveAs(
  docId: string,
  recipe: RecipeDto,
  format: ExportFormat,
  withRecipe: boolean,
): Promise<SavedAs | null> {
  const res = await postJson(`/api/docs/${docId}/save-as`, { recipe, format, with_recipe: withRecipe });
  if (res.status === 204) return null;
  return json<SavedAs>(res);
}

export async function recipeToToml(recipe: RecipeDto): Promise<string> {
  const res = await postJson('/api/recipe/to-toml', recipe);
  if (!res.ok) await fail(res);
  return res.text();
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

/** Hand a blob to the browser as a download. */
export function saveBlob(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  // Give the browser time to start the download before revoking.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
