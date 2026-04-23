/**
 * AddSourcePopover — the reusable surface for adding a photo source.
 *
 * Used in two places:
 *   - `CatalogEmptyState` (first-run, when catalog has zero sources)
 *   - `SourcesPanel` (sidebar "+" button, after the user has at least one source)
 *
 * Presents:
 *   1. A required radio choice: Index in place | Consolidate
 *   2. Three action buttons (disabled until a mode is picked):
 *        - Add a folder (local / SD / external)
 *        - Connect iCloud (auto-detect sync folder)
 *        - Google Photos (links to Settings → Cloud sources; the OAuth flow
 *          lives in `GooglePhotosPanel` there, not duplicated here)
 *
 * iPhone USB is deliberately NOT offered here; that flow remains accessible
 * from Settings once we port it over. Per-plan scope: pragmatic first cut.
 *
 * The chosen mode is persisted globally via `useImportMode`, so it becomes
 * the default for every subsequent import (until the user switches).
 */

import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import { Icon } from '../../primitives/Icon';
import type { ImportMode } from '../../state/import';
import { useImportStore } from '../../state/import';
import {
  useCreateSource,
  useDefaultCatalogPath,
  useDetectIcloudPath,
  useStartImport,
} from '../../state/queries';
import { useCatalogHome, useImportMode } from '../../state/settings';
import { useUi } from '../../state/ui';
import { debug, errorMessage } from '../../util/log';

interface AddSourcePopoverProps {
  /** Orientation hint — `'block'` for empty-state, `'inline'` for sidebar popovers. */
  layout?: 'block' | 'inline';
}

export function AddSourcePopover({ layout = 'block' }: AddSourcePopoverProps) {
  const [mode, setMode, modeHydrated] = useImportMode();
  const [catalogHome] = useCatalogHome();
  const { data: defaultCatalogHome } = useDefaultCatalogPath();
  const { data: icloudPath } = useDetectIcloudPath();
  const createSource = useCreateSource();
  const startImport = useStartImport();
  const registerImport = useImportStore((s) => s.register);
  const setScreen = useUi((s) => s.setScreen);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const effectiveHome = catalogHome ?? defaultCatalogHome ?? null;
  const canAct = modeHydrated && mode !== null && !busy;

  async function runLocalFolder() {
    setError(null);
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== 'string') return;

    setBusy(true);
    try {
      const folderName = selected.split(/[\\/]/).pop() ?? selected;
      const source = await createSource.mutateAsync({
        name: `Local · ${folderName}`,
        kind: 'local',
        rootPath: selected,
      });
      const resp = await startImport.mutateAsync({ sourceId: source.id, root: selected });
      registerImport({
        importId: resp.import_id,
        sourceId: source.id,
        sourceName: source.name,
        mode: mode ?? 'index_in_place',
      });
    } catch (err) {
      debug('AddSourcePopover: local folder failed', err);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function runIcloud() {
    setError(null);
    let root = icloudPath ?? null;
    if (!root) {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Select your iCloud Photos folder',
      });
      if (!selected || typeof selected !== 'string') return;
      root = selected;
    }

    setBusy(true);
    try {
      const source = await createSource.mutateAsync({
        name: 'iCloud Photos',
        kind: 'icloud',
        rootPath: root,
      });
      const resp = await startImport.mutateAsync({ sourceId: source.id, root });
      registerImport({
        importId: resp.import_id,
        sourceId: source.id,
        sourceName: source.name,
        mode: mode ?? 'index_in_place',
      });
    } catch (err) {
      debug('AddSourcePopover: iCloud failed', err);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  function runGooglePhotos() {
    // Full oauth + picker flow lives in the existing GooglePhotosPanel under
    // Settings → Cloud sources. We simply navigate there.
    setScreen('settings');
  }

  async function handleModeChange(next: ImportMode) {
    await setMode(next);
  }

  const modeRadioStyle: React.CSSProperties = {
    display: 'flex',
    flexDirection: 'column',
    gap: 8,
    padding: 14,
    border: '1px solid var(--stroke)',
    borderRadius: 'var(--radius-md)',
    background: 'var(--bg-elev)',
  };

  const actionBtnStyle: React.CSSProperties = {
    display: 'flex',
    alignItems: 'center',
    gap: 10,
    padding: '14px 16px',
    border: '1px solid var(--stroke)',
    borderRadius: 'var(--radius-md)',
    background: 'var(--bg-elev)',
    fontSize: 13,
    fontWeight: 500,
    textAlign: 'left',
    cursor: canAct ? 'pointer' : 'not-allowed',
    opacity: canAct ? 1 : 0.45,
    flex: 1,
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 14,
        maxWidth: layout === 'block' ? 720 : 320,
        width: '100%',
      }}
    >
      {/* Mode radio */}
      <div style={modeRadioStyle}>
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.06em' }}>
          WHEN I IMPORT
        </div>
        <label style={{ display: 'flex', alignItems: 'flex-start', gap: 8, cursor: 'pointer' }}>
          <input
            type="radio"
            name="import-mode"
            value="index_in_place"
            checked={mode === 'index_in_place'}
            onChange={() => handleModeChange('index_in_place')}
            style={{ marginTop: 3 }}
          />
          <span style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <span style={{ fontSize: 13 }}>Index in place</span>
            <span style={{ fontSize: 11.5, color: 'var(--fg-mute)' }}>
              Just catalog — your files stay where they are.
            </span>
          </span>
        </label>
        <label style={{ display: 'flex', alignItems: 'flex-start', gap: 8, cursor: 'pointer' }}>
          <input
            type="radio"
            name="import-mode"
            value="consolidate"
            checked={mode === 'consolidate'}
            onChange={() => handleModeChange('consolidate')}
            style={{ marginTop: 3 }}
          />
          <span style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <span style={{ fontSize: 13 }}>Consolidate</span>
            <span style={{ fontSize: 11.5, color: 'var(--fg-mute)' }}>
              Copy originals into your catalog home
              {effectiveHome ? (
                <>
                  {' — '}
                  <code style={{ fontSize: 11 }}>{effectiveHome}</code>
                </>
              ) : null}
              .
            </span>
          </span>
        </label>
      </div>

      {/* Action buttons */}
      <div
        style={{
          display: 'flex',
          flexDirection: layout === 'block' ? 'row' : 'column',
          gap: 10,
          flexWrap: 'wrap',
        }}
      >
        <button type="button" style={actionBtnStyle} disabled={!canAct} onClick={runLocalFolder}>
          <Icon name="disk" size={16} />
          Add a folder
        </button>
        <button type="button" style={actionBtnStyle} disabled={!canAct} onClick={runIcloud}>
          <Icon name="cloud" size={16} />
          Connect iCloud
        </button>
        <button type="button" style={actionBtnStyle} disabled={!canAct} onClick={runGooglePhotos}>
          <Icon name="layers" size={16} />
          Google Photos
        </button>
      </div>

      {!modeHydrated && (
        <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
          Loading settings…
        </div>
      )}

      {modeHydrated && mode === null && (
        <div
          className="mono"
          style={{ fontSize: 11, color: 'var(--fg-mute)' }}
          role="note"
          aria-live="polite"
        >
          ↑ Pick a mode first.
        </div>
      )}

      {error && (
        <div style={{ fontSize: 12, color: 'var(--danger)' }} role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
