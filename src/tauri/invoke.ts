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

/**
 * A photo row as returned by catalog queries and `search_photos`.
 * Field names mirror the `photos` SQLite table.
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
  size_bytes: number | null;
  camera_make: string | null;
  camera_model: string | null;
  aperture: number | null;
  shutter: string | null;
  iso: number | null;
  focal_mm: number | null;
  aesthetic_score: number | null;
  paired_photo_id: number | null;
  raw_format: string | null;
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

/** Download AI models to the local models directory.
 *  Pass `names` to download a subset; omit for all known models.
 *  Progress is emitted as `DOWNLOAD_PROGRESS_EVENT` Tauri events.
 *  Returns the names of successfully installed models. */
export async function downloadModels(names?: string[]): Promise<string[]> {
  return tauriInvoke<string[]>('download_models', { names: names ?? null });
}

// ── Download progress event ─────────────────────────────────────────────────

export interface DownloadProgressEvent {
  model_name: string;
  downloaded_bytes: number;
  total_bytes: number;
  done: boolean;
  already_installed: boolean;
}

export const DOWNLOAD_PROGRESS_EVENT = 'chronimage://download-progress';

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

// ── Dedupe ─────────────────────────────────────────────────────────────────

export type DupeKind = 'Exact' | 'Near';

export interface DuplicateGroup {
  photo_ids: number[];
  max_similarity: number;
  kind: DupeKind;
}

export async function findDuplicates(minSimilarity?: number): Promise<DuplicateGroup[]> {
  return tauriInvoke<DuplicateGroup[]>('find_duplicates', {
    min_similarity: minSimilarity ?? null,
  });
}

// ── Natural-language search ─────────────────────────────────────────────────

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

// ── Source-side cleanup ────────────────────────────────────────────────────

export interface CleanupItem {
  copy_id: number;
  photo_id: number;
  source_id: number;
  source_kind: string;
  path: string | null;
  verified_sha256: string;
  size_bytes: number;
}

export interface SourceCleanupItem {
  source_id: number;
  source_name: string;
  source_kind: string;
  reclaimable_bytes: number;
  file_count: number;
  items: CleanupItem[];
}

export interface CleanupPlan {
  plan_id: string;
  confirm_token: string;
  total_reclaimable_bytes: number;
  total_file_count: number;
  sources: SourceCleanupItem[];
}

export async function cleanupDryRun(): Promise<CleanupPlan> {
  return tauriInvoke<CleanupPlan>('cleanup_dry_run');
}

export interface CleanupExecuteResult {
  deleted_count: number;
  freed_bytes: number;
  errors: string[];
}

export async function cleanupExecute(planId: string, confirmToken: string): Promise<CleanupExecuteResult> {
  return tauriInvoke<CleanupExecuteResult>('cleanup_execute', {
    planId,
    confirmToken,
  });
}

// ── Face clusters ──────────────────────────────────────────────────────────

export interface ClusterRow {
  id: number;
  name: string | null;
  isNamed: boolean;
  faceCount: number;
  coverPhotoId: number | null;
}

export async function faceClustersList(limit = 60): Promise<ClusterRow[]> {
  return tauriInvoke<ClusterRow[]>('face_clusters_list', { limit });
}

export async function faceClusterName(clusterId: number, name: string): Promise<void> {
  return tauriInvoke<void>('face_cluster_name', { clusterId, name });
}

export async function faceClusterMerge(a: number, b: number): Promise<number> {
  return tauriInvoke<number>('face_cluster_merge', { a, b });
}

// ── AI Models status ────────────────────────────────────────────────────────

export interface ModelStatus {
  name: string;
  kind: string;
  filename: string;
  installed: boolean;
  sizeBytes: number;
}

export async function aiModelsStatus(): Promise<ModelStatus[]> {
  return tauriInvoke<ModelStatus[]>('ai_models_status');
}

// ── Lift & Shift ────────────────────────────────────────────────────────────

export interface LiftItem {
  copy_id: number;
  photo_id: number;
  source_id: number;
  src_path: string;
  dest_rel_path: string;
  sha256: string;
  size_bytes: number;
}

export interface LiftPlan {
  plan_id: string;
  confirm_token: string;
  target_root: string;
  total_bytes: number;
  total_file_count: number;
  items: LiftItem[];
  free_space_ok: boolean;
}

export interface LiftReceipt {
  copied_count: number;
  bytes_copied: number;
  manifest_path: string;
  errors: string[];
}

export async function liftShiftDryRun(targetRoot: string): Promise<LiftPlan> {
  return tauriInvoke<LiftPlan>('lift_shift_dry_run', { targetRoot });
}

export async function liftShiftExecute(planId: string, confirmToken: string): Promise<LiftReceipt> {
  return tauriInvoke<LiftReceipt>('lift_shift_execute', { planId, confirmToken });
}

// ── View tracking ──────────────────────────────────────────────────────────

/**
 * Records that a photo was viewed (opens the Detail overlay).
 * Backend upserts `photo_views` and a trigger propagates `last_viewed_at`
 * onto `photos` for the LastViewed rule engine predicate.
 */
export async function recordPhotoView(photoId: number): Promise<void> {
  return tauriInvoke<void>('record_photo_view', { photoId });
}
