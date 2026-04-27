import type { QueryClient } from '@tanstack/react-query';

function revokeThumbnailBlobUrl(data: unknown) {
  if (typeof data === 'string' && data.startsWith('blob:')) {
    URL.revokeObjectURL(data);
  }
}

export function revokeRemovedThumbnailQueryBlobs(qc: QueryClient) {
  return qc.getQueryCache().subscribe((event) => {
    if (event.type !== 'removed' || event.query.queryKey[0] !== 'thumbnail') return;
    revokeThumbnailBlobUrl(event.query.state.data);
  });
}

/**
 * Photo ids can be reused after disconnecting all sources and importing a new
 * source. Any query keyed only by photo id must be removed, not merely marked
 * stale, or thumbnails/inspector metadata from the old catalog can flash under
 * the new rows until a full refresh.
 */
export function clearPhotoScopedQueries(qc: QueryClient) {
  for (const query of qc.getQueryCache().findAll({ queryKey: ['thumbnail'] })) {
    revokeThumbnailBlobUrl(query.state.data);
  }
  qc.removeQueries({ queryKey: ['thumbnail'] });
  qc.removeQueries({ queryKey: ['tags'] });
  qc.removeQueries({ queryKey: ['photo_quality'] });
  qc.removeQueries({ queryKey: ['photo_location'] });
  qc.removeQueries({ queryKey: ['photo_faces'] });
  qc.removeQueries({ queryKey: ['develop_open'] });
}

export function invalidateCatalogCollections(qc: QueryClient) {
  qc.invalidateQueries({ queryKey: ['photos'] });
  qc.invalidateQueries({ queryKey: ['albums'] });
  qc.invalidateQueries({ queryKey: ['sources'] });
  qc.invalidateQueries({ queryKey: ['imports'] });
  qc.invalidateQueries({ queryKey: ['duplicates'] });
  qc.invalidateQueries({ queryKey: ['face-clusters'] });
  qc.invalidateQueries({ queryKey: ['photos_for_cluster'] });
  qc.invalidateQueries({ queryKey: ['search_photos'] });
  qc.invalidateQueries({ queryKey: ['search_suggestions'] });
  qc.invalidateQueries({ queryKey: ['on_this_day'] });
  qc.invalidateQueries({ queryKey: ['unseen_photos'] });
  qc.invalidateQueries({ queryKey: ['first_time_on_new_camera'] });
  qc.invalidateQueries({ queryKey: ['unflagged_favorites'] });
}

export function resetCatalogContentQueries(qc: QueryClient) {
  clearPhotoScopedQueries(qc);
  invalidateCatalogCollections(qc);
}
