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
 *   3. A copy-confirmation dialog (shadcn `Dialog`) shows the preview + the
 *      target catalog root and a delete-after-copy checkbox. User cancels
 *      or confirms.
 *   4. On confirm: createSource → startImport → (auto-chain in state/import.ts
 *      runs the lift-and-shift + optional source recycle on progress finish).
 */

import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useCallback, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';
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
  readonly layout?: 'block' | 'inline';
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
  const [absorbPending, setAbsorbPending] = useState<OverlappingSource[] | null>(null);

  const effectiveHome = catalogHome ?? defaultCatalogHome ?? null;
  const canAct = !busy;

  const preview = useScanPreview(pending?.root ?? null);

  const closeConfirm = useCallback(() => {
    setPending(null);
    setDeleteAfterCopy(false);
    setAbsorbPending(null);
  }, []);

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

  return (
    <div className={cn('add-source-actions', layout === 'inline' && 'add-source-actions-inline')}>
      <div className="eyebrow add-source-tip">
        Chronimage always copies · originals stay intact until you choose to delete
      </div>

      <div className={cn('add-source-action-row', layout === 'inline' && 'add-source-action-row-inline')}>
        <button type="button" className="add-source-action" disabled={!canAct} onClick={runLocalFolder}>
          <Icon name="disk" size={16} />
          <span>Add a folder</span>
        </button>
        <button type="button" className="add-source-action" disabled={!canAct} onClick={runIcloud}>
          <Icon name="cloud" size={16} />
          <span>Connect iCloud</span>
        </button>
        <button type="button" className="add-source-action" disabled={!canAct} onClick={runGooglePhotos}>
          <Icon name="layers" size={16} />
          <span>Google Photos</span>
        </button>
      </div>

      {error && (
        <div className="add-source-error" role="alert">
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
            <div className="flex flex-col gap-1.5">
              <span>
                This folder contains {absorbPending.length} existing source
                {absorbPending.length === 1 ? '' : 's'}. They will be merged into the new source — photos stay
                in your catalog, but the inner source label
                {absorbPending.length === 1 ? '' : 's'} will be replaced by the new one.
              </span>
              <ul className="add-source-absorb-list">
                {absorbPending.map((s) => (
                  <li key={s.id}>
                    <span>{s.name}</span>{' '}
                    <span className="mono text-[var(--text-xs)] text-[color:var(--fg-mute)]">({s.root})</span>
                  </li>
                ))}
              </ul>
            </div>
          )
        }
        confirmLabel="Merge and import"
        confirmTone="default"
        busy={busy}
        onCancel={cancelAbsorb}
        onConfirm={confirmAbsorb}
      />
    </div>
  );
}

interface CopyConfirmModalProps {
  readonly pending: PendingPick;
  readonly catalogRoot: string | null;
  readonly previewCount: number | null;
  readonly previewRawJpgPairs: number | null;
  readonly deleteAfterCopy: boolean;
  readonly onToggleDelete: (v: boolean) => void;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
  readonly busy: boolean;
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
    <Dialog open onOpenChange={(next) => !next && !busy && onCancel()}>
      <DialogContent className="copy-confirm-dialog">
        <DialogHeader className="copy-confirm-head">
          <div className="eyebrow">Copy into catalog</div>
          <DialogTitle asChild>
            <h2 className="h1">
              {pending.sourceKind === 'icloud' ? 'Connect iCloud' : 'Import folder'}
              <em className="display-trail">.</em>
            </h2>
          </DialogTitle>
          <DialogDescription asChild>
            <div className="body-sm copy-confirm-paths">
              <span>From</span>
              <code className="copy-confirm-path">{pending.root}</code>
              <span>To</span>
              <code className="copy-confirm-path">{catalogRoot ?? '(catalog home not configured)'}</code>
            </div>
          </DialogDescription>
        </DialogHeader>

        <div className="copy-confirm-scan">
          {previewCount === null
            ? 'Scanning folder…'
            : `${previewCount.toLocaleString()} photo${previewCount === 1 ? '' : 's'} found${
                previewRawJpgPairs
                  ? ` · ${previewRawJpgPairs.toLocaleString()} RAW + JPG pair${
                      previewRawJpgPairs === 1 ? '' : 's'
                    }`
                  : ''
              }`}
        </div>

        <Label
          htmlFor="copy-confirm-delete"
          className={cn('copy-confirm-delete-toggle', deleteAfterCopy && 'is-armed')}
        >
          <Checkbox
            id="copy-confirm-delete"
            checked={deleteAfterCopy}
            onCheckedChange={(next) => onToggleDelete(next === true)}
          />
          <div className="copy-confirm-delete-text">
            <span className="body">Delete originals from the source after copy</span>
            <span className="caption">
              Files are recycled (not permanently deleted) only after the catalog copy's SHA256 has verified.
              Leave unchecked to keep a redundant copy on the source disk.
            </span>
          </div>
        </Label>

        <DialogFooter>
          <Button variant="outline" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button
            onClick={onConfirm}
            disabled={busy || catalogRoot === null}
            className={cn(deleteAfterCopy && 'copy-confirm-armed-action')}
          >
            {busy ? 'Starting…' : deleteAfterCopy ? 'Copy + delete source' : 'Copy into catalog'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
