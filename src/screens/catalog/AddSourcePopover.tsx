/**
 * AddSourcePopover — the reusable surface for adding a photo source.
 *
 * Used in two places:
 *   - `CatalogEmptyState` (first-run, when catalog has zero sources)
 *   - `SourcesPanel` (sidebar "+" button, after the user has at least one source)
 *
 * Flow:
 *   1. User clicks one of three actions (Add folder / iCloud / Google Photos).
 *   2. For local + iCloud: OS folder-picker opens.
 *   3. A **copy confirmation dialog** shows the preview (count + size) of the
 *      selected folder, the target catalog root, and a "Delete originals from
 *      source after verified copy" checkbox. User cancels or confirms.
 *   4. On confirm: createSource → startImport → (auto-chain in state/import.ts
 *      runs the lift-and-shift + optional source recycle on progress finish).
 *
 * We do NOT offer an "index in place" option — Chronimage always copies to
 * the catalog so backups + portability work reliably. The checkbox gives the
 * user an explicit, auditable path to free up the source disk.
 */

import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useCallback, useState } from 'react';
import { ConfirmDialog } from '../../primitives/ConfirmDialog';
import { Icon } from '../../primitives/Icon';
import { useImportStore } from '../../state/import';
import {
  useCreateSource,
  useDefaultCatalogPath,
  useDetectIcloudPath,
  useScanPreview,
  useStartImport,
} from '../../state/queries';
import { useCatalogHome } from '../../state/settings';
import { useUi } from '../../state/ui';
import { checkSourceOverlap, type OverlappingSource } from '../../tauri/invoke';
import { debug, errorMessage } from '../../util/log';

interface AddSourcePopoverProps {
  /** Orientation hint — `'block'` for empty-state, `'inline'` for sidebar popovers. */
  layout?: 'block' | 'inline';
}

interface PendingPick {
  root: string;
  sourceKind: 'local' | 'icloud';
  defaultSourceName: string;
}

export function AddSourcePopover({ layout = 'block' }: AddSourcePopoverProps) {
  const [catalogHome] = useCatalogHome();
  const { data: defaultCatalogHome } = useDefaultCatalogPath();
  const { data: icloudPath } = useDetectIcloudPath();
  const createSource = useCreateSource();
  const startImport = useStartImport();
  const registerImport = useImportStore((s) => s.register);
  const setScreen = useUi((s) => s.setScreen);

  const [pending, setPending] = useState<PendingPick | null>(null);
  const [deleteAfterCopy, setDeleteAfterCopy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Set when the user has confirmed copy but the chosen folder contains
  // existing sources. Holds those children so the second confirmation
  // dialog can list them. Resolved on absorb-confirm or absorb-cancel.
  const [absorbPending, setAbsorbPending] = useState<OverlappingSource[] | null>(null);

  const effectiveHome = catalogHome ?? defaultCatalogHome ?? null;
  const canAct = !busy;

  const preview = useScanPreview(pending?.root ?? null);

  const closeConfirm = useCallback(() => {
    setPending(null);
    setDeleteAfterCopy(false);
    setAbsorbPending(null);
  }, []);

  // Stage 2 (separated from stage 1 so absorb confirmation can interleave):
  // actually create the source + kick off the import. Called either directly
  // when there are no overlaps, or after the user confirms absorbing inner
  // sources.
  const runCreateAndImport = useCallback(
    async (pick: PendingPick, absorb: boolean) => {
      const source = await createSource.mutateAsync({
        name: pick.defaultSourceName,
        kind: pick.sourceKind,
        rootPath: pick.root,
        absorbOverlappingChildren: absorb,
      });
      const resp = await startImport.mutateAsync({
        sourceId: source.id,
        root: pick.root,
      });
      registerImport({
        importId: resp.import_id,
        sourceId: source.id,
        sourceName: source.name,
        mode: 'consolidate',
        deleteAfterCopy,
      });
    },
    [createSource, deleteAfterCopy, registerImport, startImport],
  );

  const confirmImport = useCallback(async () => {
    if (!pending) return;
    setBusy(true);
    setError(null);
    try {
      // Probe for overlap with existing sources. Hard rejects (subfolder of
      // an existing source, or any overlap with the catalog folder) bail
      // here. Absorbable inner sources route through the second confirmation
      // dialog before we touch the catalog.
      const overlap = await checkSourceOverlap(pending.root);
      if (overlap.blocking_parent) {
        const p = overlap.blocking_parent;
        setError(
          `${pending.root} is inside existing source "${p.name}" (${p.root}). Remove that source first if you want to re-add it under a different scope.`,
        );
        return;
      }
      const managed = overlap.blocking_managed[0];
      if (managed) {
        setError(
          `${pending.root} overlaps the catalog folder ("${managed.name}" at ${managed.root}). Pick a folder outside the catalog.`,
        );
        return;
      }
      if (overlap.absorbable_children.length > 0) {
        setAbsorbPending(overlap.absorbable_children);
        return;
      }
      await runCreateAndImport(pending, false);
      closeConfirm();
    } catch (err) {
      debug('AddSourcePopover: confirm import failed', err);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [closeConfirm, pending, runCreateAndImport]);

  const confirmAbsorb = useCallback(async () => {
    if (!pending || !absorbPending) return;
    setBusy(true);
    setError(null);
    try {
      await runCreateAndImport(pending, true);
      closeConfirm();
    } catch (err) {
      debug('AddSourcePopover: absorb + import failed', err);
      setError(errorMessage(err));
      setAbsorbPending(null);
    } finally {
      setBusy(false);
    }
  }, [absorbPending, closeConfirm, pending, runCreateAndImport]);

  const cancelAbsorb = useCallback(() => {
    setAbsorbPending(null);
    setBusy(false);
  }, []);

  async function runLocalFolder() {
    setError(null);
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== 'string') return;
    const folderName = selected.split(/[\\/]/).pop() ?? selected;
    setPending({
      root: selected,
      sourceKind: 'local',
      defaultSourceName: `Local · ${folderName}`,
    });
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
    setPending({
      root,
      sourceKind: 'icloud',
      defaultSourceName: 'iCloud Photos',
    });
  }

  function runGooglePhotos() {
    setScreen('settings');
  }

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
      <div
        className="mono"
        style={{
          fontSize: 10.5,
          color: 'var(--fg-mute)',
          letterSpacing: '0.06em',
          lineHeight: 1.5,
        }}
      >
        CHRONIMAGE ALWAYS COPIES · ORIGINALS STAY INTACT UNTIL YOU CHOOSE TO DELETE
      </div>

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

      {error && (
        <div style={{ fontSize: 12, color: 'var(--danger)' }} role="alert">
          {error}
        </div>
      )}

      {pending && !absorbPending && (
        <CopyConfirmModal
          pending={pending}
          catalogRoot={effectiveHome}
          previewCount={preview.data?.total_files ?? null}
          previewRawJpgPairs={preview.data?.raw_jpg_pairs ?? null}
          deleteAfterCopy={deleteAfterCopy}
          onToggleDelete={setDeleteAfterCopy}
          onCancel={closeConfirm}
          onConfirm={confirmImport}
          busy={busy}
        />
      )}

      <ConfirmDialog
        open={absorbPending !== null}
        title="Merge existing sources?"
        description={
          absorbPending && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              <div>
                This folder contains {absorbPending.length} existing source
                {absorbPending.length === 1 ? '' : 's'}. They will be merged into the new source — photos stay
                in your catalog, but the inner source label
                {absorbPending.length === 1 ? '' : 's'} will be replaced by the new one.
              </div>
              <ul
                style={{
                  margin: 0,
                  paddingLeft: 18,
                  fontSize: 12,
                  color: 'var(--fg-mute)',
                }}
              >
                {absorbPending.map((s) => (
                  <li key={s.id}>
                    {s.name}{' '}
                    <span className="mono" style={{ fontSize: 11 }}>
                      ({s.root})
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )
        }
        confirmLabel={`Merge and import`}
        confirmTone="default"
        busy={busy}
        onCancel={cancelAbsorb}
        onConfirm={confirmAbsorb}
      />
    </div>
  );
}

interface CopyConfirmModalProps {
  pending: PendingPick;
  catalogRoot: string | null;
  previewCount: number | null;
  previewRawJpgPairs: number | null;
  deleteAfterCopy: boolean;
  onToggleDelete: (v: boolean) => void;
  onCancel: () => void;
  onConfirm: () => void;
  busy: boolean;
}

function CopyConfirmModal({
  pending,
  catalogRoot,
  previewCount,
  previewRawJpgPairs,
  deleteAfterCopy,
  onToggleDelete,
  onCancel,
  onConfirm,
  busy,
}: CopyConfirmModalProps) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Copy confirmation"
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.45)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 60,
      }}
    >
      <div
        style={{
          background: 'var(--bg)',
          border: '1px solid var(--stroke-strong)',
          borderRadius: 'var(--radius-md)',
          padding: 22,
          minWidth: 420,
          maxWidth: 560,
          display: 'flex',
          flexDirection: 'column',
          gap: 14,
        }}
      >
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.06em' }}>
          COPY INTO CATALOG
        </div>
        <div style={{ fontSize: 14, lineHeight: 1.5 }}>
          Copy photos from <code style={{ fontSize: 12, color: 'var(--fg-dim)' }}>{pending.root}</code> into
          your catalog at{' '}
          <code style={{ fontSize: 12, color: 'var(--fg-dim)' }}>
            {catalogRoot ?? '(catalog home not configured)'}
          </code>
          .
        </div>
        <div
          className="mono"
          style={{
            fontSize: 11.5,
            color: 'var(--fg-dim)',
            padding: '8px 10px',
            border: '1px solid var(--stroke)',
            borderRadius: 'var(--radius-sm)',
            background: 'var(--bg-elev)',
          }}
        >
          {previewCount === null
            ? 'Scanning folder…'
            : `${previewCount} photo${previewCount === 1 ? '' : 's'} found${
                previewRawJpgPairs
                  ? ` · ${previewRawJpgPairs} RAW+JPG pair${previewRawJpgPairs === 1 ? '' : 's'}`
                  : ''
              }`}
        </div>
        <label
          style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 10,
            padding: 10,
            border: `1px solid ${deleteAfterCopy ? 'var(--danger)' : 'var(--stroke)'}`,
            borderRadius: 'var(--radius-sm)',
            background: deleteAfterCopy
              ? 'color-mix(in oklch, var(--danger) 10%, transparent)'
              : 'var(--bg-elev)',
            cursor: 'pointer',
          }}
        >
          <input
            type="checkbox"
            checked={deleteAfterCopy}
            onChange={(e) => onToggleDelete(e.target.checked)}
            style={{ marginTop: 3 }}
            aria-label="Delete originals from source after copy"
          />
          <span style={{ display: 'flex', flexDirection: 'column', gap: 2, fontSize: 13 }}>
            <span style={{ fontWeight: 500 }}>Delete originals from the source after copy</span>
            <span style={{ fontSize: 11.5, color: 'var(--fg-mute)' }}>
              Files are recycled (not permanently deleted) only after the catalog copy's SHA256 has verified.
              Leave unchecked to keep a redundant copy on the source disk.
            </span>
          </span>
        </label>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <button
            type="button"
            className="btn"
            onClick={onCancel}
            disabled={busy}
            style={{ padding: '7px 14px', fontSize: 12.5 }}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn primary"
            onClick={onConfirm}
            disabled={busy || catalogRoot === null}
            style={{ padding: '7px 14px', fontSize: 12.5 }}
          >
            {busy ? 'Starting…' : deleteAfterCopy ? 'Copy + delete source' : 'Copy into catalog'}
          </button>
        </div>
      </div>
    </div>
  );
}
