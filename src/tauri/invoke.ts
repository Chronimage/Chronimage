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

// ── Catalog read ────────────────────────────────────────────────────────────

export interface AlbumRow {
  id: number;
  name: string;
  description: string | null;
  tag: string | null;
  photo_count: number;
  /** JSON-encoded array of photo ids, e.g. "[1,2,3]" */
  cover_photo_ids: string;
  is_system: boolean;
}

export interface PhotoRow {
  id: number;
  sha256: string;
  filename: string;
  width: number;
  height: number;
  captured_at: string | null;
  is_raw: boolean;
  size_bytes: number | null;
  camera_make: string | null;
  camera_model: string | null;
  aperture: number | null;
  shutter: string | null;
  iso: number | null;
  focal_mm: number | null;
  aesthetic_score: number | null;
  paired_photo_id: number | null;
}

export interface SourceRow {
  id: number;
  name: string;
  kind: string;
  status: string;
  last_scan_at: string | null;
  photo_count: number;
}

export async function listAlbums(): Promise<AlbumRow[]> {
  return tauriInvoke<AlbumRow[]>('list_albums');
}

export interface ListPhotosParams {
  limit?: number;
  offset?: number;
}

export async function listPhotos(params?: ListPhotosParams): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('list_photos', {
    limit: params?.limit ?? null,
    offset: params?.offset ?? null,
  });
}

export async function listSources(): Promise<SourceRow[]> {
  return tauriInvoke<SourceRow[]>('list_sources');
}

export async function createSource(name: string, kind: string, rootPath?: string): Promise<SourceRow> {
  return tauriInvoke<SourceRow>('create_source', {
    name,
    kind,
    rootPath: rootPath ?? null,
  });
}

// ── Import commands ─────────────────────────────────────────────────────────

export interface StartImportResponse {
  import_id: number;
}

export async function startImport(sourceId: number, root: string): Promise<StartImportResponse> {
  return tauriInvoke<StartImportResponse>('start_import', { sourceId, root });
}

export interface ImportSummary {
  id: number;
  source_id: number;
  started_at: string;
  finished_at: string | null;
  total_files: number;
  imported_count: number;
  skipped_count: number;
  error_count: number;
  last_seen_path: string | null;
}

export async function listImports(sourceId?: number): Promise<ImportSummary[]> {
  return tauriInvoke<ImportSummary[]>('list_imports', { sourceId: sourceId ?? null });
}

// ── Import progress event ───────────────────────────────────────────────────

export interface ImportProgressEvent {
  source_id: number;
  import_id: number;
  total: number;
  done: number;
  current_file: string;
  eta_seconds: number | null;
}

export const IMPORT_PROGRESS_EVENT = 'chronimage://import-progress';
