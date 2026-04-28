import { type ReactNode, useState } from 'react';
import { CollapsibleSection } from '../../primitives/CollapsibleSection';
import { Icon } from '../../primitives/Icon';
import { Slider } from '../../primitives/Slider';
import type { DevelopCurves, PhotoRow } from '../../tauri/invoke';
import { type CurveChannel, CurvesPanel } from './CurvesPanel';
import type { DevelopValues } from './types';

export interface EditorInspectorProps {
  photo: PhotoRow;
  values: DevelopValues;
  mode?: 'global' | 'mask';
  targetName?: string;
  targetMeta?: string;
  cropMode?: boolean;
  onToggleCropMode?: () => void;
  onClearMaskSelection?: () => void;
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
  mode = 'global',
  targetName,
  targetMeta,
  cropMode = false,
  onToggleCropMode,
  onClearMaskSelection,
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
  const isMaskMode = mode === 'mask';
  const megapixels =
    photo.width && photo.height ? ((photo.width * photo.height) / 1_000_000).toFixed(0) : '—';
  const cameraLabel = [photo.camera_make, photo.camera_model].filter(Boolean).join(' ') || 'Camera unknown';
  const resetAction = (label: string, patch: Partial<DevelopValues>): ReactNode => (
    <button
      type="button"
      className="editor-section-reset"
      onClick={() => onChangeMany(patch)}
      title={`Reset ${label}`}
    >
      reset
    </button>
  );
  const disabledRow = (label: string): ReactNode => (
    <div key={label} className="editor-disabled-row" title="Available when AI models are installed">
      <span className="checkbox" aria-hidden="true" />
      <span>{label}</span>
      <span className="pill">Coming soon</span>
    </div>
  );

  return (
    <div className="editor-inspector">
      <div className="editor-ihead">
        {isMaskMode ? (
          <div>
            <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-dim)' }}>
              Local mask / {targetName ?? 'Selected mask'}
            </div>
            {targetMeta && (
              <div className="mono" style={{ marginTop: 4, fontSize: 10.5, color: 'var(--fg-mute)' }}>
                {targetMeta}
              </div>
            )}
          </div>
        ) : (
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
        )}
        <div style={{ display: 'flex', gap: 4 }}>
          {isMaskMode && onClearMaskSelection ? (
            <button
              type="button"
              title="Return to global adjustments"
              onClick={onClearMaskSelection}
              style={{ color: 'var(--fg-mute)', padding: 4, fontSize: 11 }}
              className="mono"
            >
              Global
            </button>
          ) : (
            <button
              type="button"
              title="Copy edits"
              onClick={onCopy}
              style={{ color: 'var(--fg-mute)', padding: 4 }}
            >
              <Icon name="layers" size={13} />
            </button>
          )}
          <button
            type="button"
            title={isMaskMode ? 'Reset selected mask adjustments' : 'Reset edits'}
            onClick={onReset}
            style={{ color: 'var(--fg-mute)', padding: 4, fontSize: 11 }}
            className="mono"
          >
            Reset
          </button>
        </div>
      </div>

      <div className="editor-ibody">
        <div className="editor-group" style={{ paddingTop: 6, borderTop: 0 }}>
          <button
            type="button"
            className="btn primary"
            onClick={onAutoLight}
            disabled={isMaskMode}
            aria-disabled={isMaskMode}
            style={{ width: '100%', padding: '6px 10px', fontSize: 11.5, justifyContent: 'center' }}
            title={isMaskMode ? 'Auto light is a global photo adjustment' : 'Auto light'}
          >
            <Icon name="sparkles" size={12} /> Auto light
          </button>
        </div>

        <CollapsibleSection
          id="light"
          title="Light"
          defaultOpen
          action={resetAction('Light', { exp: 0, con: 0, hi: 0, sh: 0, whites: 0, blacks: 0 })}
        >
          <Slider label="Exposure" value={values.exp} onChange={(v) => onChange('exp', v)} suffix=" EV" />
          <Slider label="Contrast" value={values.con} onChange={(v) => onChange('con', v)} />
          <Slider label="Highlights" value={values.hi} onChange={(v) => onChange('hi', v)} />
          <Slider label="Shadows" value={values.sh} onChange={(v) => onChange('sh', v)} />
          <Slider label="Whites" value={values.whites} onChange={(v) => onChange('whites', v)} />
          <Slider label="Blacks" value={values.blacks} onChange={(v) => onChange('blacks', v)} />
        </CollapsibleSection>

        <CollapsibleSection id="curves" title="Curve" defaultOpen>
          <CurvesPanel
            value={values.curves}
            onChange={onCurvesChange}
            channel={curveChannel}
            setChannel={setCurveChannel}
          />
        </CollapsibleSection>

        <CollapsibleSection
          id="color"
          title="Color"
          defaultOpen
          action={resetAction('Color', { temp: 0, tint: 0, vib: 0, sat: 0 })}
        >
          <Slider label="Temp" value={values.temp} onChange={(v) => onChange('temp', v)} suffix="K" />
          <Slider label="Tint" value={values.tint} onChange={(v) => onChange('tint', v)} />
          <Slider label="Vibrance" value={values.vib} onChange={(v) => onChange('vib', v)} />
          <Slider label="Saturation" value={values.sat} onChange={(v) => onChange('sat', v)} />
        </CollapsibleSection>

        <CollapsibleSection id="color-mixer" title="Color Mixer">
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Per-band Hue / Saturation / Luminance arrives in the next ops update.
          </div>
        </CollapsibleSection>

        <CollapsibleSection id="color-grading" title="Color Grading">
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Shadow / Midtone / Highlight wheels arrive in the next ops update.
          </div>
        </CollapsibleSection>

        <CollapsibleSection
          id="effects"
          title="Effects"
          action={resetAction('Effects', { clarity: 0, dehaze: 0 })}
        >
          <Slider label="Clarity" value={values.clarity} onChange={(v) => onChange('clarity', v)} />
          <Slider label="Dehaze" value={values.dehaze} onChange={(v) => onChange('dehaze', v)} />
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Texture and Grain arrive with the next ops update.
          </div>
        </CollapsibleSection>

        <CollapsibleSection id="detail" title="Detail">
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Sharpening sliders arrive with the next ops update.
          </div>
          {disabledRow('Denoise')}
          {disabledRow('Raw Details')}
          {disabledRow('Super Resolution')}
        </CollapsibleSection>

        <CollapsibleSection
          id="optics"
          title="Optics"
          action={resetAction('Optics', { lensVignette: 0, lensDistortion: 0, chromaticAberration: 0 })}
        >
          <Slider
            label="Vignette"
            value={values.lensVignette}
            onChange={(v) => onChange('lensVignette', v)}
          />
          <Slider
            label="Lens distortion"
            value={values.lensDistortion}
            onChange={(v) => onChange('lensDistortion', v)}
          />
          <Slider
            label="Chromatic aberration"
            value={values.chromaticAberration}
            onChange={(v) => onChange('chromaticAberration', v)}
            min={0}
            max={100}
          />
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', lineHeight: 1.4 }}>
            Defringe (purple / green) arrives with the next ops update.
          </div>
        </CollapsibleSection>

        <CollapsibleSection
          id="geometry"
          title="Geometry"
          action={resetAction('Geometry', {
            cropX: 0,
            cropY: 0,
            cropW: 100,
            cropH: 100,
            rotation: 0,
            straighten: 0,
            transformH: 0,
            transformV: 0,
          })}
        >
          {onToggleCropMode ? (
            <button
              type="button"
              className="btn"
              onClick={onToggleCropMode}
              disabled={isMaskMode}
              aria-pressed={cropMode}
              style={{ width: '100%', padding: '6px 10px', fontSize: 11.5, justifyContent: 'center' }}
            >
              <Icon name="crop" size={12} /> {cropMode ? 'Exit crop' : 'Crop & straighten'}
            </button>
          ) : null}
          <Slider
            label="Rotation"
            value={values.rotation}
            onChange={(v) => onChange('rotation', v)}
            min={-180}
            max={180}
            suffix="°"
          />
          <Slider
            label="Straighten"
            value={values.straighten}
            onChange={(v) => onChange('straighten', v)}
            min={-10}
            max={10}
            step={0.1}
            suffix="°"
          />
          <Slider label="Transform H" value={values.transformH} onChange={(v) => onChange('transformH', v)} />
          <Slider label="Transform V" value={values.transformV} onChange={(v) => onChange('transformV', v)} />
        </CollapsibleSection>

        <CollapsibleSection
          id="lens-blur"
          title="Lens Blur"
          action={resetAction('Lens Blur', {
            lensBlurAmount: 0,
            lensBlurFocusNear: 0,
            lensBlurFocusFar: 100,
            lensBlurBokehBoost: 0,
            lensBlurCatEye: 0,
          })}
        >
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
        </CollapsibleSection>

        <div className="editor-group">
          <h4>Copy · Paste · Sync</h4>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
            <button
              type="button"
              className="btn"
              onClick={onCopy}
              disabled={isMaskMode}
              aria-disabled={isMaskMode}
              title={isMaskMode ? 'Copy is global-only' : 'Copy current edits'}
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
              disabled={isMaskMode || !canPaste}
              aria-disabled={isMaskMode || !canPaste}
              onClick={onPaste}
              title={
                isMaskMode
                  ? 'Paste is global-only'
                  : canPaste
                    ? 'Paste copied edits onto this photo'
                    : 'Copy edits from a photo first'
              }
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
