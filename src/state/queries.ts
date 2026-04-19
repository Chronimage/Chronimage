/**
 * TanStack Query hooks that wrap Tauri invoke calls.
 *
 * Each hook maps 1-to-1 to a Rust #[tauri::command]. The queryKey convention
 * is `[commandName, ...args]` so invalidation is predictable.
 */

import { useQuery } from '@tanstack/react-query';
import { type PhotoRow, searchPhotos } from '../tauri/invoke';

export type { PhotoRow };

/**
 * Execute a natural-language photo search via the SigLIP text encoder.
 *
 * The query is only sent to Rust when `query.trim()` is non-empty; otherwise
 * the hook stays in `idle` state and returns an empty array. This matches the
 * UI pattern where an empty search bar shows the normal grid, not a spinner.
 */
export function useSearchPhotos(query: string) {
  return useQuery<PhotoRow[], Error>({
    queryKey: ['search_photos', query],
    queryFn: () => searchPhotos(query),
    enabled: query.trim().length > 0,
    // Stale after 30 s — text embeddings are deterministic so caching is safe,
    // but we want to pick up new embeddings after an import finishes.
    staleTime: 30_000,
    // Keep previous data visible while refetching so the grid doesn't flash.
    placeholderData: (prev) => prev,
  });
}
