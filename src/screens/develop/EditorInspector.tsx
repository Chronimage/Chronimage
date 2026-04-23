import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Slider } from '../../primitives/Slider';
import type { PhotoRow } from '../../tauri/invoke';
import { CurvesPanel } from './CurvesPanel';
import type { DevelopValues } from './types';

export interface EditorInspectorProps {
  photo: PhotoRow;
  values: DevelopValues;
  onChange: (key: keyof DevelopValues, value: number) => void;
  onAutoLight: () => void;
  onReset: () => void;
}

export function EditorInspector({ photo, values, onChange, onAutoLight, onReset }: EditorInspectorProps) {
  const megapixels =
    photo.width && photo.height ? ((photo.width * photo.height) / 1_000_000).toFixed(0) : '—';
  const cameraLabel = [photo.camera_make, photo.camera_model].filter(Boolean).join(' ') || 'Camera unknown';

  return (
    <div className="editor-inspector">
      <div className="editor-ihead">
        <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-dim)' }}>
          {photo.filename}
          {photo.is_raw && (
            <>
              {' · '}
              <span style={{ color: 'var(--accent)' }}>{(photo.raw_format || 'RAW').toUpperCase()}</span>
            </>
          )}
          {' · '}
          {megapixels}MP · {cameraLabel}
        </div>
        <div style={{ display: 'flex', gap: 4 }}>
          <button
            type="button"
            title="Copy edits"
            className="phase-gated"
            disabled
            aria-disabled="true"
            style={{ color: 'var(--fg-mute)', padding: 4 }}
          >
            <Icon name="layers" size={13} />
          </button>
          <button
            type="button"
            title="History"
            className="phase-gated"
            disabled
            aria-disabled="true"
            style={{ color: 'var(--fg-mute)', padding: 4 }}
          >
            <Icon name="history" size={13} />
          </button>
          <button
            type="button"
            title="Reset edits"
            onClick={onReset}
            style={{ color: 'var(--fg-mute)', padding: 4, fontSize: 11 }}
            className="mono"
          >
            Reset
          </button>
        </div>
      </div>

      <div className="editor-ibody">
        <div className="editor-group">
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 8,
            }}
          >
            <h4>Auto</h4>
            <button
              type="button"
              className="btn primary"
              onClick={onAutoLight}
              style={{ padding: '5px 10px', fontSize: 11.5 }}
              title="Auto light (⌘A) — local stub, no edits applied"
            >
              <Icon name="sparkles" size={12} /> Auto light
            </button>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 6 }}>
            {(['Neutral', 'Vivid', 'Match batch'] as const).map((m) => (
              <button
                key={m}
                type="button"
                className="btn phase-gated"
                disabled
                aria-disabled="true"
                title="Coming in Phase 3 · RAW engine"
                style={{ padding: '6px', fontSize: 11, justifyContent: 'center' }}
              >
                {m}
              </button>
            ))}
          </div>
        </div>

        <div className="editor-group">
          <h4>Light</h4>
          <Slider label="Exposure" value={values.exp} onChange={(v) => onChange('exp', v)} suffix=" EV" />
          <Slider label="Contrast" value={values.con} onChange={(v) => onChange('con', v)} />
          <Slider label="Highlights" value={values.hi} onChange={(v) => onChange('hi', v)} />
          <Slider label="Shadows" value={values.sh} onChange={(v) => onChange('sh', v)} />
        </div>

        <div className="editor-group">
          <h4>Curves</h4>
          <CurvesPanel />
        </div>

        <div className="editor-group">
          <h4>Color</h4>
          <Slider label="Temp" value={values.temp} onChange={(v) => onChange('temp', v)} suffix="K" />
          <Slider label="Tint" value={values.tint} onChange={(v) => onChange('tint', v)} />
          <Slider label="Vibrance" value={values.vib} onChange={(v) => onChange('vib', v)} />
          <Slider label="Saturation" value={values.sat} onChange={(v) => onChange('sat', v)} />
        </div>

        <div className="editor-group">
          <h4>Detail</h4>
          <Slider label="Clarity" value={values.clarity} onChange={(v) => onChange('clarity', v)} />
          <Slider label="Dehaze" value={values.dehaze} onChange={(v) => onChange('dehaze', v)} />
        </div>

        <div className="editor-group">
          <h4>Copy · Paste · Sync</h4>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
            <button
              type="button"
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 3 · edits clipboard"
              style={{ padding: '7px', fontSize: 11.5, justifyContent: 'center' }}
            >
              Copy{' '}
              <span className="mono" style={{ color: 'var(--fg-mute)', marginLeft: 4 }}>
                ⌘C
              </span>
            </button>
            <button
              type="button"
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 3 · edits clipboard"
              style={{ padding: '7px', fontSize: 11.5, justifyContent: 'center' }}
            >
              Paste{' '}
              <span className="mono" style={{ color: 'var(--fg-mute)', marginLeft: 4 }}>
                ⌘V
              </span>
            </button>
            <button
              type="button"
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 3 · sync across selection"
              style={{ padding: '7px', fontSize: 11.5, justifyContent: 'center', gridColumn: 'span 2' }}
            >
              <Icon name="layers" size={12} /> Sync edits to selection
            </button>
          </div>
        </div>

        <div className="editor-group">
          <h4>Export &amp; Archive</h4>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <button
              type="button"
              className="btn primary phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 2 · Export sheet"
              style={{ justifyContent: 'center', fontSize: 12.5 }}
            >
              <Icon name="download" size={13} /> Export JPG · keep original
            </button>
            <button
              type="button"
              className="btn phase-gated"
              disabled
              aria-disabled="true"
              title="Coming in Phase 2 · Export + archive"
              style={{ justifyContent: 'center', fontSize: 12, padding: '7px' }}
            >
              <Icon name="export" size={12} /> Export + archive original
            </button>
            <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', padding: '4px 2px' }}>
              <Chip>non-destructive history</Chip>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
