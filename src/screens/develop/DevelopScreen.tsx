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
  useDevelopCopyEdits,
  useDevelopOpen,
  useDevelopReset,
  useDevelopSave,
  usePhotos,
} from '../../state/queries';
import type { PhotoRow } from '../../tauri/invoke';
import { warn } from '../../util/log';
import { EditorInspector } from './EditorInspector';
import {
  DEFAULT_DEVELOP_VALUES,
  type DevelopTab,
  type DevelopValues,
  operationsToValues,
  valuesToOperations,
} from './types';

export function DevelopScreen() {
  const { data: photos = [], isLoading } = usePhotos();
  const [focusedIdx, setFocusedIdx] = useState(0);
  const [tab, setTab] = useState<DevelopTab>('develop');
  const [values, setValues] = useState<DevelopValues>(DEFAULT_DEVELOP_VALUES);
  const [preview, setPreview] = useState<string | null>(null);
  const [promptText, setPromptText] = useState(
    'Lift shadows slightly, keep skin tones natural, subtle dehaze on sky.',
  );
  const [promptStrength, setPromptStrength] = useState(65);
  const [promptConstraints, setPromptConstraints] = useState<string[]>(['keep faces sharp', 'natural tones']);

  const photo = photos[focusedIdx] ?? null;
  const focusedPhotoId = photo?.id ?? null;

  // Publish focus + preview through the tiny Zustand store so the
  // DevelopSidePanel (a sibling in the app shell) can drive
  // `develop_preset_apply` against the right photo and push the
  // returned preview back into the stage.
  const setSharedFocus = useDevelopUi((s) => s.setFocusedPhotoId);
  const setSharedPreview = useDevelopUi((s) => s.setPreview);
  const sharedPreview = useDevelopUi((s) => s.preview);
  useEffect(() => {
    setSharedFocus(focusedPhotoId);
  }, [focusedPhotoId, setSharedFocus]);

  // Load the photo's current edit state + baseline preview ONCE per photo.
  // Re-fetching `opened` on every render would wipe unsaved slider
  // positions because the backend's `current_edit_id` only moves on
  // explicit Save. Keying off `focusedPhotoId` ensures we only seed
  // slider state when the user opens a different photo.
  const { data: opened } = useDevelopOpen(focusedPhotoId);
  const seededForPhotoRef = useRef<number | null>(null);
  useEffect(() => {
    if (!opened) return;
    if (seededForPhotoRef.current === opened.photo_id) return;
    seededForPhotoRef.current = opened.photo_id;
    setValues(operationsToValues(opened.operations));
    setPreview(opened.preview_data_url);
    setSharedPreview(opened.preview_data_url);
  }, [opened, setSharedPreview]);

  // If the sidepanel pushed a new preview (preset apply), surface it.
  useEffect(() => {
    if (sharedPreview && sharedPreview !== preview) setPreview(sharedPreview);
  }, [sharedPreview, preview]);

  const applyMut = useDevelopApply();
  const saveMut = useDevelopSave();
  const resetMut = useDevelopReset();
  const copyMut = useDevelopCopyEdits();

  // Debounce slider input → backend render. 80 ms feels responsive; the
  // rayon pipeline at 1280 long-edge runs ~30–60 ms on i5.
  const debounceTimer = useRef<number | null>(null);
  const scheduleApply = useCallback(
    (ops: DevelopOperations) => {
      if (focusedPhotoId == null) return;
      if (debounceTimer.current !== null) {
        window.clearTimeout(debounceTimer.current);
      }
      debounceTimer.current = window.setTimeout(() => {
        applyMut.mutate(
          { photoId: focusedPhotoId, operations: ops },
          {
            onSuccess: (r) => setPreview(r.preview_data_url),
            onError: (e) => warn('develop_apply failed', e),
          },
        );
      }, 80);
    },
    [applyMut, focusedPhotoId],
  );

  // Keep a ref on the latest values so `updateValue` can compute the
  // next state without closing over a stale `values` snapshot — and so
  // the side-effect (scheduleApply) happens outside `setValues`'s
  // updater function (an anti-pattern that fires twice under React
  // Strict Mode, clearing our debounce timer on the second run).
  // The ref is updated *synchronously* inside updateValue so two slider
  // drags dispatched in the same event tick don't both compute `next`
  // from the same stale snapshot.
  const valuesRef = useRef(values);
  // Sync the ref whenever React commits a new `values` from elsewhere
  // (preset apply / reset / paste / opened seed). Slider drags update
  // the ref synchronously below so they don't depend on this effect.
  useEffect(() => {
    valuesRef.current = values;
  }, [values]);

  const updateValue = useCallback(
    (key: keyof DevelopValues, value: number) => {
      const next = { ...valuesRef.current, [key]: value };
      valuesRef.current = next;
      setValues(next);
      scheduleApply(valuesToOperations(next));
    },
    [scheduleApply],
  );

  const autoLight = useCallback(() => {
    const next: DevelopValues = {
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
    setValues(next);
    scheduleApply(valuesToOperations(next));
  }, [scheduleApply]);

  const resetEdits = useCallback(() => {
    if (focusedPhotoId == null) return;
    resetMut.mutate(focusedPhotoId, {
      onSuccess: () => {
        setValues(DEFAULT_DEVELOP_VALUES);
        scheduleApply(valuesToOperations(DEFAULT_DEVELOP_VALUES));
      },
    });
  }, [focusedPhotoId, resetMut, scheduleApply]);

  const saveEdits = useCallback(() => {
    if (focusedPhotoId == null) return;
    saveMut.mutate({ photoId: focusedPhotoId, operations: valuesToOperations(values) });
  }, [focusedPhotoId, saveMut, values]);

  const copyEdits = useCallback(() => {
    if (focusedPhotoId == null) return;
    copyMut.mutate(focusedPhotoId, {
      onSuccess: (ops) => {
        setValues(operationsToValues(ops));
      },
    });
  }, [focusedPhotoId, copyMut]);

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
        onSave={saveEdits}
        onCopy={copyEdits}
        saving={saveMut.isPending}
      />

      {tab !== 'prompt' ? (
        <DevelopStageSplit
          photo={photo}
          photos={photos}
          focusedIdx={focusedIdx}
          setFocusedIdx={setFocusedIdx}
          stageAspect={stageAspect}
          values={values}
          onValueChange={updateValue}
          onAutoLight={autoLight}
          onReset={resetEdits}
          preview={preview}
        />
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
  onSave: () => void;
  onCopy: () => void;
  saving: boolean;
}

function DevelopToolbar({ photo, megapixels, tab, setTab, onSave, onCopy, saving }: DevelopToolbarProps) {
  return (
    <div className="toolbar">
      <button type="button" className="btn" style={{ padding: '5px 8px' }} title="Previous photo">
        <Icon name="chevL" size={13} />
      </button>
      <button type="button" className="btn" style={{ padding: '5px 8px' }} title="Next photo">
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
  onAutoLight: () => void;
  onReset: () => void;
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
  onAutoLight,
  onReset,
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
              <Placeholder
                photo={{ hue: (p.id * 31) % 360, filename: p.filename, id: String(p.id) }}
                showLabel={false}
              />
            </button>
          ))}
        </div>
      </div>
      <EditorInspector
        photo={photo}
        values={values}
        onChange={onValueChange}
        onAutoLight={onAutoLight}
        onReset={onReset}
      />
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
  const addConstraint = () => {
    const v = newConstraint.trim();
    if (!v || constraints.includes(v)) return;
    setConstraints([...constraints, v]);
    setNewConstraint('');
  };
  const removeConstraint = (s: string) => {
    setConstraints(constraints.filter((c) => c !== s));
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
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--accent)', letterSpacing: '0.08em' }}>
            AFTER · awaiting Flux/SDXL sidecar (Phase 4 week 3+)
          </div>
          <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>
            <Placeholder
              photo={{ hue: 220, filename: photo.filename, id: String(photo.id) }}
              showLabel={false}
            />
            <div style={{ position: 'absolute', top: 10, right: 10 }}>
              <Chip variant="solid">AI · Flux/SDXL pending</Chip>
            </div>
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
            aria-label="Prompt describing the edit"
            style={{ minHeight: 48 }}
          />
          <div className="prompt-row">
            <button
              type="button"
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Masked prompt edits need SAM2 — Phase 4 week 3+"
              style={{ padding: '5px 9px', fontSize: 11.5 }}
            >
              <Icon name="brush" size={12} /> Mask
            </button>
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
              className="btn primary phase-gated"
              disabled
              aria-disabled="true"
              title="Needs Flux/SDXL sidecar — Phase 4 week 3+"
              style={{ padding: '6px 12px', fontSize: 12 }}
            >
              <Icon name="sparkles" size={12} /> Generate <span className="kbd">⌘↵</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
