/**
 * TanStack Query hooks for Chronimage.
 *
 * Keep hooks thin: they bind invoke wrappers to query keys and handle cache
 * invalidation. Domain logic stays in `src/tauri/invoke.ts` and the Rust back-end.
 */

import { type UseMutationResult, useMutation, useQueryClient } from '@tanstack/react-query';
import { type CleanupExecuteResult, type CleanupPlan, cleanupDryRun, cleanupExecute } from '../tauri/invoke';

// ── Cleanup ────────────────────────────────────────────────────────────────

/**
 * Fetch a cleanup dry-run plan. The result contains `plan_id` and
 * `confirm_token` that must be passed to `useCleanupExecute` to proceed.
 *
 * Exposed as a mutation (not a query) because every call generates a new
 * single-use token — it is not safe to cache or re-use a stale plan.
 */
export function useCleanupDryRun(): UseMutationResult<CleanupPlan, Error, void> {
  return useMutation({
    mutationFn: () => cleanupDryRun(),
  });
}

/**
 * Execute a previously issued cleanup plan.
 *
 * On success, invalidates the `sources`, `cleanup`, and `photos` query caches
 * so the UI reflects the freed storage immediately.
 */
export function useCleanupExecute(): UseMutationResult<
  CleanupExecuteResult,
  Error,
  { planId: string; confirmToken: string }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ planId, confirmToken }: { planId: string; confirmToken: string }) =>
      cleanupExecute(planId, confirmToken),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['sources'] });
      qc.invalidateQueries({ queryKey: ['cleanup'] });
      qc.invalidateQueries({ queryKey: ['photos'] });
    },
  });
}
