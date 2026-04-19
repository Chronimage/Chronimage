import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { describe, expect, it, vi } from 'vitest';
import {
  appVersion,
  createSource,
  currentChannel,
  listAlbums,
  listImports,
  listPhotos,
  listSources,
  ping,
  startImport,
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
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', { limit: null, offset: null });
  });

  it('listPhotos forwards limit and offset', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await listPhotos({ limit: 50, offset: 200 });
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', { limit: 50, offset: 200 });
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

  it('listImports calls list_imports with optional sourceId', async () => {
    expect(Array.isArray(await listImports())).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('list_imports', { sourceId: null });

    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await listImports(3);
    expect(tauriInvoke).toHaveBeenCalledWith('list_imports', { sourceId: 3 });
  });
});
