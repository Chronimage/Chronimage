import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import React from 'react';
import { describe, expect, it } from 'vitest';
import {
  useAlbums,
  useCleanupDryRun,
  useCleanupExecute,
  useCreateSource,
  useDetectIcloudPath,
  useDuplicates,
  useFirstTimeOnNewCamera,
  useImportGoogleTakeout,
  useImports,
  useIphoneDevices,
  useOnThisDay,
  usePhotoLocation,
  usePhotoQuality,
  usePhotos,
  usePhotosForCluster,
  useRefreshSmartAlbums,
  useSearchSuggestions,
  useSources,
  useStartImport,
  useTags,
  useThumbnailUrl,
  useUnflaggedFavorites,
  useUnseenPhotos,
} from './queries';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

describe('catalog query hooks', () => {
  it('useAlbums returns empty array from mock', async () => {
    const { result } = renderHook(() => useAlbums(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('usePhotos returns flat photo array from mock', async () => {
    const { result } = renderHook(() => usePhotos(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(Array.isArray(result.current.data)).toBe(true);
  });

  it('useSources returns empty array from mock', async () => {
    const { result } = renderHook(() => useSources(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useImports returns empty array from mock', async () => {
    const { result } = renderHook(() => useImports(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useImports accepts a sourceId filter', async () => {
    const { result } = renderHook(() => useImports(1), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useCreateSource mutation succeeds and returns source row', async () => {
    const { result } = renderHook(() => useCreateSource(), { wrapper });
    await act(async () => {
      const row = await result.current.mutateAsync({ name: 'Test', kind: 'local', rootPath: '/tmp' });
      expect(row.id).toBe(1);
      expect(row.kind).toBe('local');
    });
  });

  it('useStartImport mutation succeeds and returns import_id', async () => {
    const { result } = renderHook(() => useStartImport(), { wrapper });
    await act(async () => {
      const resp = await result.current.mutateAsync({ sourceId: 1, root: '/tmp/photos' });
      expect(resp.import_id).toBe(1);
    });
  });

  it('useOnThisDay returns empty array from mock', async () => {
    const { result } = renderHook(() => useOnThisDay(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useUnseenPhotos returns empty array from mock', async () => {
    const { result } = renderHook(() => useUnseenPhotos(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useCleanupDryRun mutation resolves with mock response', async () => {
    const { result } = renderHook(() => useCleanupDryRun(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync();
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it('useRefreshSmartAlbums mutation calls refresh_smart_albums', async () => {
    const { result } = renderHook(() => useRefreshSmartAlbums(), { wrapper });
    await act(async () => {
      await result.current.mutateAsync();
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
  });

  it('useImportGoogleTakeout mutation succeeds and returns import_id', async () => {
    const { result } = renderHook(() => useImportGoogleTakeout(), { wrapper });
    await act(async () => {
      const resp = await result.current.mutateAsync({ sourceId: 1, root: '/takeout' });
      expect(resp.import_id).toBe(2);
    });
  });

  it('useDetectIcloudPath returns null from mock', async () => {
    const { result } = renderHook(() => useDetectIcloudPath(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeNull();
  });

  it('useIphoneDevices returns empty array from mock', async () => {
    const { result } = renderHook(() => useIphoneDevices(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useSearchSuggestions returns the curated mock list', async () => {
    const { result } = renderHook(() => useSearchSuggestions(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([
      'golden hour portraits',
      'sunset over water',
      'laughing at a dinner table',
    ]);
  });

  it('useTags is disabled when photoId is missing', () => {
    const { result } = renderHook(() => useTags(null), { wrapper });
    expect(result.current.fetchStatus).toBe('idle');
    expect(result.current.data).toBeUndefined();
  });

  it('useTags fetches the mock tag list when given a photoId', async () => {
    const { result } = renderHook(() => useTags(42), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('usePhotoQuality returns the mock quality shape when enabled', async () => {
    const { result } = renderHook(() => usePhotoQuality(1), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toMatchObject({ face_count: 0 });
  });

  it('usePhotoQuality is disabled when photoId is null', () => {
    const { result } = renderHook(() => usePhotoQuality(null), { wrapper });
    expect(result.current.fetchStatus).toBe('idle');
  });

  it('usePhotoLocation returns null coords from mock', async () => {
    const { result } = renderHook(() => usePhotoLocation(1), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual({ lat: null, lng: null });
  });

  it('usePhotosForCluster returns empty array from mock', async () => {
    const { result } = renderHook(() => usePhotosForCluster(7), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('usePhotosForCluster is disabled when clusterId is null', () => {
    const { result } = renderHook(() => usePhotosForCluster(null), { wrapper });
    expect(result.current.fetchStatus).toBe('idle');
  });

  it('useFirstTimeOnNewCamera returns empty array from mock', async () => {
    const { result } = renderHook(() => useFirstTimeOnNewCamera(10), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useUnflaggedFavorites returns empty array from mock', async () => {
    const { result } = renderHook(() => useUnflaggedFavorites(10, 8), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useDuplicates returns empty array from mock', async () => {
    const { result } = renderHook(() => useDuplicates(0.9), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('useCleanupExecute resolves with the configured mock payload', async () => {
    const { result } = renderHook(() => useCleanupExecute(), { wrapper });
    await act(async () => {
      const r = await result.current.mutateAsync({
        planId: 'plan-abc',
        confirmToken: 'tok-xyz',
      });
      expect(r.deleted_count).toBe(0);
      expect(r.errors).toEqual([]);
    });
  });

  it('useThumbnailUrl resolves to null when command throws', async () => {
    const { result } = renderHook(() => useThumbnailUrl(99, 128), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeNull();
  });

  it('useThumbnailUrl is disabled when photoId is null', () => {
    const { result } = renderHook(() => useThumbnailUrl(null), { wrapper });
    expect(result.current.fetchStatus).toBe('idle');
  });
});
