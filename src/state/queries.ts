/**
 * TanStack Query hooks for catalog data. These replace the static fixture
 * arrays in fixtures.ts once the Rust backend has real data.
 */

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  type AlbumRow,
  aiModelsStatus,
  type CleanupExecuteResult,
  type CleanupPlan,
  type ClusterRow,
  cleanupDryRun,
  cleanupExecute,
  createSource,
  type DuplicateGroup,
  deleteSource,
  detectIcloudPath,
  downloadModels,
  faceClusterMerge,
  faceClusterName,
  faceClustersList,
  findDuplicates,
  IMPORT_PROGRESS_EVENT,
  type ImportProgressEvent,
  type ImportSummary,
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
  listSources,
  type ModelStatus,
  onThisDay,
  type PhotoRow,
  recordPhotoView,
  refreshSmartAlbums,
  type SourceCleanupItem,
  type SourceRow,
  type StartImportResponse,
  searchPhotos,
  startImport,
  type UsbDevice,
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
  ModelStatus,
  PhotoRow,
  SourceCleanupItem,
  SourceRow,
  StartImportResponse,
  UsbDevice,
};
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
