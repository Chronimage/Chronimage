/**
 * DevelopScreen — Phase 3 RAW editor stub. Unified surface with Develop / Mask /
 * Prompt tabs, curves + sliders + presets + AI-prompt box. All sliders are
 * fully interactive locally (no RAW decode yet); Export / Save / Copy actions
 * are phase-gated until the wgpu develop pipeline lands (Phase 3).
 */

import { useCallback, useState } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Placeholder } from '../../primitives/Placeholder';
import { Seg } from '../../primitives/Seg';
import { Thumbnail } from '../../primitives/Thumbnail';
import { usePhotos } from '../../state/queries';
import type { PhotoRow } from '../../tauri/invoke';
import { EditorInspector } from './EditorInspector';
import { DEFAULT_DEVELOP_VALUES, type DevelopTab, type DevelopValues } from './types';

export function DevelopScreen() {
  const { data: photos = [], isLoading } = usePhotos();
  const [focusedIdx, setFocusedIdx] = useState(0);
  const [tab, setTab] = useState<DevelopTab>('develop');
  const [values, setValues] = useState<DevelopValues>(DEFAULT_DEVELOP_VALUES);
  const [promptText, setPromptText] = useState(
    'Lift shadows slightly, keep skin tones natural, subtle dehaze on sky.',
  );
  const [promptStrength, setPromptStrength] = useState(65);

  const photo = photos[focusedIdx] ?? null;

  const updateValue = useCallback((key: keyof DevelopValues, value: number) => {
    setValues((v) => ({ ...v, [key]: value }));
  }, []);

  const autoLight = useCallback(() => {
    // Local stub: compute sane defaults based on the photo's sharpness hint.
    setValues({
      exp: 12,
      con: 8,
      hi: -24,
      sh: 32,
      temp: 6,
      tint: -2,
      vib: 14,
      sat: 4,
      clarity: 8,
      dehaze: 10,
    });
  }, []);

  const resetEdits = useCallback(() => setValues(DEFAULT_DEVELOP_VALUES), []);

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
      <DevelopToolbar photo={photo} megapixels={megapixels} tab={tab} setTab={setTab} />

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
        />
      ) : (
        <PromptStage
          photo={photo}
          promptText={promptText}
          setPromptText={setPromptText}
          promptStrength={promptStrength}
          setPromptStrength={setPromptStrength}
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
}

function DevelopToolbar({ photo, megapixels, tab, setTab }: DevelopToolbarProps) {
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
      <button
        type="button"
        className="btn phase-gated"
        disabled
        aria-disabled="true"
        title="Coming in Phase 3 · edits clipboard"
      >
        <Icon name="layers" size={13} /> Copy edits
      </button>
      <button
        type="button"
        className="btn primary phase-gated"
        disabled
        aria-disabled="true"
        title="Coming in Phase 2 · Export sheet"
      >
        <Icon name="download" size={13} /> Export
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
            <Thumbnail
              photoId={photo.id}
              sizePx={1280}
              photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
            />
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
}

function PromptStage({
  photo,
  promptText,
  setPromptText,
  promptStrength,
  setPromptStrength,
}: PromptStageProps) {
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
            AFTER · prompt stub (no edits applied)
          </div>
          <div style={{ flex: 1, minHeight: 0, position: 'relative' }}>
            <Placeholder
              photo={{ hue: 220, filename: photo.filename, id: String(photo.id) }}
              showLabel={false}
            />
            <div style={{ position: 'absolute', top: 10, right: 10 }}>
              <Chip variant="solid">AI · Phase 3</Chip>
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
              title="Coming in Phase 3 · masked prompt edits"
              style={{ padding: '5px 9px', fontSize: 11.5 }}
            >
              <Icon name="brush" size={12} /> Mask
            </button>
            <Chip onClose={() => {}}>keep faces sharp</Chip>
            <Chip onClose={() => {}}>natural tones</Chip>
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
              title="Coming in Phase 3 · AI generate"
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
