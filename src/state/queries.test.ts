import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import React from 'react';
import { describe, expect, it } from 'vitest';
import {
  useAlbums,
  useCleanupDryRun,
  useCreateSource,
  useDetectIcloudPath,
  useImportGoogleTakeout,
  useImports,
  useIphoneDevices,
  useOnThisDay,
  usePhotos,
  useRefreshSmartAlbums,
  useSources,
  useStartImport,
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
});
