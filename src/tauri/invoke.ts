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
  albumId?: number | null;
}

export async function listPhotos(params?: ListPhotosParams): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('list_photos', {
    limit: params?.limit ?? null,
    offset: params?.offset ?? null,
    albumId: params?.albumId ?? null,
  });
}

export async function refreshSmartAlbums(): Promise<void> {
  return tauriInvoke<void>('refresh_smart_albums');
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

export async function deleteSource(sourceId: number): Promise<void> {
  return tauriInvoke<void>('delete_source', { sourceId });
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

// ── Source-side cleanup ─────────────────────────────────────────────────────

export interface SourceCleanupItem {
  source_copy_id: number;
  photo_id: number;
  source_id: number;
  path: string;
  size_bytes: number;
  sha256: string;
}

export interface CleanupPlan {
  source_id: number;
  source_name: string;
  reclaimable_bytes: number;
  item_count: number;
  items: SourceCleanupItem[];
}

export async function cleanupDryRun(sourceId?: number): Promise<CleanupPlan[]> {
  return tauriInvoke<CleanupPlan[]>('cleanup_dry_run', { sourceId: sourceId ?? null });
}

// ── Rediscovery commands ────────────────────────────────────────────────────

export async function onThisDay(limit?: number): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('on_this_day', { limit: limit ?? null });
}

export async function unseenPhotos(limit?: number, minScore?: number): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('unseen_photos', {
    limit: limit ?? null,
    minScore: minScore ?? null,
  });
}

// ── Source connectors ───────────────────────────────────────────────────────

/** Run the full import pipeline over a Google Photos Takeout export, then enrich with sidecar metadata. */
export async function importGoogleTakeout(sourceId: number, root: string): Promise<StartImportResponse> {
  return tauriInvoke<StartImportResponse>('import_google_takeout', { sourceId, root });
}

/** Detect the iCloud-for-Windows Photos folder path, or null if not installed. */
export async function detectIcloudPath(): Promise<string | null> {
  return tauriInvoke<string | null>('detect_icloud_path');
}

export interface UsbDevice {
  device_id: string;
  friendly_name: string;
  manufacturer: string;
  description: string;
}

/** List Apple USB devices connected via WPD/MTP. Returns [] when none. */
export async function listIphoneDevices(): Promise<UsbDevice[]> {
  return tauriInvoke<UsbDevice[]>('list_iphone_devices');
}

// ── AI commands ────────────────────────────────────────────────────────────

export type HardwareTier = 'CpuOnly' | 'GpuLow' | 'GpuHigh';

export interface HardwareInfo {
  tier: HardwareTier;
  vram_mb: number;
  adapter_name: string;
}

/** Detect GPU tier + VRAM (Windows DXGI; stub on other platforms). */
export async function detectHardware(): Promise<HardwareInfo> {
  return tauriInvoke<HardwareInfo>('detect_hardware');
}

/** Embed an image via SigLIP-B/16. Returns 768-dim f32 array. Errors if model not downloaded. */
export async function embedImage(path: string): Promise<number[]> {
  return tauriInvoke<number[]>('embed_image', { path });
}

/** Score a photo 0–10 for aesthetic quality via NIMA. Errors if model not downloaded. */
export async function scoreAesthetic(path: string): Promise<number> {
  return tauriInvoke<number>('score_aesthetic', { path });
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
