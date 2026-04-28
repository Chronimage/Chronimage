/**
 * DevelopScreen — Phase 3 non-destructive RAW/JPEG editor. Slider drags
 * post through `develop_apply` to the Rust CPU pipeline (rayon; wgpu is a
 * follow-up); preview comes back as a base64 JPEG data URL. Save/Reset/
 * CopyEdits hit the `edits` table via the develop commands.
 */

import { type CSSProperties, type PointerEvent, useCallback, useEffect, useRef, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Placeholder } from '../../primitives/Placeholder';
import { Seg } from '../../primitives/Seg';
import { Thumbnail, thumbnailSizeForCssBox } from '../../primitives/Thumbnail';
import { useDevelopUi } from '../../state/develop';
import {
  type DevelopOperations,
  useAiEditRefresh,
  useAiEditStatus,
  useDevelopApply,
  useDevelopMaskApplyPreview,
  useDevelopMasks,
  useDevelopMaskUpdate,
  useDevelopOpen,
  useDevelopPasteEdits,
  useDevelopReset,
  useDevelopSave,
  useDevelopSnapshotSave,
  usePhotos,
} from '../../state/queries';
import {
  type DevelopMask,
  identityOperations,
  maskFromPrompt,
  type PhotoRow,
  promptEdit,
  promptEditAccept,
  promptEditReject,
  promptSidecarPing,
  type SidecarStatus,
} from '../../tauri/invoke';
import { warn } from '../../util/log';
import { EditorInspector } from './EditorInspector';
import {
  type DevelopTab,
  type DevelopValues,
  defaultDevelopValues,
  operationsToValues,
  parseOperationsJson,
  valuesToOperations,
} from './types';

interface UpdateValuesOptions {
  render?: boolean;
}

function uncroppedPreviewOperations(ops: DevelopOperations): DevelopOperations {
  return {
    ...ops,
    crop_x: 0,
    crop_y: 0,
    crop_w: 1,
    crop_h: 1,
  };
}

function sanitizeMaskOperations(ops: DevelopOperations): DevelopOperations {
  return {
    ...ops,
    crop_x: 0,
    crop_y: 0,
    crop_w: 1,
    crop_h: 1,
    rotation: 0,
    straighten: 0,
    transform_h: 0,
    transform_v: 0,
  };
}

export function DevelopScreen() {
  const { data: photos = [], isLoading } = usePhotos();
  const [focusedIdx, setFocusedIdx] = useState(0);
  const [tab, setTab] = useState<DevelopTab>('develop');
  const [cropMode, setCropModeState] = useState(false);
  const [values, setValues] = useState<DevelopValues>(() => defaultDevelopValues());
  const [preview, setPreview] = useState<string | null>(null);
  const [copiedOps, setCopiedOps] = useState<DevelopOperations | null>(null);
  const [promptText, setPromptText] = useState(
    'Lift shadows slightly, keep skin tones natural, subtle dehaze on sky.',
  );
  const [promptStrength, setPromptStrength] = useState(65);
  const [promptConstraints, setPromptConstraints] = useState<string[]>(['keep faces sharp', 'natural tones']);
  const valuesRef = useRef(values);
  const cropModeRef = useRef(false);
  const focusedPhotoIdRef = useRef<number | null>(null);
  const applySeqRef = useRef(0);
  const userEditedRef = useRef(false);
  const debounceTimer = useRef<number | null>(null);
  const maskUpdateTimer = useRef<number | null>(null);
  const [localMaskOperations, setLocalMaskOperations] = useState<Record<number, DevelopOperations>>({});

  const photo = photos[focusedIdx] ?? null;
  const focusedPhotoId = photo?.id ?? null;

  // Publish focus + preview through the tiny Zustand store so the
  // DevelopSidePanel (a sibling in the app shell) can drive
  // `develop_preset_apply` against the right photo and push the
  // returned preview back into the stage.
  const setSharedFocus = useDevelopUi((s) => s.setFocusedPhotoId);
  const setSharedPreview = useDevelopUi((s) => s.setPreview);
  const setSharedOperations = useDevelopUi((s) => s.setOperations);
  const setSharedMask = useDevelopUi((s) => s.setActiveMask);
  const selectedMaskId = useDevelopUi((s) => s.selectedMaskId);
  const setSelectedMaskId = useDevelopUi((s) => s.setSelectedMaskId);
  const maskOverlayVisible = useDevelopUi((s) => s.maskOverlayVisible);
  const maskOverlayOpacity = useDevelopUi((s) => s.maskOverlayOpacity);
  const sharedPreview = useDevelopUi((s) => s.preview);
  const sharedOperations = useDevelopUi((s) => s.operations);
  const operationSource = useDevelopUi((s) => s.operationSource);
  useEffect(() => {
    focusedPhotoIdRef.current = focusedPhotoId;
    cropModeRef.current = false;
    setCropModeState(false);
    applySeqRef.current += 1;
    userEditedRef.current = false;
    if (debounceTimer.current !== null) {
      window.clearTimeout(debounceTimer.current);
      debounceTimer.current = null;
    }
    setSharedFocus(focusedPhotoId);
    setSharedPreview(null);
    setSharedOperations(null, 'screen');
    setSharedMask(null);
    setSelectedMaskId(null);
    setPreview(null);
    setLocalMaskOperations({});
  }, [
    focusedPhotoId,
    setSelectedMaskId,
    setSharedFocus,
    setSharedMask,
    setSharedOperations,
    setSharedPreview,
  ]);

  // Load the photo's current edit state + baseline preview ONCE per photo.
  // Re-fetching `opened` on every render would wipe unsaved slider
  // positions because the backend's `current_edit_id` only moves on
  // explicit Save. Keying off `focusedPhotoId` ensures we only seed
  // slider state when the user opens a different photo.
  const { data: opened } = useDevelopOpen(focusedPhotoId);
  const { data: masks = [] } = useDevelopMasks(focusedPhotoId);
  const updateMask = useDevelopMaskUpdate();
  const applyMaskPreview = useDevelopMaskApplyPreview();
  const seededForPhotoRef = useRef<number | null>(null);
  useEffect(() => {
    if (!opened) return;
    if (opened.photo_id !== focusedPhotoIdRef.current) return;
    if (seededForPhotoRef.current === opened.photo_id) return;
    seededForPhotoRef.current = opened.photo_id;
    const sharedState = useDevelopUi.getState();
    if (sharedState.operationSource === 'sidepanel' && sharedState.operations) {
      userEditedRef.current = true;
      return;
    }
    if (userEditedRef.current) return;
    const nextValues = operationsToValues(opened.operations);
    valuesRef.current = nextValues;
    setValues(nextValues);
    setPreview(opened.preview_data_url);
    setSharedPreview(opened.preview_data_url);
    setSharedOperations(opened.operations, 'screen');
  }, [opened, setSharedOperations, setSharedPreview]);

  // If the sidepanel pushed a new preview (preset apply), surface it.
  useEffect(() => {
    if (sharedPreview && sharedPreview !== preview) setPreview(sharedPreview);
  }, [sharedPreview, preview]);

  useEffect(() => {
    if (operationSource !== 'sidepanel' || !sharedOperations) return;
    const nextValues = operationsToValues(sharedOperations);
    userEditedRef.current = true;
    valuesRef.current = nextValues;
    setValues(nextValues);
  }, [operationSource, sharedOperations]);

  useEffect(() => {
    if (selectedMaskId != null && masks.length > 0 && !masks.some((mask) => mask.id === selectedMaskId)) {
      setSelectedMaskId(null);
    }
  }, [masks, selectedMaskId, setSelectedMaskId]);

  const applyMut = useDevelopApply();
  const saveMut = useDevelopSave();
  const snapshotMut = useDevelopSnapshotSave();
  const resetMut = useDevelopReset();
  const pasteMut = useDevelopPasteEdits();

  // Debounce slider input → backend render. 80 ms feels responsive; the
  // rayon pipeline at 1280 long-edge runs ~30–60 ms on i5.
  useEffect(() => {
    return () => {
      if (debounceTimer.current !== null) window.clearTimeout(debounceTimer.current);
      if (maskUpdateTimer.current !== null) window.clearTimeout(maskUpdateTimer.current);
    };
  }, []);

  const scheduleApply = useCallback(
    (ops: DevelopOperations) => {
      if (focusedPhotoId == null) return;
      const photoId = focusedPhotoId;
      const seq = ++applySeqRef.current;
      if (debounceTimer.current !== null) {
        window.clearTimeout(debounceTimer.current);
      }
      debounceTimer.current = window.setTimeout(() => {
        applyMut.mutate(
          { photoId, operations: ops },
          {
            onSuccess: (r) => {
              if (seq !== applySeqRef.current) return;
              if (focusedPhotoIdRef.current !== r.photo_id) return;
              setPreview(r.preview_data_url);
              setSharedPreview(r.preview_data_url);
            },
            onError: (e) => warn('develop_apply failed', e),
          },
        );
      }, 80);
    },
    [applyMut, focusedPhotoId, setSharedPreview],
  );

  const scheduleEditorPreview = useCallback(
    (ops: DevelopOperations) => {
      scheduleApply(cropModeRef.current ? uncroppedPreviewOperations(ops) : ops);
    },
    [scheduleApply],
  );

  const setCropMode = useCallback(
    (next: boolean) => {
      cropModeRef.current = next;
      setCropModeState(next);
      const ops = valuesToOperations(valuesRef.current);
      scheduleApply(next ? uncroppedPreviewOperations(ops) : ops);
    },
    [scheduleApply],
  );

  const setDevelopTab = useCallback(
    (next: DevelopTab) => {
      setTab(next);
      if (next !== 'develop' && cropModeRef.current) {
        setCropMode(false);
      }
    },
    [setCropMode],
  );

  // Keep a ref on the latest values so `updateValue` can compute the
  // next state without closing over a stale `values` snapshot — and so
  // the side-effect (scheduleApply) happens outside `setValues`'s
  // updater function (an anti-pattern that fires twice under React
  // Strict Mode, clearing our debounce timer on the second run).
  // The ref is updated *synchronously* inside updateValue so two slider
  // drags dispatched in the same event tick don't both compute `next`
  // from the same stale snapshot.
  // Sync the ref whenever React commits a new `values` from elsewhere
  // (preset apply / reset / paste / opened seed). Slider drags update
  // the ref synchronously below so they don't depend on this effect.
  useEffect(() => {
    valuesRef.current = values;
  }, [values]);

  const updateValue = useCallback(
    (key: keyof DevelopValues, value: number) => {
      userEditedRef.current = true;
      const next = { ...valuesRef.current, [key]: value };
      valuesRef.current = next;
      setValues(next);
      const ops = valuesToOperations(next);
      setSharedOperations(ops, 'screen');
      scheduleEditorPreview(ops);
    },
    [scheduleEditorPreview, setSharedOperations],
  );

  const updateValues = useCallback(
    (patch: Partial<DevelopValues>, options: UpdateValuesOptions = {}) => {
      userEditedRef.current = true;
      const next = { ...valuesRef.current, ...patch };
      valuesRef.current = next;
      setValues(next);
      const ops = valuesToOperations(next);
      setSharedOperations(ops, 'screen');
      if (options.render !== false) {
        scheduleEditorPreview(ops);
      }
    },
    [scheduleEditorPreview, setSharedOperations],
  );

  const updateCurves = useCallback(
    (curves: DevelopValues['curves']) => {
      userEditedRef.current = true;
      const next: DevelopValues = { ...valuesRef.current, curves };
      valuesRef.current = next;
      setValues(next);
      const ops = valuesToOperations(next);
      setSharedOperations(ops, 'screen');
      scheduleEditorPreview(ops);
    },
    [scheduleEditorPreview, setSharedOperations],
  );

  const autoLight = useCallback(() => {
    userEditedRef.current = true;
    const next: DevelopValues = {
      ...valuesRef.current,
      exp: 12,
      con: 8,
      hi: -24,
      sh: 32,
      whites: 0,
      blacks: 0,
      temp: 6,
      tint: -2,
      vib: 14,
      sat: 4,
      clarity: 8,
      dehaze: 10,
    };
    valuesRef.current = next;
    setValues(next);
    const ops = valuesToOperations(next);
    setSharedOperations(ops, 'screen');
    scheduleEditorPreview(ops);
  }, [scheduleEditorPreview, setSharedOperations]);

  const resetEdits = useCallback(() => {
    if (focusedPhotoId == null) return;
    resetMut.mutate(focusedPhotoId, {
      onSuccess: () => {
        const next = defaultDevelopValues();
        const ops = valuesToOperations(next);
        userEditedRef.current = true;
        valuesRef.current = next;
        setValues(next);
        setSharedOperations(ops, 'screen');
        scheduleEditorPreview(ops);
      },
    });
  }, [focusedPhotoId, resetMut, scheduleEditorPreview, setSharedOperations]);

  const currentOperations = useCallback(() => {
    return useDevelopUi.getState().operations ?? valuesToOperations(valuesRef.current);
  }, []);

  const refreshMaskPreview = useCallback(() => {
    if (focusedPhotoId == null) return;
    applyMaskPreview.mutate(
      { photoId: focusedPhotoId, operations: currentOperations() },
      {
        onSuccess: (receipt) => {
          setPreview(receipt.preview_data_url);
          setSharedPreview(receipt.preview_data_url);
        },
        onError: (e) => warn('develop_mask_apply_preview failed', e),
      },
    );
  }, [applyMaskPreview, currentOperations, focusedPhotoId, setSharedPreview]);

  const maskOperations = useCallback(
    (mask: DevelopMask) =>
      sanitizeMaskOperations(
        localMaskOperations[mask.id] ?? parseOperationsJson(mask.operations_json) ?? identityOperations(),
      ),
    [localMaskOperations],
  );

  const selectedMask =
    selectedMaskId == null ? null : (masks.find((mask) => mask.id === selectedMaskId) ?? null);
  const selectedMaskOps = selectedMask ? maskOperations(selectedMask) : null;
  const selectedMaskValues = selectedMaskOps ? operationsToValues(selectedMaskOps) : null;

  const queueMaskOperationUpdate = useCallback(
    (maskId: number, nextOps: DevelopOperations) => {
      if (maskUpdateTimer.current !== null) window.clearTimeout(maskUpdateTimer.current);
      maskUpdateTimer.current = window.setTimeout(() => {
        updateMask.mutate(
          { mask_id: maskId, operations: nextOps },
          {
            onSuccess: (mask) => {
              setLocalMaskOperations((current) => ({
                ...current,
                [mask.id]: parseOperationsJson(mask.operations_json) ?? nextOps,
              }));
              refreshMaskPreview();
            },
            onError: (e) => warn('develop_mask_update failed', e),
          },
        );
      }, 120);
    },
    [refreshMaskPreview, updateMask],
  );

  const updateSelectedMaskValues = useCallback(
    (patch: Partial<DevelopValues>) => {
      if (!selectedMask || !selectedMaskValues) return;
      const nextValues = { ...selectedMaskValues, ...patch };
      const nextOps = sanitizeMaskOperations(valuesToOperations(nextValues));
      setLocalMaskOperations((local) => ({ ...local, [selectedMask.id]: nextOps }));
      queueMaskOperationUpdate(selectedMask.id, nextOps);
    },
    [queueMaskOperationUpdate, selectedMask, selectedMaskValues],
  );

  const updateSelectedMaskValue = useCallback(
    (key: keyof DevelopValues, value: number) => {
      updateSelectedMaskValues({ [key]: value } as Partial<DevelopValues>);
    },
    [updateSelectedMaskValues],
  );

  const updateSelectedMaskCurves = useCallback(
    (curves: DevelopValues['curves']) => {
      updateSelectedMaskValues({ curves });
    },
    [updateSelectedMaskValues],
  );

  const resetSelectedMaskOperations = useCallback(() => {
    if (!selectedMask) return;
    const nextOps = sanitizeMaskOperations(identityOperations());
    setLocalMaskOperations((local) => ({ ...local, [selectedMask.id]: nextOps }));
    queueMaskOperationUpdate(selectedMask.id, nextOps);
  }, [queueMaskOperationUpdate, selectedMask]);

  const saveEdits = useCallback(() => {
    if (focusedPhotoId == null) return;
    saveMut.mutate({ photoId: focusedPhotoId, operations: currentOperations() });
  }, [currentOperations, focusedPhotoId, saveMut]);

  const saveSnapshot = useCallback(() => {
    if (focusedPhotoId == null) return;
    snapshotMut.mutate({
      photoId: focusedPhotoId,
      operations: currentOperations(),
      label: `Snapshot ${new Date().toLocaleTimeString()}`,
    });
  }, [currentOperations, focusedPhotoId, snapshotMut]);

  const copyEdits = useCallback(() => {
    setCopiedOps(currentOperations());
  }, [currentOperations]);

  const pasteEdits = useCallback(() => {
    if (focusedPhotoId == null || !copiedOps) return;
    pasteMut.mutate(
      { photoIds: [focusedPhotoId], operations: copiedOps },
      {
        onSuccess: (receipt) => {
          if (receipt.skipped.includes(focusedPhotoId)) return;
          const next = operationsToValues(copiedOps);
          userEditedRef.current = true;
          valuesRef.current = next;
          setValues(next);
          setSharedOperations(copiedOps, 'screen');
          scheduleEditorPreview(copiedOps);
        },
        onError: (e) => warn('develop_paste_edits failed', e),
      },
    );
  }, [copiedOps, focusedPhotoId, pasteMut, scheduleEditorPreview, setSharedOperations]);

  if (isLoading) {
    return (
      <div className="canvas">
        <div style={{ padding: 40, color: 'var(--fg-mute)', fontSize: 13 }}>Loading editor…</div>
      </div>
    );
  }

  if (!photo) {
    return (
      <div className="canvas">
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 14,
            padding: 48,
            textAlign: 'center',
          }}
        >
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.1em' }}>
            DEVELOP · PHASE 3
          </div>
          <h1 className="page-title">
            Pick a photo to develop
            <em>.</em>
          </h1>
          <p style={{ maxWidth: 540, color: 'var(--fg-dim)', fontSize: 13, lineHeight: 1.5 }}>
            Non-destructive RAW develop with exposure, curves, masks and the AI preset library. Sony A7 IV ARW
            is the priority RAW format. Backend wgpu engine lands in Phase 3 §1–§3.
          </p>
        </div>
      </div>
    );
  }

  const megapixels =
    photo.width && photo.height ? ((photo.width * photo.height) / 1_000_000).toFixed(0) : '—';
  const swap = photo.orientation >= 5 && photo.orientation <= 8;
  const w = swap ? photo.height : photo.width;
  const h = swap ? photo.width : photo.height;
  const stageAspect = w > 0 && h > 0 ? `${w} / ${h}` : '3 / 2';
  const cropW = clampNumber(values.cropW / 100, 0.05, 1);
  const cropH = clampNumber(values.cropH / 100, 0.05, 1);
  const displayStageAspect = !cropMode && w > 0 && h > 0 ? `${w * cropW} / ${h * cropH}` : stageAspect;
  const selectedMaskPreview =
    selectedMask && maskOverlayVisible
      ? localMaskPreview(selectedMask, maskOverlayOpacity / 100, cropMode ? null : values)
      : null;

  return (
    <div className="canvas">
      <DevelopToolbar
        photo={photo}
        megapixels={megapixels}
        tab={tab}
        setTab={setDevelopTab}
        cropMode={cropMode}
        onToggleCrop={() => {
          if (tab !== 'develop') {
            setTab('develop');
            setCropMode(true);
            return;
          }
          setCropMode(!cropModeRef.current);
        }}
        canPrev={focusedIdx > 0}
        canNext={focusedIdx < photos.length - 1}
        onPrev={() => setFocusedIdx((i) => Math.max(0, i - 1))}
        onNext={() => setFocusedIdx((i) => Math.min(photos.length - 1, i + 1))}
        onSave={saveEdits}
        onSnapshot={saveSnapshot}
        onCopy={copyEdits}
        saving={saveMut.isPending || snapshotMut.isPending}
      />

      {tab === 'prompt' ? (
        <PromptStage
          photo={photo}
          promptText={promptText}
          setPromptText={setPromptText}
          promptStrength={promptStrength}
          setPromptStrength={setPromptStrength}
          constraints={promptConstraints}
          setConstraints={setPromptConstraints}
        />
      ) : (
        <DevelopStageSplit
          photo={photo}
          photos={photos}
          focusedIdx={focusedIdx}
          setFocusedIdx={setFocusedIdx}
          stageAspect={displayStageAspect}
          values={selectedMaskValues ?? values}
          inspectorMode={selectedMask && selectedMaskValues ? 'mask' : 'global'}
          selectedMask={selectedMask}
          selectedMaskPreview={selectedMaskPreview}
          onValueChange={selectedMask && selectedMaskValues ? updateSelectedMaskValue : updateValue}
          onValuesChange={selectedMask && selectedMaskValues ? updateSelectedMaskValues : updateValues}
          onCurvesChange={selectedMask && selectedMaskValues ? updateSelectedMaskCurves : updateCurves}
          onAutoLight={autoLight}
          onReset={selectedMask && selectedMaskValues ? resetSelectedMaskOperations : resetEdits}
          onCopy={copyEdits}
          onPaste={pasteEdits}
          canPaste={!!copiedOps && !pasteMut.isPending}
          preview={preview}
          cropMode={cropMode}
          globalValues={values}
          onGlobalValuesChange={updateValues}
          setCropMode={setCropMode}
          onClearMaskSelection={() => setSelectedMaskId(null)}
        />
      )}
    </div>
  );
}

interface DevelopToolbarProps {
  photo: PhotoRow;
  megapixels: string;
  tab: DevelopTab;
  setTab: (tab: DevelopTab) => void;
  cropMode: boolean;
  onToggleCrop: () => void;
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
  onSave: () => void;
  onSnapshot: () => void;
  onCopy: () => void;
  saving: boolean;
}

function DevelopToolbar({
  photo,
  megapixels,
  tab,
  setTab,
  cropMode,
  onToggleCrop,
  canPrev,
  canNext,
  onPrev,
  onNext,
  onSave,
  onSnapshot,
  onCopy,
  saving,
}: DevelopToolbarProps) {
  return (
    <div className="toolbar">
      <button
        type="button"
        className="btn"
        style={{ padding: '5px 8px' }}
        title="Previous photo"
        onClick={onPrev}
        disabled={!canPrev}
        aria-disabled={!canPrev}
      >
        <Icon name="chevL" size={13} />
      </button>
      <button
        type="button"
        className="btn"
        style={{ padding: '5px 8px' }}
        title="Next photo"
        onClick={onNext}
        disabled={!canNext}
        aria-disabled={!canNext}
      >
        <Icon name="chevR" size={13} />
      </button>
      <div className="divider" />
      <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
        {photo.filename} · {photo.is_raw ? (photo.raw_format || 'RAW').toUpperCase() : 'JPG'} · {photo.width}×
        {photo.height} · {megapixels}MP
      </span>
      <div style={{ flex: 1 }} />
      <Seg<DevelopTab>
        value={tab}
        onChange={setTab}
        options={[
          { value: 'develop', label: 'Develop' },
          { value: 'prompt', label: 'Prompt' },
        ]}
      />
      <div className="divider" />
      <button
        type="button"
        className={cropMode ? 'btn on' : 'btn'}
        title={cropMode ? 'Finish crop' : 'Crop directly on the canvas'}
        aria-pressed={cropMode}
        onClick={onToggleCrop}
      >
        <Icon name="crop" size={13} />
      </button>
      <button type="button" className="btn" title="Save a named before/after snapshot" onClick={onSnapshot}>
        <Icon name="eye" size={13} /> Snapshot
      </button>
      <div className="divider" />
      <button type="button" className="btn" onClick={onCopy} title="Copy current edits to the clipboard">
        <Icon name="layers" size={13} /> Copy edits
      </button>
      <button
        type="button"
        className="btn primary"
        onClick={onSave}
        disabled={saving}
        title="Save current edits as a new history entry"
      >
        <Icon name="download" size={13} /> {saving ? 'Saving…' : 'Save'}
      </button>
    </div>
  );
}

interface DevelopStageSplitProps {
  photo: PhotoRow;
  photos: PhotoRow[];
  focusedIdx: number;
  setFocusedIdx: (n: number) => void;
  stageAspect: string;
  values: DevelopValues;
  globalValues: DevelopValues;
  inspectorMode: 'global' | 'mask';
  selectedMask: DevelopMask | null;
  selectedMaskPreview: { className: string; style: CSSProperties } | null;
  onValueChange: (key: keyof DevelopValues, value: number) => void;
  onValuesChange: (patch: Partial<DevelopValues>, options?: UpdateValuesOptions) => void;
  onGlobalValuesChange: (patch: Partial<DevelopValues>, options?: UpdateValuesOptions) => void;
  onCurvesChange: (curves: DevelopValues['curves']) => void;
  onAutoLight: () => void;
  onReset: () => void;
  onCopy: () => void;
  onPaste: () => void;
  canPaste: boolean;
  preview: string | null;
  cropMode: boolean;
  setCropMode: (active: boolean) => void;
  onClearMaskSelection: () => void;
}

function DevelopStageSplit({
  photo,
  photos,
  focusedIdx,
  setFocusedIdx,
  stageAspect,
  values,
  globalValues,
  inspectorMode,
  selectedMask,
  selectedMaskPreview,
  onValueChange,
  onValuesChange,
  onGlobalValuesChange,
  onCurvesChange,
  onAutoLight,
  onReset,
  onCopy,
  onPaste,
  canPaste,
  preview,
  cropMode,
  setCropMode,
  onClearMaskSelection,
}: DevelopStageSplitProps) {
  const [zoom, setZoom] = useState(100);
  const [panX, setPanX] = useState(0);
  const [panY, setPanY] = useState(0);
  const [zoomMenuOpen, setZoomMenuOpen] = useState(false);
  const frameRef = useRef<HTMLDivElement | null>(null);
  // The inner photo-aspect wrapper. Frame is full-bleed (fills the
  // editor canvas, light-dark background); the wrapper inside it sizes
  // to the photo's aspect ratio so the crop overlay's percentage coords
  // stay aligned to the actual image bounds.
  const zoomWrapperRef = useRef<HTMLDivElement | null>(null);
  const panDragRef = useRef<{
    startX: number;
    startY: number;
    basePanX: number;
    basePanY: number;
    moved: boolean;
  } | null>(null);
  // A pointerup that comes within this many pixels of the pointerdown is
  // treated as a click (toggle zoom anchored at the click) rather than a
  // pan-drag. Matches the threshold most desktop UIs use for click-vs-drag.
  const CLICK_THRESHOLD_PX = 4;
  const cropDragRef = useRef<{
    kind: 'move' | 'nw' | 'ne' | 'sw' | 'se';
    startX: number;
    startY: number;
    start: Pick<DevelopValues, 'cropX' | 'cropY' | 'cropW' | 'cropH'>;
  } | null>(null);

  // Multiplicative zoom step matches Lightroom's "wheel notch" feel — at
  // 1.15 each notch zooms in by ~15%, four notches roughly doubles.
  const ZOOM_STEP = 1.15;
  const ZOOM_MIN = 8;
  const ZOOM_MAX = 800;
  const zoomIn = () => setZoom((z) => clampNumber(z * ZOOM_STEP, ZOOM_MIN, ZOOM_MAX));
  const zoomOut = () => setZoom((z) => clampNumber(z / ZOOM_STEP, ZOOM_MIN, ZOOM_MAX));
  const resetView = () => {
    setZoom(100);
    setPanX(0);
    setPanY(0);
  };
  // 1:1 means one preview-source pixel per screen pixel. The preview is
  // delivered at 2048 px long-edge (DevelopDecodeCache::PREVIEW_LONG_EDGE).
  // At zoom=100 the wrapper renders at its photo-aspect-fit size inside
  // the frame, so we measure the wrapper's long edge — not the frame's —
  // to compute the zoom factor that maps preview pixels 1:1 to screen px.
  const goOneToOne = () => {
    const wrapper = zoomWrapperRef.current;
    if (!wrapper) {
      setZoom(200);
      return;
    }
    const rect = wrapper.getBoundingClientRect();
    const longEdge = Math.max(rect.width, rect.height);
    if (longEdge <= 0) {
      setZoom(200);
      return;
    }
    const next = clampNumber((2048 / longEdge) * 100, ZOOM_MIN, ZOOM_MAX);
    setZoom(next);
    setPanX(0);
    setPanY(0);
  };
  const resetCrop = () =>
    onGlobalValuesChange({ cropX: 0, cropY: 0, cropW: 100, cropH: 100 }, { render: !cropMode });
  const applyAspect = (patch: Partial<DevelopValues>) => onGlobalValuesChange(patch, { render: !cropMode });
  const cropLeft = clampNumber(globalValues.cropX, 0, 95);
  const cropTop = clampNumber(globalValues.cropY, 0, 95);
  const cropWidth = clampNumber(globalValues.cropW, 5, 100 - cropLeft);
  const cropHeight = clampNumber(globalValues.cropH, 5, 100 - cropTop);
  const isCropping = cropLeft > 0 || cropTop > 0 || cropWidth < 100 || cropHeight < 100;
  const showCropOverlay = cropMode;

  const startCropDrag = (
    kind: 'move' | 'nw' | 'ne' | 'sw' | 'se',
    event: PointerEvent<HTMLButtonElement | HTMLDivElement>,
  ) => {
    if (!cropMode) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const nextDrag = {
      kind,
      startX: event.clientX,
      startY: event.clientY,
      start: {
        cropX: cropLeft,
        cropY: cropTop,
        cropW: cropWidth,
        cropH: cropHeight,
      },
    };
    cropDragRef.current = nextDrag;
  };

  const updateCropDrag = (event: PointerEvent<HTMLDivElement>) => {
    const activeDrag = cropDragRef.current;
    // Crop coords are % of the image. The wrapper has the photo aspect
    // and is centered inside the (now full-bleed) frame, so divide by
    // its rect — not the frame's — to get the right percentages.
    const wrapper = zoomWrapperRef.current ?? frameRef.current;
    if (!activeDrag || !wrapper) return;
    const rect = wrapper.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    const dx = ((event.clientX - activeDrag.startX) / rect.width) * 100;
    const dy = ((event.clientY - activeDrag.startY) / rect.height) * 100;
    const start = activeDrag.start;
    let nextX = start.cropX;
    let nextY = start.cropY;
    let nextW = start.cropW;
    let nextH = start.cropH;

    if (activeDrag.kind === 'move') {
      nextX = clampNumber(start.cropX + dx, 0, 100 - start.cropW);
      nextY = clampNumber(start.cropY + dy, 0, 100 - start.cropH);
    } else {
      const east = activeDrag.kind === 'ne' || activeDrag.kind === 'se';
      const south = activeDrag.kind === 'sw' || activeDrag.kind === 'se';
      if (east) {
        nextW = clampNumber(start.cropW + dx, 5, 100 - start.cropX);
      } else {
        const maxX = start.cropX + start.cropW - 5;
        nextX = clampNumber(start.cropX + dx, 0, maxX);
        nextW = start.cropW + (start.cropX - nextX);
      }
      if (south) {
        nextH = clampNumber(start.cropH + dy, 5, 100 - start.cropY);
      } else {
        const maxY = start.cropY + start.cropH - 5;
        nextY = clampNumber(start.cropY + dy, 0, maxY);
        nextH = start.cropH + (start.cropY - nextY);
      }
    }

    onGlobalValuesChange(
      {
        cropX: Math.round(nextX * 10) / 10,
        cropY: Math.round(nextY * 10) / 10,
        cropW: Math.round(nextW * 10) / 10,
        cropH: Math.round(nextH * 10) / 10,
      },
      { render: false },
    );
  };

  const stopCropDrag = () => {
    cropDragRef.current = null;
  };

  // Reset pan + zoom when switching photos so the user always starts at
  // Fit. Without this you can land on a new photo mid-pan and see only a
  // corner of it.
  useEffect(() => {
    setZoom(100);
    setPanX(0);
    setPanY(0);
  }, [photo.id]);

  // Mouse-wheel cursor-anchored zoom. React 19's onWheel is a passive
  // listener and can't preventDefault, which means the page would scroll
  // every time you spin the wheel over the canvas. Attach imperatively
  // with passive:false so wheel events stay on the canvas.
  useEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const handler = (event: WheelEvent) => {
      if (cropMode) return;
      event.preventDefault();
      const rect = frame.getBoundingClientRect();
      const cx = event.clientX - rect.left - rect.width / 2;
      const cy = event.clientY - rect.top - rect.height / 2;
      const factor = event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      setZoom((current) => {
        const next = clampNumber(current * factor, ZOOM_MIN, ZOOM_MAX);
        const ratio = next / current;
        // Cursor-anchored math: keep the image-space coord under the
        // pointer fixed across the zoom step.
        setPanX((px) => cx - (cx - px) * ratio);
        setPanY((py) => cy - (cy - py) * ratio);
        return next;
      });
    };
    frame.addEventListener('wheel', handler, { passive: false });
    return () => {
      frame.removeEventListener('wheel', handler);
    };
  }, [cropMode]);

  const startPanDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (cropMode) return;
    const target = event.target as HTMLElement;
    // Don't hijack drags that started on a crop handle — those are
    // captured separately and need their own pointer-capture path.
    if (target.closest('.crop-handle, .crop-box')) return;
    // Capture even when zoom is at fit so we can detect a click (no
    // movement on pointer-up) and toggle zoom-to-1:1 anchored there.
    event.currentTarget.setPointerCapture?.(event.pointerId);
    panDragRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      basePanX: panX,
      basePanY: panY,
      moved: false,
    };
  };

  const movePanDrag = (event: PointerEvent<HTMLDivElement>) => {
    const drag = panDragRef.current;
    if (!drag) return;
    const dx = event.clientX - drag.startX;
    const dy = event.clientY - drag.startY;
    if (!drag.moved && Math.hypot(dx, dy) > CLICK_THRESHOLD_PX) {
      drag.moved = true;
    }
    // Pan only kicks in when the image is actually larger than the
    // viewport — at fit-zoom there's nothing to pan to, but we still
    // want to capture the pointer so we can detect a click on release.
    if (zoom <= 100) return;
    setPanX(drag.basePanX + dx);
    setPanY(drag.basePanY + dy);
  };

  const stopPanDrag = (event: PointerEvent<HTMLDivElement>) => {
    const drag = panDragRef.current;
    if (!drag) return;
    panDragRef.current = null;
    event.currentTarget.releasePointerCapture?.(event.pointerId);
    if (drag.moved) return;
    // No drag → click. Toggle between fit-zoom and 1:1, anchored at the
    // click position so the pixel under the cursor stays put. Matches
    // Lightroom's space-bar / click-to-zoom behavior.
    const frame = frameRef.current;
    const wrapper = zoomWrapperRef.current;
    if (!frame || !wrapper) return;
    const frameRect = frame.getBoundingClientRect();
    const cx = event.clientX - frameRect.left - frameRect.width / 2;
    const cy = event.clientY - frameRect.top - frameRect.height / 2;
    const wrapperRect = wrapper.getBoundingClientRect();
    const longEdge = Math.max(wrapperRect.width, wrapperRect.height);
    const oneToOneZoom = longEdge > 0 ? clampNumber((2048 / longEdge) * 100, ZOOM_MIN, ZOOM_MAX) : 200;
    const isAtFit = Math.abs(zoom - 100) < 3;
    if (isAtFit) {
      const ratio = oneToOneZoom / zoom;
      setZoom(oneToOneZoom);
      setPanX(cx - (cx - panX) * ratio);
      setPanY(cy - (cy - panY) * ratio);
    } else {
      setZoom(100);
      setPanX(0);
      setPanY(0);
    }
  };

  return (
    <div className="editor-stage">
      <div className="editor-main">
        <div className="editor-canvas">
          <div className="develop-canvas-toolbar">
            <button type="button" className="btn" onClick={zoomOut} aria-label="Zoom out">
              -
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => setZoomMenuOpen((o) => !o)}
              aria-haspopup="menu"
              aria-expanded={zoomMenuOpen}
            >
              {Math.round(zoom)}%
            </button>
            <button type="button" className="btn" onClick={zoomIn} aria-label="Zoom in">
              +
            </button>
            <button type="button" className="btn" onClick={resetView}>
              Fit
            </button>
            <button type="button" className="btn" onClick={goOneToOne} title="Native preview pixels">
              1:1
            </button>
            {zoomMenuOpen ? (
              <div className="develop-zoom-menu" role="menu">
                {[25, 50, 100, 200, 400].map((preset) => (
                  <button
                    key={preset}
                    type="button"
                    role="menuitem"
                    className="develop-zoom-menu-item"
                    onClick={() => {
                      setZoom(preset);
                      setPanX(0);
                      setPanY(0);
                      setZoomMenuOpen(false);
                    }}
                  >
                    {preset}%
                  </button>
                ))}
              </div>
            ) : null}
          </div>
          <div
            ref={frameRef}
            data-testid="develop-canvas-frame"
            className="develop-canvas-frame"
            onPointerDown={startPanDrag}
            onPointerMove={(event) => {
              movePanDrag(event);
              updateCropDrag(event);
            }}
            onPointerUp={(event) => {
              stopPanDrag(event);
              stopCropDrag();
            }}
            onPointerCancel={(event) => {
              stopPanDrag(event);
              stopCropDrag();
            }}
            style={{
              width: '100%',
              height: '100%',
              position: 'relative',
              overflow: 'hidden',
              cursor: cropMode ? 'default' : zoom > 100 ? 'grab' : 'zoom-in',
            }}
          >
            {/* The zoom wrapper sizes to the photo's aspect inside the
                full-bleed frame so the crop overlay's percentage coords
                stay aligned with the actual image. `width/height: auto`
                + `max-width/height: 100%` resolves to the largest
                photo-aspect rectangle that fits, centered via auto
                margins (Lightroom's Fit semantics). */}
            <div
              ref={zoomWrapperRef}
              className="develop-canvas-zoom"
              style={{
                position: 'absolute',
                inset: 0,
                margin: 'auto',
                aspectRatio: stageAspect,
                width: 'auto',
                height: 'auto',
                maxWidth: '100%',
                maxHeight: '100%',
                transformOrigin: 'center center',
                transform: `translate(${panX}px, ${panY}px) scale(${zoom / 100})`,
                willChange: 'transform',
              }}
            >
              {preview ? (
                <img
                  src={preview}
                  alt={photo.filename}
                  draggable={false}
                  style={{
                    position: 'absolute',
                    inset: 0,
                    width: '100%',
                    height: '100%',
                    objectFit: 'contain',
                    borderRadius: 4,
                    background: 'var(--bg-chrome)',
                    userSelect: 'none',
                  }}
                />
              ) : (
                <div
                  className="mono"
                  style={{
                    position: 'absolute',
                    inset: 0,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    color: 'var(--fg-mute)',
                    fontSize: 12,
                  }}
                >
                  Rendering…
                </div>
              )}
              {selectedMaskPreview && (
                <div
                  className={`local-mask-preview ${selectedMaskPreview.className}`}
                  style={selectedMaskPreview.style}
                  aria-hidden="true"
                  data-testid="selected-mask-overlay"
                />
              )}
              {showCropOverlay && (
                <>
                  <div className="crop-dim crop-dim-top" style={{ height: `${cropTop}%` }} />
                  <div
                    className="crop-dim crop-dim-left"
                    style={{ top: `${cropTop}%`, width: `${cropLeft}%`, height: `${cropHeight}%` }}
                  />
                  <div
                    className="crop-dim crop-dim-right"
                    style={{
                      top: `${cropTop}%`,
                      left: `${cropLeft + cropWidth}%`,
                      right: 0,
                      height: `${cropHeight}%`,
                    }}
                  />
                  <div className="crop-dim crop-dim-bottom" style={{ top: `${cropTop + cropHeight}%` }} />
                  <div
                    className={cropMode ? 'crop-box active' : 'crop-box passive'}
                    style={{
                      left: `${cropLeft}%`,
                      top: `${cropTop}%`,
                      width: `${cropWidth}%`,
                      height: `${cropHeight}%`,
                    }}
                    onPointerDown={(event) => startCropDrag('move', event)}
                    role="presentation"
                  >
                    <div className="crop-grid" />
                    {cropMode &&
                      (['nw', 'ne', 'sw', 'se'] as const).map((handle) => (
                        <button
                          key={handle}
                          type="button"
                          className={`crop-handle ${handle}`}
                          aria-label={`Resize crop ${handle}`}
                          onPointerDown={(event) => {
                            event.stopPropagation();
                            startCropDrag(handle, event);
                          }}
                        />
                      ))}
                  </div>
                </>
              )}
            </div>
          </div>
          {cropMode && (
            <div className="develop-crop-toolbar">
              <span className="mono">Crop</span>
              {[
                ['Original', { cropX: 0, cropY: 0, cropW: 100, cropH: 100 }],
                ['1:1', { cropX: 12.5, cropY: 0, cropW: 75, cropH: 100 }],
                ['4:5', { cropX: 10, cropY: 0, cropW: 80, cropH: 100 }],
                ['16:9', { cropX: 0, cropY: 21.9, cropW: 100, cropH: 56.2 }],
              ].map(([label, patch]) => (
                <button
                  key={label as string}
                  type="button"
                  className="btn"
                  onClick={() => applyAspect(patch as Partial<DevelopValues>)}
                >
                  {label as string}
                </button>
              ))}
              <button type="button" className="btn" onClick={resetCrop} disabled={!isCropping}>
                Reset
              </button>
              <button type="button" className="btn primary" onClick={() => setCropMode(false)}>
                Done
              </button>
            </div>
          )}
        </div>
        <div className="editor-strip">
          {photos.slice(0, 14).map((p, i) => (
            <button
              key={p.id}
              type="button"
              className={`thumb ${i === focusedIdx ? 'active' : ''}`}
              onClick={() => setFocusedIdx(i)}
              title={p.filename}
            >
              <Thumbnail
                photoId={p.id}
                sizePx={thumbnailSizeForCssBox(96, 64, { maxPx: 240 })}
                photo={{ hue: (p.id * 31) % 360, filename: p.filename, id: String(p.id) }}
              />
            </button>
          ))}
        </div>
      </div>
      <EditorInspector
        photo={photo}
        values={values}
        mode={inspectorMode}
        targetName={selectedMask?.name}
        targetMeta={
          selectedMask
            ? `${selectedMask.source.replaceAll('_', ' ')} / ${selectedMask.mode}${selectedMask.visible ? '' : ' / hidden'}`
            : undefined
        }
        cropMode={cropMode}
        onToggleCropMode={() => setCropMode(!cropMode)}
        onClearMaskSelection={onClearMaskSelection}
        onChange={onValueChange}
        onChangeMany={onValuesChange}
        onCurvesChange={onCurvesChange}
        onAutoLight={onAutoLight}
        onReset={onReset}
        onCopy={onCopy}
        onPaste={onPaste}
        canPaste={canPaste}
      />
    </div>
  );
}

const clampNumber = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

function localMaskPreview(
  mask: DevelopMask,
  opacity: number,
  cropValues: DevelopValues | null = null,
): {
  className: string;
  style: CSSProperties;
} {
  let payload: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(mask.mask_payload);
    if (parsed && typeof parsed === 'object') payload = parsed as Record<string, unknown>;
  } catch {
    payload = {};
  }
  const kind = typeof payload.kind === 'string' ? payload.kind : mask.source;
  const cropLeft = cropValues ? clampNumber(cropValues.cropX, 0, 95) : 0;
  const cropTop = cropValues ? clampNumber(cropValues.cropY, 0, 95) : 0;
  const cropWidth = cropValues ? clampNumber(cropValues.cropW, 5, 100 - cropLeft) : 100;
  const cropHeight = cropValues ? clampNumber(cropValues.cropH, 5, 100 - cropTop) : 100;
  const cropStyle: CSSProperties =
    cropValues == null
      ? {}
      : {
          left: `${-(cropLeft / cropWidth) * 100}%`,
          top: `${-(cropTop / cropHeight) * 100}%`,
          right: 'auto',
          bottom: 'auto',
          width: `${10000 / cropWidth}%`,
          height: `${10000 / cropHeight}%`,
        };
  const baseStyle = {
    ...cropStyle,
    '--mask-opacity': String(clampNumber(opacity, 0.1, 1)),
  } as CSSProperties & Record<'--mask-opacity', string>;
  if (kind === 'bitmap' && typeof payload.data_b64 === 'string') {
    return {
      className: 'kind-bitmap',
      style: {
        ...baseStyle,
        '--mask-image': `url("data:image/png;base64,${payload.data_b64}")`,
      } as CSSProperties,
    };
  }
  const cx = typeof payload.cx === 'number' ? payload.cx : 0.5;
  const cy = typeof payload.cy === 'number' ? payload.cy : 0.5;
  const radius = typeof payload.radius === 'number' ? payload.radius : 0.35;
  const top = typeof payload.top === 'number' ? payload.top : 0;
  const bottom = typeof payload.bottom === 'number' ? payload.bottom : 0.55;
  const style = {
    ...baseStyle,
    '--mask-cx': `${clampNumber(cx, 0, 1) * 100}%`,
    '--mask-cy': `${clampNumber(cy, 0, 1) * 100}%`,
    '--mask-radius': `${clampNumber(radius, 0.02, 1) * 100}%`,
    '--mask-top': `${clampNumber(top, 0, 1) * 100}%`,
    '--mask-bottom': `${clampNumber(bottom, 0, 1) * 100}%`,
  } as CSSProperties;

  return { className: `kind-${kind.replaceAll('_', '-')}`, style };
}

interface PromptStageProps {
  photo: PhotoRow;
  promptText: string;
  setPromptText: (s: string) => void;
  promptStrength: number;
  setPromptStrength: (n: number) => void;
  constraints: string[];
  setConstraints: (s: string[]) => void;
}

function PromptStage({
  photo,
  promptText,
  setPromptText,
  promptStrength,
  setPromptStrength,
  constraints,
  setConstraints,
}: PromptStageProps) {
  const [newConstraint, setNewConstraint] = useState('');
  const [status, setStatus] = useState<SidecarStatus | null>(null);
  const [generating, setGenerating] = useState(false);
  const [renderedB64, setRenderedB64] = useState<string | null>(null);
  const [renderError, setRenderError] = useState<string | null>(null);
  const [maskPrompt, setMaskPrompt] = useState('the main subject');
  const [masking, setMasking] = useState(false);
  const [currentEditId, setCurrentEditId] = useState<number | null>(null);
  const [acceptedB64, setAcceptedB64] = useState<string | null>(null);
  const activeMask = useDevelopUi((s) => s.activeMask);
  const setActiveMask = useDevelopUi((s) => s.setActiveMask);
  const { data: aiEdits = [] } = useAiEditStatus(photo.id);
  const refreshAiEdit = useAiEditRefresh();

  useEffect(() => {
    if (activeMask?.prompt) setMaskPrompt(activeMask.prompt);
  }, [activeMask?.prompt]);

  useEffect(() => {
    let cancelled = false;
    const ping = () => {
      promptSidecarPing()
        .then((s) => {
          if (!cancelled) setStatus(s);
        })
        .catch((e) => {
          if (!cancelled) warn('prompt sidecar ping failed', e);
        });
    };
    ping();
    // Keep the badge honest — if the sidecar crashes between generations
    // the chip flips to "Unreachable" within 30 seconds.
    const id = globalThis.setInterval(ping, 30_000);
    return () => {
      cancelled = true;
      globalThis.clearInterval(id);
    };
  }, []);

  const addConstraint = () => {
    const v = newConstraint.trim();
    if (!v || constraints.includes(v)) return;
    setConstraints([...constraints, v]);
    setNewConstraint('');
  };
  const removeConstraint = (s: string) => {
    setConstraints(constraints.filter((c) => c !== s));
  };

  const canGenerate = !!status?.configured && status?.reachable && !generating;
  const sidecarBadge = !status
    ? 'Checking sidecar…'
    : status.configured && status.reachable
      ? `Connected · ${status.model ?? 'flux-dev'}`
      : status.configured
        ? `Unreachable${status.error ? ` · ${status.error}` : ''}`
        : 'No sidecar configured';

  const onGenerate = async () => {
    if (!canGenerate) return;
    setGenerating(true);
    setRenderError(null);
    try {
      const result = await promptEdit({
        photo_id: photo.id,
        prompt: promptText,
        strength: promptStrength,
        constraints,
        mask_b64: activeMask?.maskB64 ?? null,
      });
      setRenderedB64(result.image_b64);
      refreshAiEdit.mutate({ photoId: photo.id, feature: 'prompt_edit' });
      // The sidecar writes a prompt_edits row server-side. The UI tracks
      // the freshest id so Accept/Reject can target it. We fetch the
      // latest pending row rather than threading the id back through the
      // command payload.
      try {
        const { promptEditList } = await import('../../tauri/invoke');
        const rows = await promptEditList(photo.id);
        const pending = rows.find((r) => r.state === 'pending');
        if (pending) setCurrentEditId(pending.id);
      } catch (e) {
        warn('prompt history list failed', e);
      }
    } catch (e) {
      setRenderError(String(e));
    } finally {
      setGenerating(false);
    }
  };

  const onAccept = async () => {
    if (currentEditId == null) return;
    try {
      await promptEditAccept(currentEditId);
      setAcceptedB64(renderedB64);
      setRenderedB64(null);
      setCurrentEditId(null);
    } catch (e) {
      setRenderError(String(e));
    }
  };

  const onReject = async () => {
    if (currentEditId == null) {
      setRenderedB64(null);
      return;
    }
    try {
      await promptEditReject(currentEditId);
      setRenderedB64(null);
      setCurrentEditId(null);
    } catch (e) {
      setRenderError(String(e));
    }
  };

  const onMask = async () => {
    const rawPrompt = maskPrompt.trim();
    if (!rawPrompt || !status?.reachable) return;
    setMasking(true);
    setRenderError(null);
    try {
      const result = await maskFromPrompt({
        photo_id: photo.id,
        prompt: rawPrompt,
      });
      setActiveMask({
        prompt: rawPrompt,
        maskB64: result.mask_b64,
        confidence: result.confidence,
        latencyMs: result.latency_ms,
        createdAt: new Date().toISOString(),
      });
      refreshAiEdit.mutate({ photoId: photo.id, feature: 'prompt_mask' });
    } catch (e) {
      setRenderError(String(e));
    } finally {
      setMasking(false);
    }
  };
  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
      <div
        style={{
          flex: 1,
          display: 'grid',
          gridTemplateColumns: '1fr 1fr',
          gap: 2,
          background: 'var(--stroke)',
          minHeight: 0,
        }}
      >
        <div
          style={{
            background: 'var(--bg)',
            padding: 20,
            display: 'flex',
            flexDirection: 'column',
            gap: 8,
            minHeight: 0,
          }}
        >
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
            BEFORE · original
          </div>
          <div className="prompt-before-frame" style={{ flex: 1, minHeight: 0, position: 'relative' }}>
            <Thumbnail
              photoId={photo.id}
              sizePx={thumbnailSizeForCssBox(700, 700, { maxPx: 1280 })}
              photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
              fit="contain"
            />
          </div>
        </div>
        <div
          style={{
            background: 'var(--bg)',
            padding: 20,
            display: 'flex',
            flexDirection: 'column',
            gap: 8,
            minHeight: 0,
            position: 'relative',
          }}
        >
          <div
            className="mono"
            style={{
              fontSize: 10.5,
              color: status?.reachable ? 'var(--accent)' : 'var(--fg-mute)',
              letterSpacing: '0.08em',
            }}
          >
            AFTER · {sidecarBadge}
          </div>
          <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>
            {renderedB64 ? (
              <img
                src={`data:image/png;base64,${renderedB64}`}
                alt="Generated result"
                style={{ width: '100%', height: '100%', objectFit: 'contain' }}
              />
            ) : acceptedB64 ? (
              <img
                src={`data:image/png;base64,${acceptedB64}`}
                alt="Accepted generation"
                style={{ width: '100%', height: '100%', objectFit: 'contain' }}
              />
            ) : (
              <Placeholder
                photo={{ hue: 220, filename: photo.filename, id: String(photo.id) }}
                showLabel={false}
              />
            )}
            {renderedB64 && (
              <div
                style={{
                  position: 'absolute',
                  left: 10,
                  bottom: 10,
                  display: 'flex',
                  gap: 6,
                }}
              >
                <button
                  type="button"
                  className="btn primary"
                  onClick={onAccept}
                  style={{ padding: '5px 11px', fontSize: 11.5 }}
                  title="Save this generation and close it"
                >
                  Accept
                </button>
                <button
                  type="button"
                  className="btn"
                  onClick={onReject}
                  style={{ padding: '5px 11px', fontSize: 11.5 }}
                  title="Discard this generation"
                >
                  Reject
                </button>
              </div>
            )}
            <div style={{ position: 'absolute', top: 10, right: 10 }}>
              {status?.reachable ? (
                <Chip variant="solid">AI · {status.model ?? 'flux-dev'}</Chip>
              ) : (
                <Chip>AI · offline</Chip>
              )}
            </div>
            {renderError && (
              <div
                className="mono"
                style={{
                  position: 'absolute',
                  left: 10,
                  bottom: 10,
                  right: 10,
                  padding: '6px 10px',
                  fontSize: 11,
                  color: 'var(--danger, #d66)',
                  background: 'color-mix(in oklch, var(--danger, #d66) 15%, var(--bg-elev))',
                  borderRadius: 4,
                }}
              >
                {renderError}
              </div>
            )}
          </div>
        </div>
      </div>
      <div style={{ padding: 14, borderTop: '1px solid var(--stroke)', background: 'var(--bg-chrome)' }}>
        <div className="prompt-box">
          <div
            className="mono"
            style={{
              color: 'var(--fg-mute)',
              fontSize: 10.5,
              letterSpacing: '0.08em',
              display: 'flex',
              gap: 6,
              alignItems: 'center',
            }}
          >
            <Icon name="sparkles" size={11} /> DESCRIBE YOUR EDIT
          </div>
          <textarea
            value={promptText}
            onChange={(e) => setPromptText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                onGenerate();
              }
            }}
            aria-label="Prompt describing the edit"
            style={{ minHeight: 48 }}
          />
          <div className="prompt-row">
            <button
              type="button"
              className={status?.reachable ? 'btn' : 'btn phase-gated'}
              disabled={!status?.reachable || masking}
              aria-disabled={!status?.reachable || masking}
              onClick={status?.reachable ? onMask : undefined}
              title={
                status?.reachable
                  ? 'Ask the sidecar to mask a region by prompt'
                  : 'Needs a reachable Flux/SDXL sidecar'
              }
              style={{ padding: '5px 9px', fontSize: 11.5 }}
            >
              <Icon name="brush" size={12} /> {masking ? 'Masking...' : activeMask ? 'Mask on' : 'Mask'}
            </button>
            <input
              type="text"
              value={maskPrompt}
              onChange={(e) => setMaskPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  onMask();
                }
              }}
              placeholder="mask prompt"
              aria-label="Mask prompt"
              style={{
                width: 130,
                fontSize: 11.5,
                padding: '3px 8px',
                border: '1px dashed var(--stroke-strong)',
                borderRadius: 999,
                background: 'transparent',
                color: 'var(--fg)',
              }}
            />
            {activeMask && (
              <Chip onClose={() => setActiveMask(null)}>
                {activeMask.prompt} · {Math.round(activeMask.confidence * 100)}%
              </Chip>
            )}
            {constraints.map((c) => (
              <Chip key={c} onClose={() => removeConstraint(c)}>
                {c}
              </Chip>
            ))}
            {aiEdits.map((edit) => (
              <Chip
                key={edit.id}
                variant={edit.state === 'current' ? 'solid' : undefined}
                onClose={
                  edit.state === 'stale'
                    ? () => refreshAiEdit.mutate({ photoId: photo.id, feature: edit.feature })
                    : undefined
                }
              >
                {edit.feature} · {edit.state}
              </Chip>
            ))}
            <input
              type="text"
              value={newConstraint}
              onChange={(e) => setNewConstraint(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  addConstraint();
                }
              }}
              placeholder="+ add constraint"
              aria-label="Add a constraint"
              style={{
                width: 140,
                fontSize: 11.5,
                padding: '3px 8px',
                border: '1px dashed var(--stroke-strong)',
                borderRadius: 999,
                background: 'transparent',
                color: 'var(--fg)',
              }}
            />
            <div style={{ flex: 1 }} />
            <span className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)' }}>
              Strength {promptStrength}
            </span>
            <input
              type="range"
              min="0"
              max="100"
              value={promptStrength}
              onChange={(e) => setPromptStrength(Number(e.target.value))}
              aria-label="Prompt strength"
              style={{ width: 80, accentColor: 'var(--accent)' }}
            />
            <button
              type="button"
              className={canGenerate ? 'btn primary' : 'btn primary phase-gated'}
              disabled={!canGenerate}
              aria-disabled={!canGenerate}
              onClick={canGenerate ? onGenerate : undefined}
              title={
                canGenerate
                  ? 'Submit to the configured sidecar'
                  : status?.configured
                    ? 'Sidecar unreachable — check Settings → AI models'
                    : 'Set a Flux/SDXL sidecar URL in Settings → AI models'
              }
              style={{ padding: '6px 12px', fontSize: 12 }}
            >
              <Icon name="sparkles" size={12} /> {generating ? 'Generating…' : 'Generate'}{' '}
              <span className="kbd">⌘↵</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
