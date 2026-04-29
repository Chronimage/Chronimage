/**
 * TanStack Query hooks for catalog data. These replace the static fixture
 * arrays in fixtures.ts once the Rust backend has real data.
 */

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  aiModelsStatus,
  aiReindex,
  type ClusterRow,
  cleanupDryRun,
  cleanupExecute,
  createSource,
  type DiskInfo,
  deleteSource,
  detectIcloudPath,
  downloadModels,
  faceAssignCluster,
  faceClustersList,
  faceCreatePersonFromFace,
  faceUnassign,
  findDuplicates,
  firstTimeOnNewCamera,
  generateAiTags,
  getDefaultCatalogPath,
  getDiskInfo,
  getThumbnail,
  IMPORT_PROGRESS_EVENT,
  importDryRun,
  importGoogleTakeout,
  type LiftPlan,
  type LiftReceipt,
  type ListPhotosParams,
  liftShiftDryRun,
  liftShiftExecute,
  listAlbums,
  listFacesForPhoto,
  listImports,
  listIphoneDevices,
  listPhotos,
  listSources,
  listTags,
  type ModelStatus,
  onThisDay,
  type PhotoFaceRow,
  type PhotoRow,
  photoLocation,
  photoQuality,
  REBUILD_PROGRESS_EVENT,
  type RebuildReceipt,
  type RecycleReceipt,
  type RemovePreview,
  type RemoveReceipt,
  rebuildThumbnails,
  recordPhotoView,
  recycleSourceCopies,
  refreshSmartAlbums,
  removePhotosFromCatalog,
  removePhotosPreview,
  type SourceDeletionPlan,
  searchPhotos,
  searchSuggestions,
  sourceDeletionPreview,
  startImport,
  unflaggedFavorites,
  unseenPhotos,
} from '../tauri/invoke';
import { warn } from '../util/log';
import { clearPhotoScopedQueries, resetCatalogContentQueries } from './queryInvalidation';
import { useSourceDeleteStore } from './sourceDelete';

export type {
  AlbumRow,
  CleanupExecuteResult,
  CleanupPlan,
  ClusterRow,
  DiskInfo,
  DuplicateGroup,
  ImportProgressEvent,
  ImportSummary,
  LiftPlan,
  LiftReceipt,
  ModelSource,
  ModelStatus,
  PhotoFaceRow,
  PhotoLocation,
  PhotoQuality,
  PhotoRow,
  RebuildReceipt,
  RecycleReceipt,
  RemovePreview,
  RemoveReceipt,
  SourceCleanupItem,
  SourceDeletionPlan,
  SourceRow,
  StartImportResponse,
  TagRow,
  UsbDevice,
} from '../tauri/invoke';
export { IMPORT_PROGRESS_EVENT, REBUILD_PROGRESS_EVENT };

const PHOTOS_PAGE_SIZE = 100;
const PHOTOS_MAX_PAGES = 20;

// ── Read hooks ────────────────────────────────────────────────────────────────

export function useAlbums() {
  return useQuery({ queryKey: ['albums'], queryFn: listAlbums });
}

export function usePhotos(params?: Omit<ListPhotosParams, 'offset'>) {
  return useInfiniteQuery({
    queryKey: ['photos', params],
    queryFn: ({ pageParam }) => listPhotos({ ...params, limit: PHOTOS_PAGE_SIZE, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) => {
      if (lastPage.length < PHOTOS_PAGE_SIZE) return undefined;
      return allPages.length * PHOTOS_PAGE_SIZE;
    },
    maxPages: PHOTOS_MAX_PAGES,
    select: (data) => data.pages.flat(),
  });
}

export function useSources() {
  return useQuery({ queryKey: ['sources'], queryFn: listSources });
}

export function useImports(sourceId?: number) {
  return useQuery({
    queryKey: ['imports', sourceId],
    queryFn: () => listImports(sourceId),
  });
}

/**
 * Dry-run a folder scan (no writes) and return the count / pairs / per-
 * extension breakdown. Used by the copy-confirmation dialog to tell the
 * user how many photos are about to be copied before they hit OK.
 *
 * `null` root disables the query so the dialog can mount/unmount without
 * spurious scans.
 */
export function useScanPreview(root: string | null) {
  return useQuery({
    queryKey: ['scan_preview', root],
    queryFn: () => importDryRun(root as string),
    enabled: root !== null,
    staleTime: 30_000,
  });
}

export function useOnThisDay(limit?: number) {
  return useQuery({
    queryKey: ['on_this_day', limit],
    queryFn: () => onThisDay(limit),
  });
}

export function useUnseenPhotos(limit?: number, minScore?: number) {
  return useQuery({
    queryKey: ['unseen_photos', limit, minScore],
    queryFn: () => unseenPhotos(limit, minScore),
  });
}

export function useFirstTimeOnNewCamera(limit?: number) {
  return useQuery({
    queryKey: ['first_time_on_new_camera', limit],
    queryFn: () => firstTimeOnNewCamera(limit),
  });
}

export function useUnflaggedFavorites(limit?: number, minScore?: number) {
  return useQuery({
    queryKey: ['unflagged_favorites', limit, minScore],
    queryFn: () => unflaggedFavorites(limit, minScore),
  });
}

export function useRefreshSmartAlbums() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: refreshSmartAlbums,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['albums'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

// ── Mutation hooks ────────────────────────────────────────────────────────────

export function useCreateSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      name,
      kind,
      rootPath,
      absorbOverlappingChildren,
    }: {
      name: string;
      kind: string;
      rootPath?: string;
      absorbOverlappingChildren?: boolean;
    }) => createSource(name, kind, rootPath, absorbOverlappingChildren),
    onSuccess: () => {
      // Sources panel + anything that pivots on source existence (empty
      // state, default selected album, rediscovery rows) should reflect
      // the new source immediately — before its import even starts.
      resetCatalogContentQueries(qc);
    },
  });
}

export function useDeleteSource() {
  const qc = useQueryClient();
  const registerDelete = useSourceDeleteStore((s) => s.register);
  return useMutation<
    RemoveReceipt,
    Error,
    {
      sourceId: number;
      sourceName?: string;
    }
  >({
    mutationFn: ({ sourceId }) => deleteSource(sourceId),
    onMutate: ({ sourceId, sourceName }) => {
      // Surface the disconnect card immediately so the user sees activity
      // even if the first backend `collecting` event lands a moment later.
      registerDelete(sourceId, sourceName ?? `Source ${sourceId}`);
      clearPhotoScopedQueries(qc);
    },
    onSuccess: () => {
      // The progress listener already invalidates these on `committed`
      // and `done`. We re-fire on success as a safety net for the case
      // where the event listener is unmounted (e.g. error in Tauri bridge).
      resetCatalogContentQueries(qc);
      qc.invalidateQueries({ queryKey: ['cleanup'] });
    },
  });
}

export function useSourceDeletionPreview(sourceId: number | null) {
  return useQuery<SourceDeletionPlan, Error>({
    queryKey: ['source_deletion_preview', sourceId] as const,
    queryFn: () =>
      typeof sourceId === 'number'
        ? sourceDeletionPreview(sourceId)
        : Promise.reject(new Error('no source_id')),
    enabled: typeof sourceId === 'number',
    staleTime: 10_000,
  });
}

export function useRemovePhotosPreview(photoIds: number[]) {
  const key = photoIds
    .slice()
    .sort((a, b) => a - b)
    .join(',');
  return useQuery<RemovePreview, Error>({
    queryKey: ['remove_photos_preview', key] as const,
    queryFn: () => removePhotosPreview(photoIds),
    enabled: photoIds.length > 0,
    staleTime: 10_000,
  });
}

export function useRemovePhotosFromCatalog() {
  const qc = useQueryClient();
  return useMutation<RemoveReceipt, Error, number[]>({
    mutationFn: (photoIds: number[]) => removePhotosFromCatalog(photoIds),
    onSuccess: () => {
      resetCatalogContentQueries(qc);
    },
  });
}

export function useRecycleSourceCopies() {
  const qc = useQueryClient();
  return useMutation<RecycleReceipt, Error, number[]>({
    mutationFn: (photoIds: number[]) => recycleSourceCopies(photoIds),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sources'] });
      qc.invalidateQueries({ queryKey: ['cleanup'] });
    },
  });
}

export function useStartImport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sourceId, root }: { sourceId: number; root: string }) => startImport(sourceId, root),
    onMutate: () => {
      clearPhotoScopedQueries(qc);
    },
    onSuccess: (_data, { sourceId }) => {
      qc.invalidateQueries({ queryKey: ['imports', sourceId] });
      qc.invalidateQueries({ queryKey: ['imports'] });
    },
  });
}

export function useImportGoogleTakeout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sourceId, root }: { sourceId: number; root: string }) =>
      importGoogleTakeout(sourceId, root),
    onMutate: () => {
      clearPhotoScopedQueries(qc);
    },
    onSuccess: (_data, { sourceId }) => {
      qc.invalidateQueries({ queryKey: ['imports', sourceId] });
      qc.invalidateQueries({ queryKey: ['imports'] });
    },
  });
}

export function useDetectIcloudPath() {
  return useQuery({ queryKey: ['icloud_path'], queryFn: detectIcloudPath });
}

export function useIphoneDevices() {
  return useQuery({ queryKey: ['iphone_devices'], queryFn: listIphoneDevices });
}

export function useDownloadModels() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (names?: string[]) => downloadModels(names),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['models'] });
      qc.invalidateQueries({ queryKey: ['ai-models-status'] });
    },
  });
}

/**
 * Truncate + recompute data produced by `kind` after a model swap.
 * kind ∈ {"embeddings", "face-detect", "face-embed", "aesthetic", "captions"}.
 * Returns count of affected rows (for the success toast).
 */
export function useAiReindex() {
  const qc = useQueryClient();
  return useMutation<number, Error, string>({
    mutationFn: (kind: string) => aiReindex(kind),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['ai-models-status'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

export function useDuplicates(minSimilarity?: number) {
  return useQuery({
    queryKey: ['duplicates', minSimilarity] as const,
    queryFn: () => findDuplicates(minSimilarity),
  });
}

export function useSearchPhotos(query: string) {
  return useQuery<PhotoRow[], Error>({
    queryKey: ['search_photos', query],
    queryFn: () => searchPhotos(query),
    enabled: query.trim().length > 0,
    staleTime: 30_000,
    placeholderData: (prev) => prev,
  });
}

export function useSearchSuggestions() {
  return useQuery<string[], Error>({
    queryKey: ['search_suggestions'],
    queryFn: searchSuggestions,
    staleTime: 5 * 60_000,
  });
}

/**
 * Thumbnail for a single photo, returned as an object URL (revokes automatically
 * when the consumer unmounts or the query is garbage-collected). Returns
 * `undefined` on error so the caller can fall back to a placeholder.
 */
/** GPS coordinates for the detail-inspector Location section. */
export function usePhotoLocation(photoId: number | null | undefined) {
  return useQuery<import('../tauri/invoke').PhotoLocation | null, Error>({
    queryKey: ['photo_location', photoId],
    enabled: typeof photoId === 'number',
    queryFn: () => (typeof photoId === 'number' ? photoLocation(photoId) : Promise.resolve(null)),
    staleTime: 5 * 60_000,
  });
}

/** Aggregate quality metrics for the detail-inspector Quality section. */
export function usePhotoQuality(photoId: number | null | undefined) {
  return useQuery<import('../tauri/invoke').PhotoQuality | null, Error>({
    queryKey: ['photo_quality', photoId],
    enabled: typeof photoId === 'number',
    queryFn: () => (typeof photoId === 'number' ? photoQuality(photoId) : Promise.resolve(null)),
    staleTime: 60_000,
  });
}

/** Tags attached to a photo — AI labels + user tags, sorted by confidence. */
export function useTags(photoId: number | null | undefined) {
  return useQuery<import('../tauri/invoke').TagRow[], Error>({
    queryKey: ['tags', photoId],
    enabled: typeof photoId === 'number',
    queryFn: () => (typeof photoId === 'number' ? listTags(photoId) : Promise.resolve([])),
    staleTime: 60_000,
  });
}

export function useGenerateAiTags() {
  const qc = useQueryClient();
  return useMutation<import('../tauri/invoke').TagRow[], Error, number>({
    mutationFn: (photoId) => generateAiTags(photoId),
    onSuccess: (rows, photoId) => {
      qc.setQueryData(['tags', photoId], rows);
      qc.invalidateQueries({ queryKey: ['photos'] });
      qc.invalidateQueries({ queryKey: ['search_suggestions'] });
    },
  });
}

export function useThumbnailUrl(photoId: number | null | undefined, sizePx = 320) {
  return useQuery<string | null, Error>({
    queryKey: ['thumbnail', photoId, sizePx],
    enabled: typeof photoId === 'number',
    queryFn: async () => {
      if (typeof photoId !== 'number') return null;
      try {
        const bytes = await getThumbnail(photoId, sizePx);
        // Copy into an owned ArrayBuffer so TS accepts it as a BlobPart
        // (the Tauri bridge may return a Uint8Array over a SharedArrayBuffer).
        const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
        const blob = new Blob([buf as ArrayBuffer], { type: 'image/jpeg' });
        return URL.createObjectURL(blob);
      } catch (err) {
        warn('[thumbnail] get_thumbnail failed for photo', photoId, err);
        return null;
      }
    },
    staleTime: 60 * 60_000,
    gcTime: 5 * 60_000,
  });
}

// ── Cleanup ────────────────────────────────────────────────────────────────

export function useCleanupDryRun() {
  return useMutation({
    mutationFn: () => cleanupDryRun(),
  });
}

export function useCleanupExecute() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ planId, confirmToken }: { planId: string; confirmToken: string }) =>
      cleanupExecute(planId, confirmToken),
    onSuccess: () => {
      resetCatalogContentQueries(qc);
      qc.invalidateQueries({ queryKey: ['cleanup'] });
    },
  });
}

// ── Lift & Shift ───────────────────────────────────────────────────────────

export function useLiftShiftDryRun() {
  return useMutation<LiftPlan, Error, { targetRoot: string }>({
    mutationFn: ({ targetRoot }) => liftShiftDryRun(targetRoot),
  });
}

export function useLiftShiftExecute() {
  const qc = useQueryClient();
  return useMutation<LiftReceipt, Error, { planId: string; confirmToken: string }>({
    mutationFn: ({ planId, confirmToken }) => liftShiftExecute(planId, confirmToken),
    onSuccess: () => {
      resetCatalogContentQueries(qc);
    },
  });
}

// ── Face clusters ─────────────────────────────────────────────────────────────

export function useFaceClusters(limit = 60) {
  return useQuery<ClusterRow[], Error>({
    queryKey: ['face-clusters', limit],
    queryFn: () => faceClustersList(limit),
  });
}

export function useFacesForPhoto(photoId: number | null | undefined) {
  return useQuery<PhotoFaceRow[], Error>({
    queryKey: ['photo_faces', photoId],
    enabled: typeof photoId === 'number',
    queryFn: () => (typeof photoId === 'number' ? listFacesForPhoto(photoId) : Promise.resolve([])),
    staleTime: 60_000,
  });
}

export function useFaceAssignCluster() {
  const qc = useQueryClient();
  return useMutation<void, Error, { faceId: number; clusterId: number; photoId?: number }>({
    mutationFn: ({ faceId, clusterId }) => faceAssignCluster(faceId, clusterId),
    onSuccess: (_result, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['face-clusters'] });
      if (typeof photoId === 'number') qc.invalidateQueries({ queryKey: ['photo_faces', photoId] });
    },
  });
}

export function useFaceCreatePersonFromFace() {
  const qc = useQueryClient();
  return useMutation<number, Error, { faceId: number; name: string; photoId?: number }>({
    mutationFn: ({ faceId, name }) => faceCreatePersonFromFace(faceId, name),
    onSuccess: (_clusterId, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['face-clusters'] });
      if (typeof photoId === 'number') qc.invalidateQueries({ queryKey: ['photo_faces', photoId] });
    },
  });
}

export function useFaceUnassign() {
  const qc = useQueryClient();
  return useMutation<void, Error, { faceId: number; photoId?: number }>({
    mutationFn: ({ faceId }) => faceUnassign(faceId),
    onSuccess: (_result, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['face-clusters'] });
      if (typeof photoId === 'number') qc.invalidateQueries({ queryKey: ['photo_faces', photoId] });
    },
  });
}

// ── AI Models status ──────────────────────────────────────────────────────────

export function useAiModelsStatus() {
  return useQuery<ModelStatus[], Error>({
    queryKey: ['ai-models-status'],
    queryFn: aiModelsStatus,
    staleTime: 5 * 60 * 1000,
  });
}

// ── View tracking ─────────────────────────────────────────────────────────────

/**
 * Record a photo-view event. Fire-and-forget mutation fired when the Detail
 * overlay opens — unblocks the `LastViewed` rule predicate and the
 * "Unseen in 2 years" rediscovery album.
 */
export function useRecordPhotoView() {
  return useMutation<void, Error, number>({
    mutationFn: (photoId: number) => recordPhotoView(photoId),
  });
}

// ── Disk / catalog-home helpers ───────────────────────────────────────────────

export function useDefaultCatalogPath() {
  return useQuery<string, Error>({
    queryKey: ['default_catalog_path'],
    queryFn: getDefaultCatalogPath,
    staleTime: Infinity,
  });
}

export function useDiskInfo(path: string | undefined) {
  return useQuery<DiskInfo, Error>({
    queryKey: ['disk_info', path],
    queryFn: () => (path ? getDiskInfo(path) : Promise.reject(new Error('no path'))),
    enabled: !!path,
    staleTime: 60_000,
  });
}

// ── Thumbnail rebuild ───────────────────────────────────────────────────────

/**
 * Trigger a full rebuild of the 320 px thumbnail cache — used to repair
 * pre-orientation-fix catalogs without a re-import. Invalidates the
 * thumbnail cache so every `<Thumbnail>` re-fetches.
 */
export function useRebuildThumbnails() {
  const qc = useQueryClient();
  return useMutation<RebuildReceipt, Error, void>({
    mutationFn: () => rebuildThumbnails(),
    onSuccess: () => {
      qc.removeQueries({ queryKey: ['thumbnail'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

// ── Phase 2: Cull + Rate + Flag ───────────────────────────────────────────────

import {
  addUserTag,
  type CullBinFilter,
  type CullBinRow,
  type CullBinSummary,
  type CullReason,
  type CullVerdict,
  cullApplyVerdict,
  cullBinDeleteForever,
  cullBinList,
  cullBinRestore,
  cullBinSummary,
  cullBinSweep,
  type EmptyReceipt,
  type ExportJob,
  type ExportPreset,
  type ExportProgress,
  exportEnqueue,
  exportListJobs,
  exportRunNext,
  flagPhoto,
  listUserTags,
  type RestoreReceipt,
  ratePhoto,
  removeUserTag,
  renameUserTag,
  type UserTagSummary,
  type VerdictReceipt,
} from '../tauri/invoke';

export type {
  CullBinFilter,
  CullBinRow,
  CullBinSummary,
  CullReason,
  CullVerdict,
  EmptyReceipt,
  ExportJob,
  ExportPreset,
  ExportProgress,
  RestoreReceipt,
  UserTagSummary,
  VerdictReceipt,
};

export function useCullApplyVerdict() {
  const qc = useQueryClient();
  return useMutation<
    VerdictReceipt,
    Error,
    { photoId: number; verdict: CullVerdict; reason: CullReason; retentionDays?: number }
  >({
    mutationFn: ({ photoId, verdict, reason, retentionDays }) =>
      cullApplyVerdict(photoId, verdict, reason, retentionDays),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['photos'] });
      qc.invalidateQueries({ queryKey: ['cull_bin'] });
      qc.invalidateQueries({ queryKey: ['cull_bin_summary'] });
    },
  });
}

export function useRatePhoto() {
  const qc = useQueryClient();
  return useMutation<void, Error, { photoId: number; rating: number }>({
    mutationFn: ({ photoId, rating }) => ratePhoto(photoId, rating),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

export function useFlagPhoto() {
  const qc = useQueryClient();
  return useMutation<boolean, Error, number>({
    mutationFn: (photoId: number) => flagPhoto(photoId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

export function useCullBin(filter?: CullBinFilter) {
  return useQuery({
    queryKey: ['cull_bin', filter ?? 'all'],
    queryFn: () => cullBinList(filter),
  });
}

export function useCullBinSummary() {
  return useQuery({
    queryKey: ['cull_bin_summary'],
    queryFn: async () => (await cullBinSummary()) ?? { total_count: 0, total_bytes: 0, by_reason: [] },
  });
}

export function useCullBinRestore() {
  const qc = useQueryClient();
  return useMutation<RestoreReceipt, Error, number[]>({
    mutationFn: (ids) => cullBinRestore(ids),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['cull_bin'] });
      qc.invalidateQueries({ queryKey: ['cull_bin_summary'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

export function useCullBinDeleteForever() {
  const qc = useQueryClient();
  return useMutation<EmptyReceipt, Error, number[]>({
    mutationFn: (ids) => cullBinDeleteForever(ids),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['cull_bin'] });
      qc.invalidateQueries({ queryKey: ['cull_bin_summary'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

export function useCullBinSweep() {
  const qc = useQueryClient();
  return useMutation<EmptyReceipt, Error, void>({
    mutationFn: () => cullBinSweep(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['cull_bin'] });
      qc.invalidateQueries({ queryKey: ['cull_bin_summary'] });
    },
  });
}

// ── Phase 2: Export ───────────────────────────────────────────────────────────

export function useExportJobs() {
  return useQuery({ queryKey: ['export_jobs'], queryFn: () => exportListJobs() });
}

export function useExportEnqueue() {
  const qc = useQueryClient();
  return useMutation<number, Error, { photoIds: number[]; preset: ExportPreset; outputDir: string }>({
    mutationFn: ({ photoIds, preset, outputDir }) => exportEnqueue(photoIds, preset, outputDir),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['export_jobs'] });
    },
  });
}

export function useExportRunNext() {
  const qc = useQueryClient();
  return useMutation<ExportProgress | null, Error, number>({
    mutationFn: (jobId) => exportRunNext(jobId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['export_jobs'] });
    },
  });
}

// ── Phase 2 §10: Manual tagging ───────────────────────────────────────────────

export function useUserTags() {
  return useQuery({ queryKey: ['user_tags'], queryFn: () => listUserTags() });
}

export function useAddUserTag() {
  const qc = useQueryClient();
  return useMutation<number, Error, { photoIds: number[]; label: string }>({
    mutationFn: ({ photoIds, label }) => addUserTag(photoIds, label),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['user_tags'] });
      qc.invalidateQueries({ queryKey: ['tags'] });
    },
  });
}

export function useRemoveUserTag() {
  const qc = useQueryClient();
  return useMutation<number, Error, { photoIds: number[]; label: string }>({
    mutationFn: ({ photoIds, label }) => removeUserTag(photoIds, label),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['user_tags'] });
      qc.invalidateQueries({ queryKey: ['tags'] });
    },
  });
}

export function useRenameUserTag() {
  const qc = useQueryClient();
  return useMutation<number, Error, { oldLabel: string; newLabel: string }>({
    mutationFn: ({ oldLabel, newLabel }) => renameUserTag(oldLabel, newLabel),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['user_tags'] });
      qc.invalidateQueries({ queryKey: ['tags'] });
    },
  });
}

// ── Phase 3: Develop ──────────────────────────────────────────────────────────

import {
  type AiEditRefreshReceipt,
  type AiEditRow,
  aiEditRefresh,
  aiEditStatus,
  type DevelopHistoryRow,
  type DevelopMask,
  type DevelopMaskCreateRequest,
  type DevelopMaskFaceEntry,
  type DevelopMaskGenerateReceipt,
  type DevelopMaskGenerateRequest,
  type DevelopMaskUpdateRequest,
  type DevelopOpenResponse,
  type DevelopOperations,
  type DevelopPreset,
  developAdaptivePresetApply,
  developApply,
  developCopyEdits,
  developHistoryList,
  developMaskApplyPreview,
  developMaskCreate,
  developMaskDelete,
  developMaskGenerate,
  developMaskListFaces,
  developMasksList,
  developMaskUpdate,
  developOpen,
  developPasteEdits,
  developPresetApply,
  developReset,
  developSave,
  developSnapshotSave,
  type PastedReceipt,
  presetsList,
  type RenderReceipt,
} from '../tauri/invoke';

export type {
  AiEditRefreshReceipt,
  AiEditRow,
  DevelopHistoryRow,
  DevelopMask,
  DevelopMaskCreateRequest,
  DevelopMaskUpdateRequest,
  DevelopOpenResponse,
  DevelopOperations,
  DevelopPreset,
  PastedReceipt,
  RenderReceipt,
};

export function useDevelopOpen(photoId: number | null) {
  return useQuery({
    queryKey: ['develop_open', photoId],
    queryFn: () => developOpen(photoId as number),
    enabled: photoId != null,
  });
}

export function useDevelopApply() {
  return useMutation<
    RenderReceipt,
    Error,
    { photoId: number; operations: DevelopOperations; previewLongEdge?: number }
  >({
    mutationFn: ({ photoId, operations, previewLongEdge }) =>
      developApply(photoId, operations, previewLongEdge),
  });
}

export function useDevelopSave() {
  const qc = useQueryClient();
  return useMutation<number, Error, { photoId: number; operations: DevelopOperations; label?: string }>({
    mutationFn: ({ photoId, operations, label }) => developSave(photoId, operations, label),
    onSuccess: (_id, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['develop_open', photoId] });
    },
  });
}

export function useDevelopSnapshotSave() {
  const qc = useQueryClient();
  return useMutation<number, Error, { photoId: number; operations: DevelopOperations; label?: string }>({
    mutationFn: ({ photoId, operations, label }) => developSnapshotSave(photoId, operations, label),
    onSuccess: (_id, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['develop_open', photoId] });
      qc.invalidateQueries({ queryKey: ['develop_history', photoId] });
    },
  });
}

export function useDevelopHistory(photoId: number | null) {
  return useQuery<DevelopHistoryRow[], Error>({
    queryKey: ['develop_history', photoId],
    queryFn: () => developHistoryList(photoId as number),
    enabled: photoId != null,
  });
}

export function useDevelopReset() {
  const qc = useQueryClient();
  return useMutation<number, Error, number>({
    mutationFn: (photoId) => developReset(photoId),
    onSuccess: (_n, photoId) => {
      qc.invalidateQueries({ queryKey: ['develop_open', photoId] });
    },
  });
}

export function useDevelopCopyEdits() {
  return useMutation<DevelopOperations, Error, number>({
    mutationFn: (photoId) => developCopyEdits(photoId),
  });
}

export function useDevelopPasteEdits() {
  const qc = useQueryClient();
  return useMutation<PastedReceipt, Error, { photoIds: number[]; operations: DevelopOperations }>({
    mutationFn: ({ photoIds, operations }) => developPasteEdits(photoIds, operations),
    onSuccess: (_r, { photoIds }) => {
      for (const id of photoIds) {
        qc.invalidateQueries({ queryKey: ['develop_open', id] });
      }
    },
  });
}

export function useDevelopPresetApply() {
  return useMutation<RenderReceipt, Error, { photoId: number; presetId: number; strength: number }>({
    mutationFn: ({ photoId, presetId, strength }) => developPresetApply(photoId, presetId, strength),
  });
}

export function useDevelopAdaptivePresetApply() {
  const qc = useQueryClient();
  return useMutation<RenderReceipt, Error, { photoId: number; presetId: number; strength: number }>({
    mutationFn: ({ photoId, presetId, strength }) => developAdaptivePresetApply(photoId, presetId, strength),
    onSuccess: (_receipt, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['develop_masks', photoId] });
      qc.invalidateQueries({ queryKey: ['develop_open', photoId] });
    },
  });
}

export function usePresets(group?: string) {
  return useQuery({
    queryKey: ['presets', group ?? 'all'],
    queryFn: () => presetsList(group),
  });
}

export function useDevelopMasks(photoId: number | null) {
  return useQuery<DevelopMask[], Error>({
    queryKey: ['develop_masks', photoId],
    queryFn: () => developMasksList(photoId as number),
    enabled: photoId != null,
  });
}

export function useDevelopMaskFaces(photoId: number | null) {
  return useQuery<DevelopMaskFaceEntry[], Error>({
    queryKey: ['develop_mask_faces', photoId],
    queryFn: () => developMaskListFaces(photoId as number),
    enabled: photoId != null,
    // Faces are stored at import; they don't change while the editor
    // is open. Cached aggressively so flipping between photos in the
    // filmstrip doesn't refetch each time.
    staleTime: 60_000,
  });
}

export function useDevelopMaskCreate() {
  const qc = useQueryClient();
  return useMutation<number, Error, DevelopMaskCreateRequest>({
    mutationFn: (req) => developMaskCreate(req),
    onSuccess: (_id, req) => {
      qc.invalidateQueries({ queryKey: ['develop_masks', req.photo_id] });
      qc.invalidateQueries({ queryKey: ['develop_open', req.photo_id] });
    },
  });
}

export function useDevelopMaskGenerate() {
  const qc = useQueryClient();
  return useMutation<DevelopMaskGenerateReceipt, Error, DevelopMaskGenerateRequest>({
    mutationFn: (req) => developMaskGenerate(req),
    onSuccess: (receipt, req) => {
      qc.invalidateQueries({ queryKey: ['develop_masks', req.photo_id] });
      qc.invalidateQueries({ queryKey: ['develop_open', req.photo_id] });
      qc.setQueryData<DevelopMask[]>(['develop_masks', req.photo_id], (current) => {
        const rows = current ?? [];
        return [...rows.filter((mask) => mask.id !== receipt.mask.id), receipt.mask].sort(
          (a, b) => a.order_index - b.order_index || a.id - b.id,
        );
      });
    },
  });
}

export function useDevelopMaskUpdate() {
  const qc = useQueryClient();
  return useMutation<DevelopMask, Error, DevelopMaskUpdateRequest>({
    mutationFn: (req) => developMaskUpdate(req),
    onSuccess: (mask) => {
      qc.invalidateQueries({ queryKey: ['develop_masks', mask.photo_id] });
      qc.invalidateQueries({ queryKey: ['develop_open', mask.photo_id] });
    },
  });
}

export function useDevelopMaskDelete() {
  const qc = useQueryClient();
  return useMutation<number, Error, { maskId: number; photoId: number }>({
    mutationFn: ({ maskId }) => developMaskDelete(maskId),
    onSuccess: (_deleted, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['develop_masks', photoId] });
      qc.invalidateQueries({ queryKey: ['develop_open', photoId] });
    },
  });
}

export function useDevelopMaskApplyPreview() {
  return useMutation<RenderReceipt, Error, { photoId: number; operations: DevelopOperations }>({
    mutationFn: ({ photoId, operations }) => developMaskApplyPreview(photoId, operations),
  });
}

export function useAiEditStatus(photoId: number | null) {
  return useQuery<AiEditRow[], Error>({
    queryKey: ['ai_edit_status', photoId],
    queryFn: () => aiEditStatus(photoId as number),
    enabled: photoId != null,
  });
}

export function useAiEditRefresh() {
  const qc = useQueryClient();
  return useMutation<AiEditRefreshReceipt, Error, { photoId: number; feature: string }>({
    mutationFn: ({ photoId, feature }) => aiEditRefresh(photoId, feature),
    onSuccess: (_receipt, { photoId }) => {
      qc.invalidateQueries({ queryKey: ['ai_edit_status', photoId] });
    },
  });
}
