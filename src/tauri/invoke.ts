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

// ── Source-side cleanup ────────────────────────────────────────────────────

export interface CleanupItem {
  copy_id: number;
  photo_id: number;
  source_id: number;
  source_kind: string;
  path: string | null;
  verified_sha256: string;
  size_bytes: number;
}

export interface SourceCleanupItem {
  source_id: number;
  source_name: string;
  source_kind: string;
  reclaimable_bytes: number;
  file_count: number;
  items: CleanupItem[];
}

/**
 * Dry-run: compute which source copies can be safely deleted and return a
 * signed plan. Pass `plan_id` + `confirm_token` to `cleanupExecute` to
 * proceed. Plans are single-use and invalidated on process restart.
 */
export interface CleanupPlan {
  plan_id: string;
  confirm_token: string;
  total_reclaimable_bytes: number;
  total_file_count: number;
  sources: SourceCleanupItem[];
}

export async function cleanupDryRun(): Promise<CleanupPlan> {
  return tauriInvoke<CleanupPlan>('cleanup_dry_run');
}

export interface CleanupExecuteResult {
  deleted_count: number;
  freed_bytes: number;
  errors: string[];
}

/**
 * Execute a previously issued cleanup plan. Requires the `confirm_token`
 * returned by `cleanupDryRun`. Safety gates: token check → local-only source
 * check → ≥2× free-space check → per-file SHA256 re-verify → delete.
 */
export async function cleanupExecute(planId: string, confirmToken: string): Promise<CleanupExecuteResult> {
  return tauriInvoke<CleanupExecuteResult>('cleanup_execute', {
    planId,
    confirmToken,
  });
}
