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
  onChangeMany: (patch: Partial<DevelopValues>) => void;
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
  onChangeMany,
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
  const sectionReset = (label: string, patch: Partial<DevelopValues>) => (
    <button
      type="button"
      className="mono"
      onClick={() => onChangeMany(patch)}
      style={{
        padding: '2px 6px',
        borderRadius: 5,
        fontSize: 10.5,
        background: 'transparent',
        color: 'var(--fg-dim)',
        border: '1px solid var(--stroke)',
      }}
      title={`Reset ${label}`}
    >
      reset
    </button>
  );
  const groupHead = (label: string, patch?: Partial<DevelopValues>) => (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
      <h4>{label}</h4>
      {patch ? sectionReset(label, patch) : null}
    </div>
  );

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
          {groupHead('Light', { exp: 0, con: 0, hi: 0, sh: 0, whites: 0, blacks: 0 })}
          <Slider label="Exposure" value={values.exp} onChange={(v) => onChange('exp', v)} suffix=" EV" />
          <Slider label="Contrast" value={values.con} onChange={(v) => onChange('con', v)} />
          <Slider label="Highlights" value={values.hi} onChange={(v) => onChange('hi', v)} />
          <Slider label="Shadows" value={values.sh} onChange={(v) => onChange('sh', v)} />
          <Slider label="Whites" value={values.whites} onChange={(v) => onChange('whites', v)} />
          <Slider label="Blacks" value={values.blacks} onChange={(v) => onChange('blacks', v)} />
        </div>

        <div className="editor-group">
          {groupHead('Curves')}
          <CurvesPanel
            value={values.curves}
            onChange={onCurvesChange}
            channel={curveChannel}
            setChannel={setCurveChannel}
          />
        </div>

        <div className="editor-group">
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
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Depth artifacts are tracked through AI edit status; refine focus with mask layers.
          </div>
        </div>

        <div className="editor-group">
          {groupHead('Color', { temp: 0, tint: 0, vib: 0, sat: 0 })}
          <Slider label="Temp" value={values.temp} onChange={(v) => onChange('temp', v)} suffix="K" />
          <Slider label="Tint" value={values.tint} onChange={(v) => onChange('tint', v)} />
          <Slider label="Vibrance" value={values.vib} onChange={(v) => onChange('vib', v)} />
          <Slider label="Saturation" value={values.sat} onChange={(v) => onChange('sat', v)} />
        </div>

        <div className="editor-group">
          {groupHead('Detail', { clarity: 0, dehaze: 0 })}
          <Slider label="Clarity" value={values.clarity} onChange={(v) => onChange('clarity', v)} />
          <Slider label="Dehaze" value={values.dehaze} onChange={(v) => onChange('dehaze', v)} />
        </div>

        <div className="editor-group">
          {groupHead('Crop', { cropX: 0, cropY: 0, cropW: 100, cropH: 100 })}
          <div className="crop-aspect-row">
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
                onClick={() => onChangeMany(patch as Partial<DevelopValues>)}
                style={{ padding: '4px 8px', fontSize: 11 }}
              >
                {label as string}
              </button>
            ))}
          </div>
          <Slider
            label="Crop X"
            value={values.cropX}
            onChange={(v) => onChange('cropX', v)}
            min={0}
            max={95}
            suffix="%"
          />
          <Slider
            label="Crop Y"
            value={values.cropY}
            onChange={(v) => onChange('cropY', v)}
            min={0}
            max={95}
            suffix="%"
          />
          <Slider
            label="Crop W"
            value={values.cropW}
            onChange={(v) => onChange('cropW', v)}
            min={5}
            max={100}
            suffix="%"
          />
          <Slider
            label="Crop H"
            value={values.cropH}
            onChange={(v) => onChange('cropH', v)}
            min={5}
            max={100}
            suffix="%"
          />
        </div>

        <div className="editor-group">
          {groupHead('Effects', { lensVignette: 0 })}
          <Slider
            label="Vignette"
            value={values.lensVignette}
            onChange={(v) => onChange('lensVignette', v)}
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
