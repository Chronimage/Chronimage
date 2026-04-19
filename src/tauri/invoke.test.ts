import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { describe, expect, it, vi } from 'vitest';
import { appVersion, currentChannel, listAlbums, listPhotos, listSources, ping } from './invoke';

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
    const result = await listAlbums();
    expect(Array.isArray(result)).toBe(true);
  });

  it('listPhotos calls list_photos with default params', async () => {
    const result = await listPhotos();
    expect(Array.isArray(result)).toBe(true);
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', { limit: null, offset: null });
  });

  it('listPhotos forwards limit and offset', async () => {
    vi.mocked(tauriInvoke).mockResolvedValueOnce([]);
    await listPhotos({ limit: 50, offset: 200 });
    expect(tauriInvoke).toHaveBeenCalledWith('list_photos', { limit: 50, offset: 200 });
  });

  it('listSources calls list_sources and returns array', async () => {
    const result = await listSources();
    expect(Array.isArray(result)).toBe(true);
  });
});
