/**
 * Typed wrappers over Tauri's `invoke`. Every Rust `#[tauri::command]`
 * surfaces as an async function here so TS call-sites stay type-safe.
 *
 * In tests, mock this module (or mock `@tauri-apps/api/core`) rather than
 * stubbing individual call sites.
 */

import { invoke as tauriInvoke } from '@tauri-apps/api/core';

export type ReleaseChannel = 'dev' | 'stable' | 'beta' | 'nightly' | 'insider';

export async function ping(): Promise<string> {
  return tauriInvoke<string>('ping');
}

export async function appVersion(): Promise<string> {
  return tauriInvoke<string>('app_version');
}

export async function currentChannel(): Promise<{ channel: ReleaseChannel }> {
  return tauriInvoke<{ channel: ReleaseChannel }>('current_channel');
}

// ── Phase 1 ────────────────────────────────────────────────────────────────

export interface ExtCount {
  ext: string;
  count: number;
}

export interface ScanReport {
  root: string;
  total_files: number;
  raw_jpg_pairs: number;
  unpaired: number;
  by_extension: ExtCount[];
}

/**
 * Preview a scan of `root` without writing to the catalog. Reports file
 * counts by extension and RAW+JPG pair count.
 */
export async function importDryRun(root: string): Promise<ScanReport> {
  return tauriInvoke<ScanReport>('import_dry_run', { root });
}

// ── Dedupe ─────────────────────────────────────────────────────────────────

/** Similarity bucket for a duplicate group. */
export type DupeKind = 'Exact' | 'Near';

/**
 * A set of photos that are semantically similar to each other according to
 * SigLIP cosine similarity.
 */
export interface DuplicateGroup {
  /** IDs of the photos in the group (always ≥ 2). */
  photo_ids: number[];
  /** Highest pairwise cosine similarity found within the group (0–1). */
  max_similarity: number;
  /** Whether the group is an exact or near-duplicate cluster. */
  kind: DupeKind;
}

/**
 * Find groups of semantically similar photos using SigLIP embeddings.
 *
 * @param minSimilarity - Cosine similarity threshold (default 0.90). Pass
 *   0.95 to restrict to exact duplicates only.
 */
export async function findDuplicates(minSimilarity?: number): Promise<DuplicateGroup[]> {
  return tauriInvoke<DuplicateGroup[]>('find_duplicates', {
    min_similarity: minSimilarity ?? null,
  });
}
