/**
 * DevelopScreen — Phase 3 non-destructive RAW/JPEG editor. Slider drags
 * post through `develop_apply` to the Rust CPU pipeline (rayon; wgpu is a
 * follow-up); preview comes back as a base64 JPEG data URL. Save/Reset/
 * CopyEdits hit the `edits` table via the develop commands.
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Placeholder } from '../../primitives/Placeholder';
import { Seg } from '../../primitives/Seg';
import { Thumbnail } from '../../primitives/Thumbnail';
import { useDevelopUi } from '../../state/develop';
import {
  type DevelopOperations,
  useDevelopApply,
  useDevelopOpen,
  useDevelopPasteEdits,
  useDevelopReset,
  useDevelopSave,
  usePhotos,
} from '../../state/queries';
import {
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
  valuesToOperations,
} from './types';

export function DevelopScreen() {
  const { data: photos = [], isLoading } = usePhotos();
  const [focusedIdx, setFocusedIdx] = useState(0);
  const [tab, setTab] = useState<DevelopTab>('develop');
  const [values, setValues] = useState<DevelopValues>(() => defaultDevelopValues());
  const [preview, setPreview] = useState<string | null>(null);
  const [copiedOps, setCopiedOps] = useState<DevelopOperations | null>(null);
  const [promptText, setPromptText] = useState(
    'Lift shadows slightly, keep skin tones natural, subtle dehaze on sky.',
  );
  const [promptStrength, setPromptStrength] = useState(65);
  const [promptConstraints, setPromptConstraints] = useState<string[]>(['keep faces sharp', 'natural tones']);
  const valuesRef = useRef(values);
  const focusedPhotoIdRef = useRef<number | null>(null);
  const applySeqRef = useRef(0);
  const userEditedRef = useRef(false);
  const debounceTimer = useRef<number | null>(null);

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
  const sharedPreview = useDevelopUi((s) => s.preview);
  const sharedOperations = useDevelopUi((s) => s.operations);
  const operationSource = useDevelopUi((s) => s.operationSource);
  useEffect(() => {
    focusedPhotoIdRef.current = focusedPhotoId;
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
    setPreview(null);
  }, [focusedPhotoId, setSharedFocus, setSharedMask, setSharedOperations, setSharedPreview]);

  // Load the photo's current edit state + baseline preview ONCE per photo.
  // Re-fetching `opened` on every render would wipe unsaved slider
  // positions because the backend's `current_edit_id` only moves on
  // explicit Save. Keying off `focusedPhotoId` ensures we only seed
  // slider state when the user opens a different photo.
  const { data: opened } = useDevelopOpen(focusedPhotoId);
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

  const applyMut = useDevelopApply();
  const saveMut = useDevelopSave();
  const resetMut = useDevelopReset();
  const pasteMut = useDevelopPasteEdits();

  // Debounce slider input → backend render. 80 ms feels responsive; the
  // rayon pipeline at 1280 long-edge runs ~30–60 ms on i5.
  useEffect(() => {
    return () => {
      if (debounceTimer.current !== null) window.clearTimeout(debounceTimer.current);
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
      scheduleApply(ops);
    },
    [scheduleApply, setSharedOperations],
  );

  const updateCurves = useCallback(
    (curves: DevelopValues['curves']) => {
      userEditedRef.current = true;
      const next: DevelopValues = { ...valuesRef.current, curves };
      valuesRef.current = next;
      setValues(next);
      const ops = valuesToOperations(next);
      setSharedOperations(ops, 'screen');
      scheduleApply(ops);
    },
    [scheduleApply, setSharedOperations],
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
    scheduleApply(ops);
  }, [scheduleApply, setSharedOperations]);

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
        scheduleApply(ops);
      },
    });
  }, [focusedPhotoId, resetMut, scheduleApply, setSharedOperations]);

  const currentOperations = useCallback(() => {
    return useDevelopUi.getState().operations ?? valuesToOperations(valuesRef.current);
  }, []);

  const saveEdits = useCallback(() => {
    if (focusedPhotoId == null) return;
    saveMut.mutate({ photoId: focusedPhotoId, operations: currentOperations() });
  }, [currentOperations, focusedPhotoId, saveMut]);

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
          scheduleApply(copiedOps);
        },
        onError: (e) => warn('develop_paste_edits failed', e),
      },
    );
  }, [copiedOps, focusedPhotoId, pasteMut, scheduleApply, setSharedOperations]);

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

  return (
    <div className="canvas">
      <DevelopToolbar
        photo={photo}
        megapixels={megapixels}
        tab={tab}
        setTab={setTab}
        canPrev={focusedIdx > 0}
        canNext={focusedIdx < photos.length - 1}
        onPrev={() => setFocusedIdx((i) => Math.max(0, i - 1))}
        onNext={() => setFocusedIdx((i) => Math.min(photos.length - 1, i + 1))}
        onSave={saveEdits}
        onCopy={copyEdits}
        saving={saveMut.isPending}
      />

      {tab === 'develop' ? (
        <DevelopStageSplit
          photo={photo}
          photos={photos}
          focusedIdx={focusedIdx}
          setFocusedIdx={setFocusedIdx}
          stageAspect={stageAspect}
          values={values}
          onValueChange={updateValue}
          onCurvesChange={updateCurves}
          onAutoLight={autoLight}
          onReset={resetEdits}
          onCopy={copyEdits}
          onPaste={pasteEdits}
          canPaste={!!copiedOps && !pasteMut.isPending}
          preview={preview}
        />
      ) : tab === 'mask' ? (
        <MaskStage photo={photo} stageAspect={stageAspect} preview={preview} />
      ) : (
        <PromptStage
          photo={photo}
          promptText={promptText}
          setPromptText={setPromptText}
          promptStrength={promptStrength}
          setPromptStrength={setPromptStrength}
          constraints={promptConstraints}
          setConstraints={setPromptConstraints}
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
  canPrev: boolean;
  canNext: boolean;
  onPrev: () => void;
  onNext: () => void;
  onSave: () => void;
  onCopy: () => void;
  saving: boolean;
}

function DevelopToolbar({
  photo,
  megapixels,
  tab,
  setTab,
  canPrev,
  canNext,
  onPrev,
  onNext,
  onSave,
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
        className="btn phase-gated"
        disabled
        aria-disabled="true"
        title="Coming in Phase 3 · crop + transform"
      >
        <Icon name="crop" size={13} />
      </button>
      <button
        type="button"
        className="btn phase-gated"
        disabled
        aria-disabled="true"
        title="Coming in Phase 3 · before/after compare"
      >
        <Icon name="eye" size={13} /> Before/After
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
  onValueChange: (key: keyof DevelopValues, value: number) => void;
  onCurvesChange: (curves: DevelopValues['curves']) => void;
  onAutoLight: () => void;
  onReset: () => void;
  onCopy: () => void;
  onPaste: () => void;
  canPaste: boolean;
  preview: string | null;
}

function DevelopStageSplit({
  photo,
  photos,
  focusedIdx,
  setFocusedIdx,
  stageAspect,
  values,
  onValueChange,
  onCurvesChange,
  onAutoLight,
  onReset,
  onCopy,
  onPaste,
  canPaste,
  preview,
}: DevelopStageSplitProps) {
  return (
    <div className="editor-stage">
      <div className="editor-main">
        <div className="editor-canvas">
          <div
            style={{
              width: 'min(100%, 1100px)',
              aspectRatio: stageAspect,
              maxHeight: '100%',
              position: 'relative',
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
            <div className="editor-histogram" aria-hidden="true">
              <svg viewBox="0 0 100 40" preserveAspectRatio="none">
                <path
                  d="M0,40 L5,30 L12,20 L20,14 L30,8 L42,12 L55,18 L65,22 L72,16 L80,24 L88,30 L95,36 L100,40 Z"
                  fill="rgba(255,255,255,0.25)"
                />
                <path
                  d="M0,40 L6,34 L14,26 L22,22 L33,14 L45,10 L58,14 L68,18 L78,22 L86,28 L94,34 L100,40 Z"
                  fill="color-mix(in oklch, var(--accent) 40%, transparent)"
                />
              </svg>
            </div>
          </div>
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
                sizePx={160}
                photo={{ hue: (p.id * 31) % 360, filename: p.filename, id: String(p.id) }}
              />
            </button>
          ))}
        </div>
      </div>
      <EditorInspector
        photo={photo}
        values={values}
        onChange={onValueChange}
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
  { id: 'subject', label: 'Subject', prompt: 'the main subject', icon: 'faces' as const },
  { id: 'sky', label: 'Sky', prompt: 'the sky', icon: 'cloud' as const },
  { id: 'person', label: 'Person', prompt: 'the person', icon: 'faces' as const },
  { id: 'face', label: 'Face', prompt: 'the face', icon: 'brush' as const },
  { id: 'foreground', label: 'Foreground', prompt: 'the foreground', icon: 'layers' as const },
  { id: 'background', label: 'Background', prompt: 'the background', icon: 'grid' as const },
];

interface MaskStageProps {
  photo: PhotoRow;
  stageAspect: string;
  preview: string | null;
}

function MaskStage({ photo, stageAspect, preview }: MaskStageProps) {
  const [status, setStatus] = useState<SidecarStatus | null>(null);
  const [masking, setMasking] = useState(false);
  const [maskError, setMaskError] = useState<string | null>(null);
  const [customPrompt, setCustomPrompt] = useState('the main subject');
  const [activePresetId, setActivePresetId] = useState<string | null>(null);
  const activeMask = useDevelopUi((s) => s.activeMask);
  const setActiveMask = useDevelopUi((s) => s.setActiveMask);

  useEffect(() => {
    let cancelled = false;
    promptSidecarPing()
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch((e) => {
        if (!cancelled) {
          warn('mask sidecar ping failed', e);
          setStatus({ configured: false, url: null, reachable: false, model: null, error: String(e) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const canMask = !!status?.configured && status.reachable && !masking;
  const statusLabel = !status
    ? 'Checking sidecar'
    : status.configured && status.reachable
      ? `Connected${status.model ? ` · ${status.model}` : ''}`
      : status.configured
        ? 'Unreachable'
        : 'No sidecar configured';

  const runMask = async (prompt: string, presetId: string | null) => {
    const trimmed = prompt.trim();
    if (!trimmed || !canMask) return;
    const photoId = photo.id;
    setActivePresetId(presetId);
    setMasking(true);
    setMaskError(null);
    try {
      const result = await maskFromPrompt({ photo_id: photoId, prompt: trimmed });
      if (useDevelopUi.getState().focusedPhotoId !== photoId) return;
      setActiveMask({
        prompt: trimmed,
        maskB64: result.mask_b64,
        confidence: result.confidence,
        latencyMs: result.latency_ms,
        createdAt: new Date().toISOString(),
      });
    } catch (e) {
      setMaskError(String(e));
    } finally {
      setMasking(false);
    }
  };

  const maskSrc = activeMask ? `data:image/png;base64,${activeMask.maskB64}` : null;

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
                sizePx={1280}
                photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
              />
            )}
            {maskSrc && <img src={maskSrc} alt="" aria-hidden="true" className="mask-overlay" />}
            <div className="mask-status">
              <Chip variant={status?.reachable ? 'solid' : undefined}>Mask · {statusLabel}</Chip>
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
          <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-dim)' }}>
            Intelligent masking
          </div>
          <button
            type="button"
            className="btn"
            onClick={() => setActiveMask(null)}
            disabled={!activeMask}
            aria-disabled={!activeMask}
            style={{ padding: '5px 9px', fontSize: 11.5 }}
          >
            Clear
          </button>
        </div>
        <div className="editor-ibody">
          <div className="editor-group">
            <h4>Quick masks</h4>
            <div className="mask-preset-grid">
              {MASK_PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  className={activePresetId === preset.id ? 'btn primary' : 'btn'}
                  disabled={!canMask}
                  aria-disabled={!canMask}
                  onClick={() => runMask(preset.prompt, preset.id)}
                  title={canMask ? preset.prompt : 'Needs a reachable prompt sidecar'}
                >
                  <Icon name={preset.icon} size={12} /> {preset.label}
                </button>
              ))}
            </div>
          </div>

          <div className="editor-group">
            <h4>Prompt</h4>
            <div className="mask-custom-row">
              <input
                type="text"
                value={customPrompt}
                onChange={(e) => setCustomPrompt(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    runMask(customPrompt, null);
                  }
                }}
                aria-label="Custom mask prompt"
              />
              <button
                type="button"
                className={canMask ? 'btn primary' : 'btn primary phase-gated'}
                disabled={!canMask}
                aria-disabled={!canMask}
                onClick={() => runMask(customPrompt, null)}
              >
                <Icon name="brush" size={12} /> {masking ? 'Masking...' : 'Mask'}
              </button>
            </div>
          </div>

          <div className="editor-group">
            <h4>Selection</h4>
            {activeMask ? (
              <div className="mask-readout">
                <div>
                  <span className="mono">Prompt</span>
                  <strong>{activeMask.prompt}</strong>
                </div>
                <div>
                  <span className="mono">Confidence</span>
                  <strong>{Math.round(activeMask.confidence * 100)}%</strong>
                </div>
                <div>
                  <span className="mono">Latency</span>
                  <strong>{activeMask.latencyMs} ms</strong>
                </div>
              </div>
            ) : (
              <div className="mask-empty-state">No active mask</div>
            )}
            {maskError && <div className="mask-error mono">{maskError}</div>}
          </div>
        </div>
      </div>
    </div>
  );
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
          <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>
            <Thumbnail
              photoId={photo.id}
              sizePx={1280}
              photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
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
