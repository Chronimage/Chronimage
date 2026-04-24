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
  /** EXIF Orientation (1–8). 1 = no rotation. Values 5–8 imply a 90°
   * transform so display width/height are swapped vs. stored dimensions. */
  orientation: number;
  /** Laplacian-variance sharpness score from Stage 2.6. */
  sharpness_score: number | null;
  /** Phase 2 §1 — 0..=5 star rating set from the detail-view toolbar. */
  star_rating: number;
  /** Phase 2 §2 — soft flag toggled via the detail-view `X` key. */
  is_flagged: boolean;
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

export type PhotoSortBy =
  | 'captured_desc'
  | 'captured_asc'
  | 'imported_desc'
  | 'filename_asc'
  | 'aesthetic_desc'
  | 'random';

export interface ListPhotosParams {
  limit?: number;
  offset?: number;
  albumId?: number | null;
  sortBy?: PhotoSortBy | null;
}

export async function listPhotos(params?: ListPhotosParams): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('list_photos', {
    limit: params?.limit ?? null,
    offset: params?.offset ?? null,
    albumId: params?.albumId ?? null,
    sortBy: params?.sortBy ?? null,
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

export interface RemoveReceipt {
  removed_photos: number;
  removed_thumbnails: number;
  errors: string[];
}

export interface RecycleReceipt {
  recycled_count: number;
  skipped_count: number;
  errors: string[];
}

export interface SourceDeletionPlan {
  photos_total: number;
  orphan_photos: number;
  local_files: number;
  total_bytes: number;
  cloud_only: number;
}

export interface RemovePreview {
  photo_count: number;
  local_files: number;
  cloud_only_photos: number;
  total_bytes: number;
}

export async function deleteSource(
  sourceId: number,
  opts?: { recycleFiles?: boolean; removeOrphanPhotos?: boolean },
): Promise<RemoveReceipt> {
  return tauriInvoke<RemoveReceipt>('delete_source', {
    sourceId,
    recycleFiles: opts?.recycleFiles ?? false,
    removeOrphanPhotos: opts?.removeOrphanPhotos ?? true,
  });
}

export async function sourceDeletionPreview(sourceId: number): Promise<SourceDeletionPlan> {
  return tauriInvoke<SourceDeletionPlan>('source_deletion_preview', { sourceId });
}

export async function removePhotosPreview(photoIds: number[]): Promise<RemovePreview> {
  return tauriInvoke<RemovePreview>('remove_photos_preview', { photoIds });
}

export async function removePhotosFromCatalog(photoIds: number[]): Promise<RemoveReceipt> {
  return tauriInvoke<RemoveReceipt>('remove_photos_from_catalog', { photoIds });
}

export async function recycleSourceCopies(photoIds: number[]): Promise<RecycleReceipt> {
  return tauriInvoke<RecycleReceipt>('recycle_source_copies', { photoIds });
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

/** Photos captured within 30 days of the first photo from a previously-unseen camera. */
export async function firstTimeOnNewCamera(limit?: number): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('first_time_on_new_camera', { limit: limit ?? null });
}

/** NIMA-high photos never viewed by the user yet. */
export async function unflaggedFavorites(limit?: number, minScore?: number): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('unflagged_favorites', {
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

/**
 * Natural-language search suggestion chips for the catalog search bar.
 *
 * Blends 6–8 curated seeds with dynamic hints derived from the live catalog
 * (top named face clusters, top camera make/model). Always ≤ 8 items.
 */
export async function searchSuggestions(): Promise<string[]> {
  return tauriInvoke<string[]>('search_suggestions');
}

export interface TagRow {
  id: number;
  label: string;
  kind: string;
  confidence: number;
}

/**
 * All tags attached to a photo, ordered by confidence (most-confident first).
 * Covers AI-assigned (people/place/object/event/color/camera/auto_scene) and
 * user-assigned (`kind === 'user'`) tags.
 */
export async function listTags(photoId: number): Promise<TagRow[]> {
  return tauriInvoke<TagRow[]>('list_tags', { photoId });
}

export interface PhotoQuality {
  aesthetic: number | null;
  sharpness: number | null;
  face_count: number;
  best_face_quality: number | null;
  min_eyes_open: number | null;
}

/** Aggregate quality metrics for the detail inspector Quality section. */
export async function photoQuality(photoId: number): Promise<PhotoQuality> {
  return tauriInvoke<PhotoQuality>('photo_quality', { photoId });
}

export interface PhotoLocation {
  lat: number | null;
  lng: number | null;
}

/** GPS coordinates for the detail inspector Location section. */
export async function photoLocation(photoId: number): Promise<PhotoLocation> {
  return tauriInvoke<PhotoLocation>('photo_location', { photoId });
}

/** Photos in which at least one face belongs to the given cluster — ordered by face quality. */
export async function listPhotosForCluster(clusterId: number, limit?: number): Promise<PhotoRow[]> {
  return tauriInvoke<PhotoRow[]>('list_photos_for_cluster', {
    clusterId,
    limit: limit ?? null,
  });
}

/**
 * JPEG-encoded thumbnail bytes for a photo, resized to `sizePx` longest edge.
 *
 * Backend caches the output at `{app_data}/cache/thumbnails/{sha256}_{size}.jpg`
 * so subsequent calls are fast. Returns a Uint8Array suitable for wrapping in
 * a Blob URL. Throws when the photo has no local copy or the source file is
 * missing — the caller should fall back to a placeholder in that case.
 */
export async function getThumbnail(photoId: number, sizePx?: number): Promise<Uint8Array> {
  const bytes = await tauriInvoke<number[]>('get_thumbnail', {
    photoId,
    sizePx: sizePx ?? null,
  });
  return new Uint8Array(bytes);
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

export type ModelSource = 'bundled' | 'downloaded' | 'missing';

export interface ModelStatus {
  name: string;
  kind: string;
  filename: string;
  installed: boolean;
  sizeBytes: number;
  source: ModelSource;
}

export async function aiModelsStatus(): Promise<ModelStatus[]> {
  return tauriInvoke<ModelStatus[]>('ai_models_status');
}

/** Trigger a re-index for a specific model kind after swapping the active model.
 *  Returns the number of rows affected. */
export async function aiReindex(kind: string): Promise<number> {
  return tauriInvoke<number>('ai_reindex', { kind });
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

// ── Google Photos OAuth2 + Picker ──────────────────────────────────────────

export interface GphotosBeginOauthResponse {
  auth_url: string;
  flow_id: string;
}

export type GphotosFlowStatus =
  | { state: 'pending' }
  | { state: 'completed'; email: string | null; scope: string }
  | { state: 'failed'; message: string }
  | { state: 'timed_out' };

export interface GphotosUserInfo {
  sub: string;
  email?: string | null;
  name?: string | null;
  picture?: string | null;
}

export interface GphotosPollingConfig {
  pollInterval?: string | null;
  timeoutIn?: string | null;
}

export interface GphotosPickerSession {
  id: string;
  /** Present on session *create*; Google omits it from poll responses
   *  once `mediaItemsSet` flips to true. Treat as optional everywhere. */
  pickerUri?: string | null;
  mediaItemsSet: boolean;
  pollingConfig?: GphotosPollingConfig | null;
  expireTime?: string | null;
}

export interface GphotosManualCleanupInstructions {
  headline: string;
  body: string;
  google_photos_url: string;
  takeout_url: string;
}

export async function gphotosBeginOauthFlow(clientId?: string): Promise<GphotosBeginOauthResponse> {
  return tauriInvoke<GphotosBeginOauthResponse>('gphotos_begin_oauth_flow', {
    clientId: clientId ?? null,
  });
}

export async function gphotosPollOauthFlow(flowId: string): Promise<GphotosFlowStatus> {
  return tauriInvoke<GphotosFlowStatus>('gphotos_poll_oauth_flow', { flowId });
}

export async function gphotosCancelOauthFlow(flowId: string): Promise<void> {
  return tauriInvoke<void>('gphotos_cancel_oauth_flow', { flowId });
}

export async function gphotosAuthStatus(): Promise<boolean> {
  return tauriInvoke<boolean>('gphotos_auth_status');
}

export async function gphotosSignOut(): Promise<void> {
  return tauriInvoke<void>('gphotos_sign_out');
}

export async function gphotosAccountInfo(clientId?: string): Promise<GphotosUserInfo> {
  return tauriInvoke<GphotosUserInfo>('gphotos_account_info', {
    clientId: clientId ?? null,
  });
}

/**
 * Synchronously ensure a `sources` row exists for the currently-connected
 * Google account. Returns the row (existing or freshly inserted). Call
 * right after `gphotosPollOauthFlow` reports `completed` — the backend
 * no longer spawns a background task to do this.
 */
export async function gphotosEnsureSourceRow(clientId?: string): Promise<SourceRow> {
  return tauriInvoke<SourceRow>('gphotos_ensure_source_row', {
    clientId: clientId ?? null,
  });
}

export async function gphotosCreatePickerSession(clientId?: string): Promise<GphotosPickerSession> {
  return tauriInvoke<GphotosPickerSession>('gphotos_create_picker_session', {
    clientId: clientId ?? null,
  });
}

export async function gphotosPollPickerSession(
  sessionId: string,
  clientId?: string,
): Promise<GphotosPickerSession> {
  return tauriInvoke<GphotosPickerSession>('gphotos_poll_picker_session', {
    sessionId,
    clientId: clientId ?? null,
  });
}

export async function gphotosDeletePickerSession(sessionId: string, clientId?: string): Promise<void> {
  return tauriInvoke<void>('gphotos_delete_picker_session', {
    sessionId,
    clientId: clientId ?? null,
  });
}

export async function gphotosManualCleanupInstructions(): Promise<GphotosManualCleanupInstructions> {
  return tauriInvoke<GphotosManualCleanupInstructions>('gphotos_manual_cleanup_instructions');
}

export async function importGooglePhotos(
  sourceId: number,
  sessionId: string,
  clientId?: string,
): Promise<StartImportResponse> {
  return tauriInvoke<StartImportResponse>('import_google_photos', {
    sourceId,
    sessionId,
    clientId: clientId ?? null,
  });
}

// ── Disk / catalog-home helpers ─────────────────────────────────────────────

export interface DiskInfo {
  free_bytes: number;
  total_bytes: number;
}

export async function getDefaultCatalogPath(): Promise<string> {
  return tauriInvoke<string>('get_default_catalog_path');
}

export async function getDiskInfo(path: string): Promise<DiskInfo> {
  return tauriInvoke<DiskInfo>('get_disk_info', { path });
}

// ── Face-cluster rebuild ────────────────────────────────────────────────────

export const RECLUSTER_PROGRESS_EVENT = 'chronimage://recluster-progress';

export interface ReclusterProgress {
  phase: 'start' | 'done';
  total_faces: number;
  clustered_faces: number;
  cluster_count: number;
}

export interface ReclusterReceipt {
  total_faces: number;
  clustered_faces: number;
  cluster_count: number;
  named_preserved: number;
  new_clusters: number;
  pruned_empty: number;
  elapsed_ms: number;
}

export async function reclusterFaces(): Promise<ReclusterReceipt> {
  return tauriInvoke<ReclusterReceipt>('recluster_faces');
}

// ── Thumbnail cache rebuild ─────────────────────────────────────────────────

export const REBUILD_PROGRESS_EVENT = 'chronimage://rebuild-progress';

export interface RebuildProgress {
  total: number;
  done: number;
  failed: number;
  current_photo_id: number;
  phase: 'start' | 'tick' | 'done';
}

export interface RebuildReceipt {
  total: number;
  regenerated: number;
  failed: number;
  elapsed_ms: number;
}

export async function rebuildThumbnails(): Promise<RebuildReceipt> {
  return tauriInvoke<RebuildReceipt>('rebuild_thumbnails');
}

// ── Phase 2: Cull verdict + rating + flag ─────────────────────────────────────

export type CullVerdict = 'keep' | 'reject_a' | 'reject_b' | 'reject_both' | 'skip';
export type CullReason =
  | 'near_dup'
  | 'blur'
  | 'eyes_closed'
  | 'exposure'
  | 'user'
  | 'flag'
  | 'duplicate'
  | 'other';

export interface VerdictReceipt {
  photo_id: number;
  verdict: CullVerdict;
  rejected_ids: number[];
  retention_days: number;
}

export const CULL_PROGRESS_EVENT = 'chronimage://cull-progress';

export async function cullApplyVerdict(
  photoId: number,
  verdict: CullVerdict,
  reason: CullReason,
  retentionDays?: number,
): Promise<VerdictReceipt> {
  return tauriInvoke<VerdictReceipt>('cull_apply_verdict', {
    photoId,
    verdict,
    reason,
    retentionDays: retentionDays ?? null,
  });
}

export async function ratePhoto(photoId: number, rating: number): Promise<void> {
  return tauriInvoke('rate_photo', { photoId, rating });
}

export async function flagPhoto(photoId: number): Promise<boolean> {
  return tauriInvoke<boolean>('flag_photo', { photoId });
}

// ── Phase 2: Cull Bin ─────────────────────────────────────────────────────────

export type CullBinFilter = CullReason | 'all';

export interface CullBinRow {
  photo_id: number;
  filename: string;
  rejected_at: string;
  reason: CullReason;
  retention_days: number;
  permanent_delete_after: string;
  size_bytes: number | null;
  sha256: string;
}

export interface CullBinSummary {
  total_count: number;
  total_bytes: number;
  by_reason: [string, number][];
}

export interface RestoreReceipt {
  restored_count: number;
  skipped: number[];
}

export interface EmptyReceipt {
  deleted_photo_count: number;
  freed_bytes: number;
  errors: string[];
}

export async function cullBinList(filter?: CullBinFilter): Promise<CullBinRow[]> {
  return tauriInvoke<CullBinRow[]>('cull_bin_list', {
    filter: filter ?? null,
  });
}

export async function cullBinSummary(): Promise<CullBinSummary> {
  return tauriInvoke<CullBinSummary>('cull_bin_summary');
}

export async function cullBinRestore(photoIds: number[]): Promise<RestoreReceipt> {
  return tauriInvoke<RestoreReceipt>('cull_bin_restore', { photoIds });
}

export async function cullBinDeleteForever(photoIds: number[]): Promise<EmptyReceipt> {
  return tauriInvoke<EmptyReceipt>('cull_bin_delete_forever', { photoIds });
}

export async function cullBinSweep(): Promise<EmptyReceipt> {
  return tauriInvoke<EmptyReceipt>('cull_bin_sweep');
}

// ── Phase 2: Export ───────────────────────────────────────────────────────────

export type ExportFormat = 'jpeg' | 'tiff' | 'heic';
export type ColorProfile = 'srgb' | 'displayp3' | 'adobergb';

export interface StripMeta {
  gps: boolean;
  all_exif: boolean;
  camera_serial: boolean;
}

export interface ExportPreset {
  format: ExportFormat;
  color: ColorProfile;
  quality: number;
  long_edge_px: number;
  strip_meta: StripMeta;
  watermark_text: string | null;
  archive_originals: boolean;
}

export function defaultExportPreset(): ExportPreset {
  return {
    format: 'jpeg',
    color: 'srgb',
    quality: 88,
    long_edge_px: 2000,
    strip_meta: { gps: true, all_exif: false, camera_serial: false },
    watermark_text: null,
    archive_originals: false,
  };
}

export interface ExportJob {
  id: number;
  created_at: string;
  preset_json: string;
  total_photos: number;
  done_count: number;
  error_count: number;
  status: 'queued' | 'running' | 'paused' | 'done' | 'cancelled' | 'error';
  output_dir: string;
}

export interface ExportProgress {
  job_id: number;
  photo_id: number;
  done_count: number;
  error_count: number;
  total: number;
  status: 'queued' | 'running' | 'paused' | 'done' | 'cancelled' | 'error';
  item_status: 'queued' | 'running' | 'done' | 'error';
  op: string;
  output_path: string | null;
  error_msg: string | null;
}

export const EXPORT_PROGRESS_EVENT = 'chronimage://export-progress';

export async function exportEnqueue(
  photoIds: number[],
  preset: ExportPreset,
  outputDir: string,
): Promise<number> {
  return tauriInvoke<number>('export_enqueue', {
    photoIds,
    preset,
    outputDir,
  });
}

export async function exportRunNext(jobId: number): Promise<ExportProgress | null> {
  return tauriInvoke<ExportProgress | null>('export_run_next', { jobId });
}

export async function exportListJobs(): Promise<ExportJob[]> {
  return tauriInvoke<ExportJob[]>('export_list_jobs');
}

// ── Phase 2 §10: Manual tagging ───────────────────────────────────────────────

export interface UserTagSummary {
  label: string;
  photo_count: number;
}

export async function listUserTags(): Promise<UserTagSummary[]> {
  return tauriInvoke<UserTagSummary[]>('list_user_tags');
}

export async function addUserTag(photoIds: number[], label: string): Promise<number> {
  return tauriInvoke<number>('add_user_tag', { photoIds, label });
}

export async function removeUserTag(photoIds: number[], label: string): Promise<number> {
  return tauriInvoke<number>('remove_user_tag', { photoIds, label });
}

export async function renameUserTag(oldLabel: string, newLabel: string): Promise<number> {
  return tauriInvoke<number>('rename_user_tag', { oldLabel, newLabel });
}

// ── Phase 2 §6: Cloud upload adapters ─────────────────────────────────────────

export interface UploadReceipt {
  uploaded_count: number;
  skipped_count: number;
  errors: string[];
}

/** True iff the stored Google Photos token already carries the
 * `photoslibrary.appendonly` scope. When false, the frontend should
 * trigger a fresh OAuth flow before calling `gphotosUpload`. */
export async function gphotosUploadScopeOk(): Promise<boolean> {
  return tauriInvoke<boolean>('gphotos_upload_scope_ok');
}

export async function gphotosUpload(photoIds: number[]): Promise<UploadReceipt> {
  return tauriInvoke<UploadReceipt>('gphotos_upload', { photoIds });
}

/** True iff OneDrive OAuth tokens are in keyring. */
export async function onedriveAuthStatus(): Promise<boolean> {
  return tauriInvoke<boolean>('onedrive_auth_status');
}

// ── Phase 3: Develop (non-destructive edits) ──────────────────────────────────

export interface DevelopOperations {
  exposure: number;
  contrast: number;
  highlights: number;
  shadows: number;
  whites: number;
  blacks: number;
  temp: number;
  tint: number;
  vibrance: number;
  saturation: number;
  clarity: number;
  dehaze: number;
}

export function identityOperations(): DevelopOperations {
  return {
    exposure: 0,
    contrast: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    temp: 0,
    tint: 0,
    vibrance: 0,
    saturation: 0,
    clarity: 0,
    dehaze: 0,
  };
}

export interface RenderReceipt {
  photo_id: number;
  preview_data_url: string;
  elapsed_ms: number;
}

export interface DevelopOpenResponse {
  photo_id: number;
  operations: DevelopOperations;
  preview_data_url: string;
}

export interface PastedReceipt {
  pasted_photo_count: number;
  skipped: number[];
}

export interface DevelopPreset {
  id: number;
  name: string;
  group_name: string;
  description: string | null;
  operations_json: string;
  is_system: boolean;
  created_at: string;
  updated_at: string;
}

export async function developOpen(photoId: number): Promise<DevelopOpenResponse> {
  return tauriInvoke<DevelopOpenResponse>('develop_open', { photoId });
}

export async function developApply(photoId: number, operations: DevelopOperations): Promise<RenderReceipt> {
  return tauriInvoke<RenderReceipt>('develop_apply', { photoId, operations });
}

export async function developSave(
  photoId: number,
  operations: DevelopOperations,
  label?: string,
): Promise<number> {
  return tauriInvoke<number>('develop_save', {
    photoId,
    operations,
    label: label ?? null,
  });
}

export async function developReset(photoId: number): Promise<number> {
  return tauriInvoke<number>('develop_reset', { photoId });
}

export async function developCopyEdits(photoId: number): Promise<DevelopOperations> {
  return tauriInvoke<DevelopOperations>('develop_copy_edits', { photoId });
}

export async function developPasteEdits(
  photoIds: number[],
  operations: DevelopOperations,
): Promise<PastedReceipt> {
  return tauriInvoke<PastedReceipt>('develop_paste_edits', { photoIds, operations });
}

export async function developPresetApply(
  photoId: number,
  presetId: number,
  strength: number,
): Promise<RenderReceipt> {
  return tauriInvoke<RenderReceipt>('develop_preset_apply', { photoId, presetId, strength });
}

export async function presetsList(group?: string): Promise<DevelopPreset[]> {
  return tauriInvoke<DevelopPreset[]>('presets_list', { group: group ?? null });
}

export async function presetSave(
  name: string,
  group: string,
  operations: DevelopOperations,
): Promise<number> {
  return tauriInvoke<number>('preset_save', { name, group, operations });
}

export async function onedriveUpload(photoIds: number[], remoteFolder: string): Promise<UploadReceipt> {
  return tauriInvoke<UploadReceipt>('onedrive_upload', { photoIds, remoteFolder });
}

// ── Phase 4: Map · Shortcuts · XMP rescan ─────────────────────────────────────

export interface TripRow {
  id: number;
  name: string | null;
  start_at: string;
  end_at: string;
  center_lat: number;
  center_lng: number;
  radius_km: number;
  photo_count: number;
  auto_generated: boolean;
  updated_at: string;
}

export interface TripRecomputeReceipt {
  trip_count: number;
  photo_count: number;
  elapsed_ms: number;
}

export async function mapRecomputeTrips(): Promise<TripRecomputeReceipt> {
  return tauriInvoke<TripRecomputeReceipt>('map_recompute_trips');
}

export async function mapListTrips(): Promise<TripRow[]> {
  return tauriInvoke<TripRow[]>('map_list_trips');
}

export async function mapPhotosInTrip(tripId: number): Promise<number[]> {
  return tauriInvoke<number[]>('map_photos_in_trip', { tripId });
}

export interface XmpRescanReceipt {
  scanned: number;
  applied: number;
  error_count: number;
  errors: string[];
}

export async function xmpRescan(): Promise<XmpRescanReceipt> {
  return tauriInvoke<XmpRescanReceipt>('xmp_rescan');
}

export interface XmpExportReceipt {
  written: number;
  skipped: number;
  error_count: number;
  errors: string[];
}

export async function xmpWriteOnChangeGet(): Promise<boolean> {
  return tauriInvoke<boolean>('xmp_write_on_change_get');
}

export async function xmpWriteOnChangeSet(enabled: boolean): Promise<void> {
  return tauriInvoke('xmp_write_on_change_set', { enabled });
}

export async function xmpExportAll(): Promise<XmpExportReceipt> {
  return tauriInvoke<XmpExportReceipt>('xmp_export_all');
}

// ── Prompt sidecar (Phase 4 §1/§2) ────────────────────────────────────────

export interface SidecarStatus {
  configured: boolean;
  url: string | null;
  reachable: boolean;
  model: string | null;
  error: string | null;
}

export interface PromptEditRequest {
  photo_id: number;
  prompt: string;
  strength: number;
  constraints: string[];
  mask_b64?: string | null;
}

export interface PromptEditResult {
  image_b64: string;
  latency_ms: number;
  model_id: string;
  seed: number;
}

export async function promptSidecarGet(): Promise<string | null> {
  return tauriInvoke<string | null>('prompt_sidecar_get');
}

export async function promptSidecarSet(url: string | null): Promise<void> {
  return tauriInvoke('prompt_sidecar_set', { url });
}

export async function promptSidecarModelGet(): Promise<string | null> {
  return tauriInvoke<string | null>('prompt_sidecar_model_get');
}

export async function promptSidecarModelSet(model: string | null): Promise<void> {
  return tauriInvoke('prompt_sidecar_model_set', { model });
}

export async function promptSidecarPing(): Promise<SidecarStatus> {
  return tauriInvoke<SidecarStatus>('prompt_sidecar_ping');
}

export async function promptEdit(req: PromptEditRequest): Promise<PromptEditResult> {
  return tauriInvoke<PromptEditResult>('prompt_edit', { req });
}

export interface MaskFromPromptRequest {
  photo_id: number;
  prompt: string;
}

export interface MaskFromPromptResult {
  mask_b64: string;
  confidence: number;
  latency_ms: number;
}

export async function maskFromPrompt(req: MaskFromPromptRequest): Promise<MaskFromPromptResult> {
  return tauriInvoke<MaskFromPromptResult>('mask_from_prompt', { req });
}

export interface PromptEditRow {
  id: number;
  photo_id: number;
  prompt: string;
  strength: number;
  constraints_json: string;
  mask_b64: string | null;
  rendered_b64: string;
  model_id: string;
  seed: number;
  latency_ms: number;
  state: 'pending' | 'accepted' | 'rejected';
  created_at: string;
}

export async function promptEditList(photoId: number): Promise<PromptEditRow[]> {
  return tauriInvoke<PromptEditRow[]>('prompt_edit_list', { photoId });
}

export async function promptEditAccept(editId: number): Promise<void> {
  return tauriInvoke('prompt_edit_accept', { editId });
}

export async function promptEditReject(editId: number): Promise<void> {
  return tauriInvoke('prompt_edit_reject', { editId });
}

export interface PlaceLabelBackfillReceipt {
  scanned: number;
  labelled: number;
  skipped: number;
  elapsed_ms: number;
}

export async function backfillPlaceLabels(): Promise<PlaceLabelBackfillReceipt> {
  return tauriInvoke<PlaceLabelBackfillReceipt>('backfill_place_labels');
}

export async function mapTile(z: number, x: number, y: number): Promise<Uint8Array> {
  const bytes = await tauriInvoke<number[]>('map_tile', { z, x, y });
  return new Uint8Array(bytes);
}

export interface ShortcutRow {
  command_id: string;
  key_binding: string;
  context: string;
  updated_at: string;
}

export async function shortcutsList(): Promise<ShortcutRow[]> {
  return tauriInvoke<ShortcutRow[]>('shortcuts_list');
}

export async function shortcutsSet(commandId: string, keyBinding: string, context?: string): Promise<void> {
  return tauriInvoke('shortcuts_set', {
    commandId,
    keyBinding,
    context: context ?? null,
  });
}
