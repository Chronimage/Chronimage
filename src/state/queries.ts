/**
 * TanStack Query hooks for catalog data. These replace the static fixture
 * arrays in fixtures.ts once the Rust backend has real data.
 *
 * Usage:
 *   const { data: albums = [] } = useAlbums();
 *   const { data: photos = [], fetchNextPage } = usePhotos();
 *   const { data: sources = [] } = useSources();
 */

import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import {
  type AlbumRow,
  type ListPhotosParams,
  listAlbums,
  listPhotos,
  listSources,
  type PhotoRow,
  type SourceRow,
} from '../tauri/invoke';

export type { AlbumRow, PhotoRow, SourceRow };

const PHOTOS_PAGE_SIZE = 100;

export function useAlbums() {
  return useQuery({
    queryKey: ['albums'],
    queryFn: listAlbums,
  });
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
  return useQuery({
    queryKey: ['sources'],
    queryFn: listSources,
  });
}
