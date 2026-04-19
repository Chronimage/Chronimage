/**
 * TanStack Query hooks for catalog data. These replace the static fixture
 * arrays in fixtures.ts once the Rust backend has real data.
 */

import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  type AlbumRow,
  type CleanupPlan,
  cleanupDryRun,
  createSource,
  IMPORT_PROGRESS_EVENT,
  type ImportProgressEvent,
  type ImportSummary,
  type ListPhotosParams,
  listAlbums,
  listImports,
  listPhotos,
  listSources,
  onThisDay,
  type PhotoRow,
  refreshSmartAlbums,
  type SourceCleanupItem,
  type SourceRow,
  type StartImportResponse,
  startImport,
  unseenPhotos,
} from '../tauri/invoke';

export type {
  AlbumRow,
  CleanupPlan,
  ImportProgressEvent,
  ImportSummary,
  PhotoRow,
  SourceCleanupItem,
  SourceRow,
  StartImportResponse,
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

export function useCleanupDryRun(sourceId?: number) {
  return useQuery({
    queryKey: ['cleanup_dry_run', sourceId],
    queryFn: () => cleanupDryRun(sourceId),
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
