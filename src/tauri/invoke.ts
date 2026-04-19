/**
 * Typed wrappers over Tauri's `invoke`. Every Rust `#[tauri::command]`
 * surfaces as an async function here so TS call-sites stay type-safe.
 *
 * In tests, mock this module (or mock `@tauri-apps/api/core`) rather than
 * stubbing individual call sites.
 */

import { invoke as tauriInvoke } from '@tauri-apps/api/core';

export type ReleaseChannel = 'dev' | 'stable' | 'beta' | 'nightly' | 'insider';

export async function ping(): Promise<string> {
  return tauriInvoke<string>('ping');
}

export async function appVersion(): Promise<string> {
  return tauriInvoke<string>('app_version');
}

export async function currentChannel(): Promise<{ channel: ReleaseChannel }> {
  return tauriInvoke<{ channel: ReleaseChannel }>('current_channel');
}

// ── Phase 1 ────────────────────────────────────────────────────────────────

export interface ExtCount {
  ext: string;
  count: number;
}

export interface ScanReport {
  root: string;
  total_files: number;
  raw_jpg_pairs: number;
  unpaired: number;
  by_extension: ExtCount[];
}

/**
 * Preview a scan of `root` without writing to the catalog. Reports file
 * counts by extension and RAW+JPG pair count.
 */
export async function importDryRun(root: string): Promise<ScanReport> {
  return tauriInvoke<ScanReport>('import_dry_run', { root });
}

// ── Search ──────────────────────────────────────────────────────────────────

/**
 * A photo row as returned by `search_photos` and (eventually) the catalog
 * grid query. Field names mirror the `photos` SQLite table.
 */
export interface PhotoRow {
  id: number;
  sha256: string;
  filename: string;
  width: number;
  height: number;
  captured_at: string | null;
  imported_at: string;
  is_raw: boolean;
  paired_photo_id: number | null;
  camera_make: string | null;
  camera_model: string | null;
  aesthetic_score: number | null;
  size_bytes: number | null;
  raw_format: string | null;
}

/**
 * Encode `query` with the on-device SigLIP text encoder and return up to
 * `limit` (default 50) photos ordered by cosine similarity desc.
 *
 * Returns an empty array — not an error — when the model is absent or no
 * embeddings have been computed yet.
 */
export async function searchPhotos(query: string, limit?: number): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('search_photos', { query, limit: limit ?? null });
}
