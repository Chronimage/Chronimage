import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { describe, expect, it, vi } from 'vitest';
import {
  appVersion,
  cleanupDryRun,
  createSource,
  currentChannel,
  deleteSource,
  detectHardware,
  detectIcloudPath,
  embedImage,
  gphotosUpload,
  gphotosUploadScopeOk,
  importDryRun,
  importGoogleTakeout,
  listAlbums,
  listImports,
  listIphoneDevices,
  listPhotos,
  listSources,
  onedriveAuthStatus,
  onedriveUpload,
  onThisDay,
  ping,
  refreshSmartAlbums,
  scoreAesthetic,
  startImport,
  unseenPhotos,
} from './invoke';

describe('invoke wrappers', () => {
  it('ping calls ping command', async () => {
    expect(await ping()).toBe('pong');
  });

  it('appVersion calls app_version command', async () => {
    expect(await appVersion()).toBe('0.0.0-test');
  });

  it('currentChannel calls current_channel command', async () => {
    const c = await currentChannel();
    expect(c.channel).toBe('dev');
  });

  it('listAlbums calls list_albums and returns array', async () => {
    expect(Array.isArray(await listAlbums())).toBe(true);
  });

  it('listPhotos calls list_photos with default params', async () => {
    expect(Array.isArray(await listPhotos())).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', {
      limit: null,
      offset: null,
      albumId: null,
      sortBy: null,
    });
  });

  it('listPhotos forwards limit and offset', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await listPhotos({ limit: 50, offset: 200 });
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', {
      limit: 50,
      offset: 200,
      albumId: null,
      sortBy: null,
    });
  });

  it('listPhotos forwards albumId', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await listPhotos({ albumId: 3 });
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', {
      limit: null,
      offset: null,
      albumId: 3,
      sortBy: null,
    });
  });

  it('refreshSmartAlbums calls refresh_smart_albums', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce(undefined);
    await refreshSmartAlbums();
    expect(tauriInvoke).toHaveBeenCalledWith('refresh_smart_albums');
  });

  it('listSources calls list_sources and returns array', async () => {
    expect(Array.isArray(await listSources())).toBe(true);
  });

  it('createSource calls create_source with correct args', async () => {
    const row = await createSource('Local D:', 'local', 'D:/Photos');
    expect(row.id).toBe(1);
    expect(tauriInvoke).toHaveBeenCalledWith('create_source', {
      name: 'Local D:',
      kind: 'local',
      rootPath: 'D:/Photos',
    });
  });

  it('createSource passes null rootPath when omitted', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce({
      id: 2,
      name: 'iCloud',
      kind: 'icloud',
      status: 'idle',
      last_scan_at: null,
      photo_count: 0,
    });
    await createSource('iCloud', 'icloud');
    expect(tauriInvoke).toHaveBeenCalledWith('create_source', {
      name: 'iCloud',
      kind: 'icloud',
      rootPath: null,
    });
  });

  it('startImport calls start_import and returns import_id', async () => {
    const resp = await startImport(1, 'D:/Photos');
    expect(resp.import_id).toBe(1);
    expect(tauriInvoke).toHaveBeenCalledWith('start_import', { sourceId: 1, root: 'D:/Photos' });
  });

  it('onThisDay calls on_this_day and returns array', async () => {
    expect(Array.isArray(await onThisDay())).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('on_this_day', { limit: null });
  });

  it('onThisDay forwards limit', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await onThisDay(10);
    expect(tauriInvoke).toHaveBeenCalledWith('on_this_day', { limit: 10 });
  });

  it('unseenPhotos calls unseen_photos and returns array', async () => {
    expect(Array.isArray(await unseenPhotos())).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('unseen_photos', { limit: null, minScore: null });
  });

  it('unseenPhotos forwards limit and minScore', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await unseenPhotos(15, 6.5);
    expect(tauriInvoke).toHaveBeenCalledWith('unseen_photos', { limit: 15, minScore: 6.5 });
  });

  it('cleanupDryRun calls cleanup_dry_run with no args', async () => {
    await cleanupDryRun();
    expect(tauriInvoke).toHaveBeenCalledWith('cleanup_dry_run');
  });

  it('importDryRun calls import_dry_run and returns scan report', async () => {
    const report = await importDryRun('/photos');
    expect(report.root).toBe('/mock');
    expect(typeof report.total_files).toBe('number');
    expect(tauriInvoke).toHaveBeenCalledWith('import_dry_run', { root: '/photos' });
  });

  it('importGoogleTakeout calls import_google_takeout and returns import_id', async () => {
    const resp = await importGoogleTakeout(1, '/takeout/root');
    expect(resp.import_id).toBe(2);
    expect(tauriInvoke).toHaveBeenCalledWith('import_google_takeout', {
      sourceId: 1,
      root: '/takeout/root',
    });
  });

  it('detectIcloudPath calls detect_icloud_path', async () => {
    const path = await detectIcloudPath();
    expect(path).toBeNull();
    expect(tauriInvoke).toHaveBeenCalledWith('detect_icloud_path');
  });

  it('detectHardware calls detect_hardware and returns tier', async () => {
    const info = await detectHardware();
    expect(info.tier).toBe('CpuOnly');
    expect(info.vram_mb).toBe(0);
    expect(tauriInvoke).toHaveBeenCalledWith('detect_hardware');
  });

  it('embedImage calls embed_image and returns 768-dim array', async () => {
    const vec = await embedImage('/photo.jpg');
    expect(vec).toHaveLength(768);
    expect(tauriInvoke).toHaveBeenCalledWith('embed_image', { path: '/photo.jpg' });
  });

  it('scoreAesthetic calls score_aesthetic and returns number', async () => {
    const score = await scoreAesthetic('/photo.jpg');
    expect(score).toBe(5.5);
    expect(tauriInvoke).toHaveBeenCalledWith('score_aesthetic', { path: '/photo.jpg' });
  });

  it('listIphoneDevices calls list_iphone_devices and returns array', async () => {
    expect(Array.isArray(await listIphoneDevices())).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('list_iphone_devices');
  });

  it('listImports calls list_imports with optional sourceId', async () => {
    expect(Array.isArray(await listImports())).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('list_imports', { sourceId: null });

    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await listImports(3);
    expect(tauriInvoke).toHaveBeenCalledWith('list_imports', { sourceId: 3 });
  });

  it('deleteSource calls delete_source with source + deletion options', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce({
      removed_photos: 0,
      removed_thumbnails: 0,
      errors: [],
    });
    await deleteSource(7);
    expect(tauriInvoke).toHaveBeenCalledWith('delete_source', {
      sourceId: 7,
      recycleFiles: false,
      removeOrphanPhotos: true,
    });
  });

  it('deleteSource forwards opts when provided', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce({
      removed_photos: 0,
      removed_thumbnails: 0,
      errors: [],
    });
    await deleteSource(9, { recycleFiles: true, removeOrphanPhotos: false });
    expect(tauriInvoke).toHaveBeenCalledWith('delete_source', {
      sourceId: 9,
      recycleFiles: true,
      removeOrphanPhotos: false,
    });
  });

  // ── Phase 2 §6: Cloud upload adapters ───────────────────────────────────────

  it('gphotosUploadScopeOk calls gphotos_upload_scope_ok', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce(true);
    expect(await gphotosUploadScopeOk()).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('gphotos_upload_scope_ok');
  });

  it('gphotosUpload forwards photoIds', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce({
      uploaded_count: 2,
      skipped_count: 0,
      errors: [],
    });
    const r = await gphotosUpload([1, 2]);
    expect(r.uploaded_count).toBe(2);
    expect(tauriInvoke).toHaveBeenCalledWith('gphotos_upload', { photoIds: [1, 2] });
  });

  it('onedriveAuthStatus calls onedrive_auth_status', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce(false);
    expect(await onedriveAuthStatus()).toBe(false);
    expect(tauriInvoke).toHaveBeenCalledWith('onedrive_auth_status');
  });

  it('onedriveUpload forwards photoIds + remoteFolder', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce({
      uploaded_count: 3,
      skipped_count: 1,
      errors: ['dropped frame 42'],
    });
    const r = await onedriveUpload([1, 2, 3, 42], 'Chronimage');
    expect(r.uploaded_count).toBe(3);
    expect(r.skipped_count).toBe(1);
    expect(r.errors).toEqual(['dropped frame 42']);
    expect(tauriInvoke).toHaveBeenCalledWith('onedrive_upload', {
      photoIds: [1, 2, 3, 42],
      remoteFolder: 'Chronimage',
    });
  });
});
