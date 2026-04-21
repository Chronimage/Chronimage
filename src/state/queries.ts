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
  deleteSource,
  detectIcloudPath,
  downloadModels,
  faceClusterMerge,
  faceClusterName,
  faceClustersList,
  findDuplicates,
  firstTimeOnNewCamera,
  getThumbnail,
  IMPORT_PROGRESS_EVENT,
  importGoogleTakeout,
  type LiftPlan,
  type LiftReceipt,
  type ListPhotosParams,
  liftShiftDryRun,
  liftShiftExecute,
  listAlbums,
  listImports,
  listIphoneDevices,
  listPhotos,
  listPhotosForCluster,
  listSources,
  listTags,
  type ModelStatus,
  onThisDay,
  type PhotoRow,
  photoLocation,
  photoQuality,
  recordPhotoView,
  refreshSmartAlbums,
  searchPhotos,
  searchSuggestions,
  startImport,
  unflaggedFavorites,
  unseenPhotos,
} from '../tauri/invoke';

export type {
  AlbumRow,
  CleanupExecuteResult,
  CleanupPlan,
  ClusterRow,
  DuplicateGroup,
  ImportProgressEvent,
  ImportSummary,
  LiftPlan,
  LiftReceipt,
  ModelSource,
  ModelStatus,
  PhotoLocation,
  PhotoQuality,
  PhotoRow,
  SourceCleanupItem,
  SourceRow,
  StartImportResponse,
  TagRow,
  UsbDevice,
} from '../tauri/invoke';
export { IMPORT_PROGRESS_EVENT };

const PHOTOS_PAGE_SIZE = 100;

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
    mutationFn: ({ name, kind, rootPath }: { name: string; kind: string; rootPath?: string }) =>
      createSource(name, kind, rootPath),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sources'] });
    },
  });
}

export function useDeleteSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (sourceId: number) => deleteSource(sourceId),
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
      } catch {
        return null;
      }
    },
    staleTime: 60 * 60_000,
    gcTime: 30 * 60_000,
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
      qc.invalidateQueries({ queryKey: ['sources'] });
      qc.invalidateQueries({ queryKey: ['cleanup'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
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
      qc.invalidateQueries({ queryKey: ['sources'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}

// ── Face clusters ─────────────────────────────────────────────────────────────

/** Photos in which at least one face belongs to the given cluster. */
export function usePhotosForCluster(clusterId: number | null, limit = 200) {
  return useQuery<PhotoRow[], Error>({
    queryKey: ['photos_for_cluster', clusterId, limit],
    enabled: typeof clusterId === 'number',
    queryFn: () =>
      typeof clusterId === 'number' ? listPhotosForCluster(clusterId, limit) : Promise.resolve([]),
    staleTime: 60_000,
  });
}

export function useFaceClusters(limit = 60) {
  return useQuery<ClusterRow[], Error>({
    queryKey: ['face-clusters', limit],
    queryFn: () => faceClustersList(limit),
  });
}

export function useFaceClusterName() {
  const qc = useQueryClient();
  return useMutation<void, Error, { clusterId: number; name: string }>({
    mutationFn: ({ clusterId, name }) => faceClusterName(clusterId, name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['face-clusters'] });
    },
  });
}

export function useFaceClusterMerge() {
  const qc = useQueryClient();
  return useMutation<number, Error, { a: number; b: number }>({
    mutationFn: ({ a, b }) => faceClusterMerge(a, b),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['face-clusters'] });
    },
  });
}

// ── AI Models status ──────────────────────────────────────────────────────────

export function useAiModelsStatus() {
  return useQuery<ModelStatus[], Error>({
    queryKey: ['ai-models-status'],
    queryFn: aiModelsStatus,
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
