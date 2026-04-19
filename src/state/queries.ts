/**
 * TanStack Query hooks for Chronimage data.
 *
 * Each hook maps 1:1 to a Tauri command. The query key always starts with the
 * command name so keys are globally unique and easy to invalidate by prefix.
 */

import { useQuery } from '@tanstack/react-query';
import { type DuplicateGroup, findDuplicates } from '../tauri/invoke';

export type { DuplicateGroup };

/**
 * Fetch all duplicate groups whose cosine similarity meets `minSimilarity`.
 *
 * @param minSimilarity - Threshold in [0, 1] (default 0.90 applied server-side).
 *
 * @example
 * ```tsx
 * function DupePanel() {
 *   const { data: groups = [], isLoading } = useDuplicates();
 *   if (isLoading) return <Spinner />;
 *   return <DupeList groups={groups} />;
 * }
 * ```
 */
export function useDuplicates(minSimilarity?: number) {
  return useQuery({
    queryKey: ['duplicates', minSimilarity] as const,
    queryFn: () => findDuplicates(minSimilarity),
  });
}
