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
import { Slider } from '../../primitives/Slider';
import { Thumbnail, thumbnailSizeForCssBox } from '../../primitives/Thumbnail';
import { useDevelopUi } from '../../state/develop';
import {
  type DevelopOperations,
  useAiEditRefresh,
  useAiEditStatus,
  useDevelopApply,
  useDevelopMaskApplyPreview,
  useDevelopMaskCreate,
  useDevelopMaskDelete,
  useDevelopMaskGenerate,
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
import { type CurveChannel, CurvesPanel } from './CurvesPanel';
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
          { value: 'mask', label: 'Mask' },
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
  const frameRef = useRef<HTMLDivElement | null>(null);
  const cropDragRef = useRef<{
    kind: 'move' | 'nw' | 'ne' | 'sw' | 'se';
    startX: number;
    startY: number;
    start: Pick<DevelopValues, 'cropX' | 'cropY' | 'cropW' | 'cropH'>;
  } | null>(null);

  const zoomIn = () => setZoom((z) => clampNumber(z + 25, 25, 300));
  const zoomOut = () => setZoom((z) => clampNumber(z - 25, 25, 300));
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
    if (!activeDrag || !frameRef.current) return;
    const rect = frameRef.current.getBoundingClientRect();
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

  return (
    <div className="editor-stage">
      <div className="editor-main">
        <div className="editor-canvas">
          <div className="develop-canvas-toolbar">
            <button type="button" className="btn" onClick={zoomOut} aria-label="Zoom out">
              -
            </button>
            <button type="button" className="btn" onClick={() => setZoom(100)}>
              {zoom}%
            </button>
            <button type="button" className="btn" onClick={zoomIn} aria-label="Zoom in">
              +
            </button>
            <button type="button" className="btn" onClick={() => setZoom(100)}>
              Fit
            </button>
          </div>
          <div
            ref={frameRef}
            data-testid="develop-canvas-frame"
            className="develop-canvas-frame"
            onPointerMove={updateCropDrag}
            onPointerUp={stopCropDrag}
            onPointerCancel={stopCropDrag}
            style={{
              width: 'min(100%, 1100px)',
              aspectRatio: stageAspect,
              maxHeight: '100%',
              position: 'relative',
              transform: `scale(${zoom / 100})`,
            }}
          >
            {preview ? (
              <img
                src={preview}
                alt={photo.filename}
                style={{
                  position: 'absolute',
                  inset: 0,
                  width: '100%',
                  height: '100%',
                  objectFit: 'contain',
                  borderRadius: 4,
                  background: 'var(--bg-chrome)',
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

const MASK_PRESETS = [
  { id: 'subject', label: 'Subject', icon: 'faces' as const },
  { id: 'sky', label: 'Sky', icon: 'cloud' as const },
  { id: 'person', label: 'Person', icon: 'faces' as const },
  { id: 'object', label: 'Objects', icon: 'wand' as const },
  { id: 'foreground', label: 'Foreground', icon: 'layers' as const },
  { id: 'background', label: 'Background', icon: 'grid' as const },
];

type MaskPreset = (typeof MASK_PRESETS)[number];
type MaskMode = 'normal' | 'add' | 'subtract' | 'intersect';
const MASK_MODES: { id: MaskMode; label: string }[] = [
  { id: 'normal', label: 'New' },
  { id: 'add', label: 'Add' },
  { id: 'subtract', label: 'Subtract' },
  { id: 'intersect', label: 'Intersect' },
];

const clampNumber = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

interface MaskStageProps {
  photo: PhotoRow;
  stageAspect: string;
  preview: string | null;
  operations: DevelopOperations;
  onPreview: (previewDataUrl: string) => void;
}

export function MaskStage({ photo, stageAspect, preview, operations, onPreview }: MaskStageProps) {
  const [masking, setMasking] = useState(false);
  const [maskError, setMaskError] = useState<string | null>(null);
  const [activePresetId, setActivePresetId] = useState<string | null>(null);
  const [maskMode, setMaskMode] = useState<MaskMode>('normal');
  const [selectedMaskId, setSelectedMaskId] = useState<number | null>(null);
  const [overlayVisible, setOverlayVisible] = useState(true);
  const [overlayOpacity, setOverlayOpacity] = useState(62);
  const [curveChannel, setCurveChannel] = useState<CurveChannel>('rgb');
  const [localMaskOperations, setLocalMaskOperations] = useState<Record<number, DevelopOperations>>({});
  const maskUpdateTimer = useRef<number | null>(null);
  const activeMask = useDevelopUi((s) => s.activeMask);
  const setActiveMask = useDevelopUi((s) => s.setActiveMask);
  const { data: masks = [] } = useDevelopMasks(photo.id);
  const createMask = useDevelopMaskCreate();
  const generateMask = useDevelopMaskGenerate();
  const updateMask = useDevelopMaskUpdate();
  const deleteMask = useDevelopMaskDelete();
  const applyMaskPreview = useDevelopMaskApplyPreview();

  useEffect(() => {
    return () => {
      if (maskUpdateTimer.current !== null) window.clearTimeout(maskUpdateTimer.current);
    };
  }, []);

  useEffect(() => {
    if (masks.length === 0) {
      setSelectedMaskId(null);
      return;
    }
    if (selectedMaskId == null || !masks.some((mask) => mask.id === selectedMaskId)) {
      setSelectedMaskId(masks[0]?.id ?? null);
    }
  }, [masks, selectedMaskId]);

  const canMask = !masking && !generateMask.isPending;
  const canCreateManualMask = !createMask.isPending;
  const statusLabel = masking ? 'Generating' : 'Local bitmap masks';

  const refreshPreview = useCallback(() => {
    applyMaskPreview.mutate(
      { photoId: photo.id, operations },
      {
        onSuccess: (receipt) => onPreview(receipt.preview_data_url),
      },
    );
  }, [applyMaskPreview, onPreview, operations, photo.id]);

  const maskOperations = useCallback(
    (mask: DevelopMask) =>
      sanitizeMaskOperations(
        localMaskOperations[mask.id] ?? parseOperationsJson(mask.operations_json) ?? identityOperations(),
      ),
    [localMaskOperations],
  );

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
              refreshPreview();
            },
            onError: (e) => setMaskError(String(e)),
          },
        );
      }, 120);
    },
    [refreshPreview, updateMask],
  );

  const runMask = (preset: MaskPreset) => {
    if (!canMask) return;
    const photoId = photo.id;
    setActivePresetId(preset.id);
    setMasking(true);
    setMaskError(null);
    setActiveMask(null);
    generateMask.mutate(
      {
        photo_id: photoId,
        name: `${preset.label} mask`,
        source: preset.id,
        mode: maskMode,
        operations: { ...identityOperations(), exposure: preset.id === 'sky' ? 0.35 : 0.2 },
      },
      {
        onSuccess: (receipt) => {
          setSelectedMaskId(receipt.mask.id);
          if (useDevelopUi.getState().focusedPhotoId === photoId) onPreview(receipt.preview_data_url);
        },
        onError: (e) => setMaskError(String(e)),
        onSettled: () => setMasking(false),
      },
    );
  };

  function createManualMask(kind: 'brush' | 'linear_gradient' | 'radial_gradient') {
    const label =
      kind === 'brush' ? 'Brush mask' : kind === 'linear_gradient' ? 'Linear gradient' : 'Radial gradient';
    createMask.mutate(
      {
        photo_id: photo.id,
        name: label,
        source: kind,
        mode: maskMode,
        payload_storage: 'inline',
        mask_payload:
          kind === 'linear_gradient'
            ? { kind, top: 0.0, bottom: 0.55 }
            : kind === 'radial_gradient'
              ? { kind, cx: 0.5, cy: 0.5, radius: 0.35, feather: 0.35 }
              : { kind, cx: 0.5, cy: 0.5, radius: 0.18, feather: 0.45, flow: 1.0, density: 1.0 },
        operations: { ...identityOperations(), exposure: kind === 'brush' ? 0.2 : 0.35 },
      },
      {
        onSuccess: (maskId) => {
          setSelectedMaskId(maskId);
          refreshPreview();
        },
      },
    );
  }

  function setMaskExposure(maskId: number, exposure: number) {
    const current = masks.find((mask) => mask.id === maskId);
    const nextOps = sanitizeMaskOperations({
      ...(current ? maskOperations(current) : identityOperations()),
      exposure,
    });
    setLocalMaskOperations((local) => ({ ...local, [maskId]: nextOps }));
    updateMask.mutate(
      {
        mask_id: maskId,
        operations: nextOps,
      },
      {
        onSuccess: () => refreshPreview(),
      },
    );
  }

  function setMaskModeForLayer(maskId: number, mode: MaskMode) {
    updateMask.mutate(
      {
        mask_id: maskId,
        mode,
      },
      {
        onSuccess: () => refreshPreview(),
      },
    );
  }

  function maskExposure(mask: (typeof masks)[number]) {
    return maskOperations(mask).exposure;
  }

  const selectedMask =
    (selectedMaskId ? masks.find((mask) => mask.id === selectedMaskId) : null) ?? masks[0] ?? null;
  const selectedMaskOps = selectedMask ? maskOperations(selectedMask) : null;
  const selectedMaskValues = selectedMaskOps ? operationsToValues(selectedMaskOps) : null;
  const selectedMaskPreview =
    selectedMask && overlayVisible ? localMaskPreview(selectedMask, overlayOpacity / 100) : null;

  function updateSelectedMaskValues(patch: Partial<DevelopValues>) {
    if (!selectedMask || !selectedMaskValues) return;
    const nextValues = { ...selectedMaskValues, ...patch };
    const nextOps = sanitizeMaskOperations(valuesToOperations(nextValues));
    setLocalMaskOperations((local) => ({ ...local, [selectedMask.id]: nextOps }));
    queueMaskOperationUpdate(selectedMask.id, nextOps);
  }

  function updateSelectedMaskCurves(curves: DevelopValues['curves']) {
    updateSelectedMaskValues({ curves });
  }

  function resetSelectedMaskOperations() {
    if (!selectedMask) return;
    const nextOps = sanitizeMaskOperations(identityOperations());
    setLocalMaskOperations((local) => ({ ...local, [selectedMask.id]: nextOps }));
    queueMaskOperationUpdate(selectedMask.id, nextOps);
  }

  return (
    <div className="mask-stage">
      <div className="mask-main">
        <div className="mask-canvas-wrap">
          <div className="mask-canvas" style={{ aspectRatio: stageAspect }}>
            {preview ? (
              <img src={preview} alt={photo.filename} className="mask-base" />
            ) : (
              <Thumbnail
                photoId={photo.id}
                sizePx={thumbnailSizeForCssBox(1100, 800, { maxPx: 1280 })}
                photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
                fit="contain"
              />
            )}
            {selectedMaskPreview && (
              <div
                className={`local-mask-preview ${selectedMaskPreview.className}`}
                style={selectedMaskPreview.style}
                aria-hidden="true"
                data-testid="selected-mask-overlay"
              />
            )}
            <div className="mask-status">
              <Chip variant="solid">Mask · {statusLabel}</Chip>
              {selectedMask && (
                <Chip>
                  {selectedMask.name}
                  {selectedMask.confidence != null ? ` / ${Math.round(selectedMask.confidence * 100)}%` : ''}
                </Chip>
              )}
              {activeMask && (
                <Chip onClose={() => setActiveMask(null)}>
                  {activeMask.prompt} · {Math.round(activeMask.confidence * 100)}%
                </Chip>
              )}
            </div>
          </div>
        </div>
      </div>

      <div className="mask-panel editor-inspector">
        <div className="editor-ihead">
          <div>
            <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-dim)' }}>
              Masking
            </div>
            <div className="mask-subtitle">Create local adjustment masks, then refine each layer.</div>
          </div>
          <button
            type="button"
            className={overlayVisible ? 'btn on' : 'btn'}
            onClick={() => setOverlayVisible((visible) => !visible)}
            disabled={!selectedMask}
            aria-disabled={!selectedMask}
            aria-pressed={overlayVisible}
            style={{ padding: '5px 9px', fontSize: 11.5 }}
          >
            Overlay
          </button>
        </div>
        <div className="editor-ibody">
          <div className="editor-group">
            <div className="mask-create-head">
              <h4>Create New Mask</h4>
              <fieldset className="mask-mode-seg">
                <legend className="mask-mode-legend">Mask combine mode</legend>
                {MASK_MODES.map((mode) => (
                  <button
                    key={mode.id}
                    type="button"
                    className={maskMode === mode.id ? 'on' : ''}
                    onClick={() => setMaskMode(mode.id)}
                    aria-pressed={maskMode === mode.id}
                  >
                    {mode.label}
                  </button>
                ))}
              </fieldset>
            </div>
            <h4>AI masks</h4>
            <div className="mask-preset-grid">
              {MASK_PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  className={activePresetId === preset.id ? 'btn primary' : 'btn'}
                  disabled={!canMask}
                  aria-disabled={!canMask}
                  onClick={() => runMask(preset)}
                  title={canMask ? `Create a local ${preset.label.toLowerCase()} mask` : 'Creating mask'}
                >
                  <Icon name={preset.icon} size={12} /> {preset.label}
                </button>
              ))}
            </div>
          </div>

          <div className="editor-group">
            <h4>Brush & Gradients</h4>
            <div className="mask-preset-grid">
              <button
                type="button"
                className="btn"
                onClick={() => createManualMask('brush')}
                disabled={!canCreateManualMask}
              >
                <Icon name="brush" size={12} /> Brush
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => createManualMask('linear_gradient')}
                disabled={!canCreateManualMask}
              >
                Linear gradient
              </button>
              <button
                type="button"
                className="btn"
                onClick={() => createManualMask('radial_gradient')}
                disabled={!canCreateManualMask}
              >
                Radial gradient
              </button>
              <button
                type="button"
                className="btn"
                onClick={refreshPreview}
                disabled={applyMaskPreview.isPending}
              >
                {applyMaskPreview.isPending ? 'Rendering...' : 'Refresh'}
              </button>
            </div>
          </div>

          <div className="editor-group">
            <h4>Selected mask</h4>
            {selectedMask && selectedMaskValues ? (
              <div className="mask-adjustments">
                <div className="mask-selected-card">
                  <div>
                    <strong>{selectedMask.name}</strong>
                    <span className="mono">
                      {selectedMask.source.replaceAll('_', ' ')} / {selectedMask.mode}
                      {selectedMask.visible ? '' : ' / hidden'}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="btn"
                    onClick={resetSelectedMaskOperations}
                    disabled={updateMask.isPending}
                  >
                    Reset
                  </button>
                </div>
                <label className="mask-overlay-toggle">
                  <input
                    type="checkbox"
                    checked={overlayVisible}
                    onChange={(event) => setOverlayVisible(event.currentTarget.checked)}
                  />
                  Show overlay
                </label>
                <Slider
                  label="Overlay opacity"
                  value={overlayOpacity}
                  onChange={setOverlayOpacity}
                  min={10}
                  max={100}
                  suffix="%"
                />
                <MaskAdjustmentPanel
                  values={selectedMaskValues}
                  onChange={(key, value) =>
                    updateSelectedMaskValues({ [key]: value } as Partial<DevelopValues>)
                  }
                  onChangeMany={updateSelectedMaskValues}
                  onCurvesChange={updateSelectedMaskCurves}
                  curveChannel={curveChannel}
                  setCurveChannel={setCurveChannel}
                />
              </div>
            ) : (
              <div className="mask-empty-state">Select or create a mask to edit local adjustments</div>
            )}
          </div>

          <div className="editor-group">
            <h4>Mask layers</h4>
            {masks.length === 0 ? (
              <div className="mask-empty-state">No persistent masks yet</div>
            ) : (
              <div className="mask-layer-list">
                {masks.map((mask) => {
                  const exposure = maskExposure(mask);
                  return (
                    <div
                      key={mask.id}
                      className={`mask-layer-row ${selectedMask?.id === mask.id ? 'active' : ''}`}
                      data-hidden={!mask.visible}
                    >
                      <button
                        type="button"
                        className="mask-layer-select"
                        onClick={() => setSelectedMaskId(mask.id)}
                      >
                        <strong>{mask.name}</strong>
                        <span className="mono">
                          {mask.source.replaceAll('_', ' ')} / {mask.mode} / {exposure > 0 ? '+' : ''}
                          {exposure.toFixed(2)} EV
                        </span>
                      </button>
                      <fieldset className="mask-layer-mode">
                        <legend className="mask-mode-legend">Combine mode for {mask.name}</legend>
                        {MASK_MODES.map((mode) => (
                          <button
                            key={mode.id}
                            type="button"
                            className={mask.mode === mode.id ? 'on' : ''}
                            onClick={(event) => {
                              event.stopPropagation();
                              setMaskModeForLayer(mask.id, mode.id);
                            }}
                          >
                            {mode.label}
                          </button>
                        ))}
                      </fieldset>
                      <div className="mask-layer-actions">
                        <button
                          type="button"
                          className="btn"
                          onClick={(event) => {
                            event.stopPropagation();
                            setSelectedMaskId(mask.id);
                          }}
                        >
                          Select
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={(event) => {
                            event.stopPropagation();
                            updateMask.mutate(
                              { mask_id: mask.id, visible: !mask.visible },
                              { onSuccess: () => refreshPreview() },
                            );
                          }}
                        >
                          {mask.visible ? 'Hide' : 'Show'}
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={(event) => {
                            event.stopPropagation();
                            setMaskExposure(mask.id, 0.35);
                          }}
                        >
                          +Light
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={(event) => {
                            event.stopPropagation();
                            setMaskExposure(mask.id, -0.35);
                          }}
                        >
                          -Dark
                        </button>
                        <button
                          type="button"
                          className="btn danger"
                          onClick={(event) => {
                            event.stopPropagation();
                            deleteMask.mutate(
                              { maskId: mask.id, photoId: photo.id },
                              { onSuccess: () => refreshPreview() },
                            );
                          }}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
            {activeMask && (
              <div className="mask-readout">
                <div>
                  <span className="mono">Latest AI mask</span>
                  <strong>{activeMask.prompt}</strong>
                </div>
                <div>
                  <span className="mono">Confidence</span>
                  <strong>{Math.round(activeMask.confidence * 100)}%</strong>
                </div>
              </div>
            )}
            {maskError && <div className="mask-error mono">{maskError}</div>}
          </div>
        </div>
      </div>
    </div>
  );
}

interface MaskAdjustmentPanelProps {
  values: DevelopValues;
  onChange: (key: keyof DevelopValues, value: number) => void;
  onChangeMany: (patch: Partial<DevelopValues>) => void;
  onCurvesChange: (next: DevelopValues['curves']) => void;
  curveChannel: CurveChannel;
  setCurveChannel: (channel: CurveChannel) => void;
}

function MaskAdjustmentPanel({
  values,
  onChange,
  onChangeMany,
  onCurvesChange,
  curveChannel,
  setCurveChannel,
}: MaskAdjustmentPanelProps) {
  const sectionReset = (label: string, patch: Partial<DevelopValues>) => (
    <button type="button" className="mask-section-reset mono" onClick={() => onChangeMany(patch)}>
      Reset {label}
    </button>
  );
  const groupHead = (label: string, patch?: Partial<DevelopValues>) => (
    <div className="mask-adjustment-head">
      <h5>{label}</h5>
      {patch ? sectionReset(label, patch) : null}
    </div>
  );

  return (
    <div className="mask-adjustment-stack">
      <div className="mask-adjustment-group">
        {groupHead('Light', { exp: 0, con: 0, hi: 0, sh: 0, whites: 0, blacks: 0 })}
        <Slider label="Exposure" value={values.exp} onChange={(v) => onChange('exp', v)} suffix=" EV" />
        <Slider label="Contrast" value={values.con} onChange={(v) => onChange('con', v)} />
        <Slider label="Highlights" value={values.hi} onChange={(v) => onChange('hi', v)} />
        <Slider label="Shadows" value={values.sh} onChange={(v) => onChange('sh', v)} />
        <Slider label="Whites" value={values.whites} onChange={(v) => onChange('whites', v)} />
        <Slider label="Blacks" value={values.blacks} onChange={(v) => onChange('blacks', v)} />
      </div>

      <div className="mask-adjustment-group">
        {groupHead('Color', { temp: 0, tint: 0, vib: 0, sat: 0 })}
        <Slider label="Temp" value={values.temp} onChange={(v) => onChange('temp', v)} suffix="K" />
        <Slider label="Tint" value={values.tint} onChange={(v) => onChange('tint', v)} />
        <Slider label="Vibrance" value={values.vib} onChange={(v) => onChange('vib', v)} />
        <Slider label="Saturation" value={values.sat} onChange={(v) => onChange('sat', v)} />
      </div>

      <div className="mask-adjustment-group">
        {groupHead('Detail', { clarity: 0, dehaze: 0 })}
        <Slider label="Clarity" value={values.clarity} onChange={(v) => onChange('clarity', v)} />
        <Slider label="Dehaze" value={values.dehaze} onChange={(v) => onChange('dehaze', v)} />
      </div>

      <div className="mask-adjustment-group">
        {groupHead('Curves')}
        <CurvesPanel
          value={values.curves}
          onChange={onCurvesChange}
          channel={curveChannel}
          setChannel={setCurveChannel}
        />
      </div>

      <div className="mask-adjustment-group">
        {groupHead('Lens Blur', {
          lensBlurAmount: 0,
          lensBlurFocusNear: 0,
          lensBlurFocusFar: 100,
          lensBlurBokehBoost: 0,
          lensBlurCatEye: 0,
        })}
        <Slider
          label="Amount"
          value={values.lensBlurAmount}
          onChange={(v) => onChange('lensBlurAmount', v)}
          min={0}
          max={100}
        />
        <Slider
          label="Focus near"
          value={values.lensBlurFocusNear}
          onChange={(v) => onChange('lensBlurFocusNear', v)}
          min={0}
          max={100}
          suffix="%"
        />
        <Slider
          label="Focus far"
          value={values.lensBlurFocusFar}
          onChange={(v) => onChange('lensBlurFocusFar', v)}
          min={0}
          max={100}
          suffix="%"
        />
        <Slider
          label="Bokeh boost"
          value={values.lensBlurBokehBoost}
          onChange={(v) => onChange('lensBlurBokehBoost', v)}
          min={0}
          max={100}
        />
        <Slider
          label="Cat eye"
          value={values.lensBlurCatEye}
          onChange={(v) => onChange('lensBlurCatEye', v)}
          min={0}
          max={100}
        />
      </div>

      <div className="mask-adjustment-group">
        {groupHead('Effects', { lensVignette: 0 })}
        <Slider label="Vignette" value={values.lensVignette} onChange={(v) => onChange('lensVignette', v)} />
      </div>
    </div>
  );
}

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
