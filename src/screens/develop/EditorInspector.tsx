import { useState } from 'react';
import { Icon } from '../../primitives/Icon';
import { Slider } from '../../primitives/Slider';
import type { DevelopCurves, PhotoRow } from '../../tauri/invoke';
import { type CurveChannel, CurvesPanel } from './CurvesPanel';
import type { DevelopValues } from './types';

export interface EditorInspectorProps {
  photo: PhotoRow;
  values: DevelopValues;
  onChange: (key: keyof DevelopValues, value: number) => void;
  onCurvesChange: (next: DevelopCurves) => void;
  onAutoLight: () => void;
  onReset: () => void;
  onCopy: () => void;
  onPaste: () => void;
  canPaste: boolean;
}

export function EditorInspector({
  photo,
  values,
  onChange,
  onCurvesChange,
  onAutoLight,
  onReset,
  onCopy,
  onPaste,
  canPaste,
}: EditorInspectorProps) {
  const [curveChannel, setCurveChannel] = useState<CurveChannel>('rgb');
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
            onClick={onCopy}
            style={{ color: 'var(--fg-mute)', padding: 4 }}
          >
            <Icon name="layers" size={13} />
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
              title="Auto light"
            >
              <Icon name="sparkles" size={12} /> Auto light
            </button>
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
          <CurvesPanel
            value={values.curves}
            onChange={onCurvesChange}
            channel={curveChannel}
            setChannel={setCurveChannel}
          />
        </div>

        <div className="editor-group">
          <h4>Lens Blur</h4>
          <Slider
            label="Amount"
            value={values.lensBlurAmount}
            onChange={(v) => onChange('lensBlurAmount', v)}
          />
          <Slider
            label="Focus near"
            value={values.lensBlurFocusNear}
            onChange={(v) => onChange('lensBlurFocusNear', v)}
            suffix="%"
          />
          <Slider
            label="Focus far"
            value={values.lensBlurFocusFar}
            onChange={(v) => onChange('lensBlurFocusFar', v)}
            suffix="%"
          />
          <Slider
            label="Bokeh boost"
            value={values.lensBlurBokehBoost}
            onChange={(v) => onChange('lensBlurBokehBoost', v)}
          />
          <Slider
            label="Cat eye"
            value={values.lensBlurCatEye}
            onChange={(v) => onChange('lensBlurCatEye', v)}
          />
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Depth artifacts are tracked through AI edit status; refine focus with mask layers.
          </div>
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
          <h4>Crop · Transform</h4>
          <Slider label="Crop X" value={values.cropX} onChange={(v) => onChange('cropX', v)} suffix="%" />
          <Slider label="Crop Y" value={values.cropY} onChange={(v) => onChange('cropY', v)} suffix="%" />
          <Slider label="Crop W" value={values.cropW} onChange={(v) => onChange('cropW', v)} suffix="%" />
          <Slider label="Crop H" value={values.cropH} onChange={(v) => onChange('cropH', v)} suffix="%" />
          <Slider
            label="Rotate"
            value={values.rotation}
            onChange={(v) => onChange('rotation', v)}
            suffix="°"
          />
          <Slider
            label="Straighten"
            value={values.straighten}
            onChange={(v) => onChange('straighten', v)}
            suffix="°"
          />
          <Slider label="Horizontal" value={values.transformH} onChange={(v) => onChange('transformH', v)} />
          <Slider label="Vertical" value={values.transformV} onChange={(v) => onChange('transformV', v)} />
        </div>

        <div className="editor-group">
          <h4>Lens · Heal</h4>
          <Slider
            label="Distortion"
            value={values.lensDistortion}
            onChange={(v) => onChange('lensDistortion', v)}
          />
          <Slider
            label="Vignette"
            value={values.lensVignette}
            onChange={(v) => onChange('lensVignette', v)}
          />
          <Slider
            label="Chromatic aberration"
            value={values.chromaticAberration}
            onChange={(v) => onChange('chromaticAberration', v)}
          />
          <Slider
            label="Spot heals"
            value={values.spotHealCount}
            onChange={(v) => onChange('spotHealCount', v)}
          />
        </div>

        <div className="editor-group">
          <h4>Copy · Paste · Sync</h4>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
            <button
              type="button"
              className="btn"
              onClick={onCopy}
              title="Copy current edits"
              style={{ padding: '7px', fontSize: 11.5, justifyContent: 'center' }}
            >
              Copy{' '}
              <span className="mono" style={{ color: 'var(--fg-mute)', marginLeft: 4 }}>
                ⌘C
              </span>
            </button>
            <button
              type="button"
              className="btn"
              disabled={!canPaste}
              aria-disabled={!canPaste}
              onClick={onPaste}
              title={canPaste ? 'Paste copied edits onto this photo' : 'Copy edits from a photo first'}
              style={{ padding: '7px', fontSize: 11.5, justifyContent: 'center' }}
            >
              Paste{' '}
              <span className="mono" style={{ color: 'var(--fg-mute)', marginLeft: 4 }}>
                ⌘V
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
