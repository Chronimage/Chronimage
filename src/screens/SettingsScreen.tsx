/**
 * SettingsScreen — ported from design-handoff/project/src/screens_misc.jsx lines 162–211.
 *
 * Four sections:
 *   1. Identity          — app name (in-memory; TODO persist)
 *   2. AI Models         — live status from ai_models_status()
 *   3. Culling thresholds — in-memory sliders/toggles
 *   4. Storage & indexing — in-memory toggles
 */

import { useState } from 'react';
import { type ModelStatus, useAiModelsStatus } from '../state/queries';
import { useUi } from '../state/ui';

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(0)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

// ── Inline primitives (design language) ──────────────────────────────────────

interface ToggleProps {
  on: boolean;
  onChange: (next: boolean) => void;
  label?: string;
}

function Toggle({ on, onChange, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => onChange(!on)}
      style={{
        width: 36,
        height: 20,
        borderRadius: 10,
        border: 'none',
        background: on ? 'var(--accent)' : 'var(--stroke-strong)',
        cursor: 'pointer',
        position: 'relative',
        flexShrink: 0,
        transition: 'background 0.15s',
      }}
    >
      <span
        style={{
          position: 'absolute',
          top: 3,
          left: on ? 19 : 3,
          width: 14,
          height: 14,
          borderRadius: '50%',
          background: 'var(--fg)',
          transition: 'left 0.15s',
        }}
      />
    </button>
  );
}

interface SliderProps {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  suffix?: string;
  label: string;
}

function Slider({ value, onChange, min, max, suffix, label }: SliderProps) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
      <input
        type="range"
        min={min}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
        style={{ accentColor: 'var(--accent)', width: 160 }}
      />
      <span className="mono" style={{ fontSize: 12, color: 'var(--fg-dim)', minWidth: 40 }}>
        {value}
        {suffix ?? ''}
      </span>
    </div>
  );
}

// ── AI Models section row ─────────────────────────────────────────────────────

function ModelRow({ model }: { model: ModelStatus }) {
  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        padding: '8px 0',
        borderBottom: '1px solid var(--stroke)',
      }}
    >
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 13, color: 'var(--fg)', fontFamily: 'var(--mono-font)' }}>{model.name}</div>
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 2 }}>
          {model.kind} · {model.filename} · {fmtBytes(model.sizeBytes)}
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span
          className="mono"
          style={{
            fontSize: 10,
            padding: '2px 7px',
            borderRadius: 'var(--radius-sm)',
            background: model.installed
              ? 'color-mix(in oklch, var(--accent) 18%, var(--bg-elev))'
              : 'var(--bg-elev)',
            color: model.installed ? 'var(--accent)' : 'var(--fg-mute)',
            border: `1px solid ${model.installed ? 'color-mix(in oklch, var(--accent) 30%, var(--stroke))' : 'var(--stroke)'}`,
          }}
        >
          {model.installed ? 'Installed' : 'Not installed'}
        </span>
        {/* Change model buttons are deferred to phase-1b */}
        <span
          className="mono"
          style={{
            fontSize: 9.5,
            color: 'var(--fg-mute)',
            padding: '1px 5px',
            border: '1px solid var(--stroke)',
            borderRadius: 'var(--radius-sm)',
          }}
          title="Model switching ships in phase 1b"
        >
          phase-1b
        </span>
      </div>
    </div>
  );
}

// ── Root ──────────────────────────────────────────────────────────────────────

export function SettingsScreen() {
  const appName = useUi((s) => s.tweaks.appName);
  const setTweaks = useUi((s) => s.setTweaks);

  // TODO(cc): persist all of these via tauri-plugin-store once the plugin is wired
  const [localAppName, setLocalAppName] = useState<string>(appName);
  const [dupeSimilarity, setDupeSimilarity] = useState<number>(85);
  const [sharpnessCutoff, setSharpnessCutoff] = useState<number>(32);
  const [requireReview, setRequireReview] = useState<boolean>(true);
  const [nightlyReindex, setNightlyReindex] = useState<boolean>(true);

  const { data: models = [], isLoading: modelsLoading, isError: modelsError } = useAiModelsStatus();

  function handleAppNameBlur() {
    const trimmed = localAppName.trim();
    if (trimmed && trimmed !== appName) {
      setTweaks({ appName: trimmed });
    } else {
      setLocalAppName(appName);
    }
  }

  return (
    <div className="canvas">
      <div className="canvas-scroll">
        <div className="settings" style={{ maxWidth: 740, padding: '28px 32px 48px', margin: '0 auto' }}>
          <h1>
            Settings<em>.</em>
          </h1>

          {/* ── 1. Identity ── */}
          <div className="set-section" style={{ marginTop: 28 }}>
            <h3
              style={{
                margin: '0 0 14px',
                fontSize: 13,
                color: 'var(--fg-dim)',
                fontFamily: 'var(--mono-font)',
                letterSpacing: '0.06em',
                textTransform: 'uppercase',
              }}
            >
              Identity
            </h3>
            <div
              className="set-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 24,
                paddingBottom: 14,
                borderBottom: '1px solid var(--stroke)',
              }}
            >
              <div className="lbl" style={{ flex: 1 }}>
                <div style={{ fontSize: 13, color: 'var(--fg)' }}>App name</div>
                <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2 }}>
                  Shown in titlebar &amp; dock
                </div>
              </div>
              <input
                className="tx-input"
                value={localAppName}
                onChange={(e) => setLocalAppName(e.target.value)}
                onBlur={handleAppNameBlur}
                aria-label="App name"
                style={{
                  background: 'var(--bg-elev)',
                  border: '1px solid var(--stroke)',
                  borderRadius: 6,
                  padding: '6px 10px',
                  color: 'var(--fg)',
                  fontFamily: 'var(--mono-font)',
                  fontSize: 12,
                  minWidth: 220,
                }}
              />
            </div>
          </div>

          {/* ── 2. AI Models ── */}
          <div className="set-section" style={{ marginTop: 28 }}>
            <h3
              style={{
                margin: '0 0 14px',
                fontSize: 13,
                color: 'var(--fg-dim)',
                fontFamily: 'var(--mono-font)',
                letterSpacing: '0.06em',
                textTransform: 'uppercase',
              }}
            >
              AI Models
            </h3>
            {modelsLoading && (
              <div className="mono" style={{ fontSize: 12, color: 'var(--fg-mute)', padding: '12px 0' }}>
                Loading model status…
              </div>
            )}
            {modelsError && (
              <div className="mono" style={{ fontSize: 12, color: 'var(--danger)', padding: '12px 0' }}>
                Failed to load model status.
              </div>
            )}
            {!modelsLoading && !modelsError && models.length === 0 && (
              <div className="mono" style={{ fontSize: 12, color: 'var(--fg-mute)', padding: '12px 0' }}>
                No models registered yet.
              </div>
            )}
            {models.map((m) => (
              <ModelRow key={m.filename} model={m} />
            ))}
          </div>

          {/* ── 3. Culling thresholds ── */}
          <div className="set-section" style={{ marginTop: 28 }}>
            <h3
              style={{
                margin: '0 0 14px',
                fontSize: 13,
                color: 'var(--fg-dim)',
                fontFamily: 'var(--mono-font)',
                letterSpacing: '0.06em',
                textTransform: 'uppercase',
              }}
            >
              Culling thresholds
            </h3>

            <div
              className="set-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 24,
                padding: '12px 0',
                borderBottom: '1px solid var(--stroke)',
              }}
            >
              <div className="lbl" style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>
                Duplicate similarity
              </div>
              {/* TODO(cc): persist via tauri-plugin-store */}
              <Slider
                label="Duplicate similarity threshold"
                value={dupeSimilarity}
                onChange={setDupeSimilarity}
                min={50}
                max={100}
                suffix="%"
              />
            </div>

            <div
              className="set-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 24,
                padding: '12px 0',
                borderBottom: '1px solid var(--stroke)',
              }}
            >
              <div className="lbl" style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>
                Sharpness cutoff
              </div>
              {/* TODO(cc): persist via tauri-plugin-store */}
              <Slider
                label="Sharpness cutoff score"
                value={sharpnessCutoff}
                onChange={setSharpnessCutoff}
                min={0}
                max={100}
              />
            </div>

            <div
              className="set-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 24,
                padding: '12px 0',
                borderBottom: '1px solid var(--stroke)',
              }}
            >
              <div className="lbl" style={{ flex: 1 }}>
                <div style={{ fontSize: 13, color: 'var(--fg)' }}>Require final review</div>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                {/* TODO(cc): persist via tauri-plugin-store */}
                <Toggle
                  on={requireReview}
                  onChange={setRequireReview}
                  label="Require final review before deletion"
                />
                <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
                  Rejects moved to trash only after you confirm
                </span>
              </div>
            </div>
          </div>

          {/* ── 4. Storage & indexing ── */}
          <div className="set-section" style={{ marginTop: 28 }}>
            <h3
              style={{
                margin: '0 0 14px',
                fontSize: 13,
                color: 'var(--fg-dim)',
                fontFamily: 'var(--mono-font)',
                letterSpacing: '0.06em',
                textTransform: 'uppercase',
              }}
            >
              Storage &amp; indexing
            </h3>

            <div
              className="set-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 24,
                padding: '12px 0',
                borderBottom: '1px solid var(--stroke)',
              }}
            >
              <div className="lbl" style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>
                Cache location
              </div>
              {/* TODO(cc): wire to tauri path picker + tauri-plugin-store */}
              <div style={{ fontFamily: 'var(--mono-font)', fontSize: 12, color: 'var(--fg-dim)' }}>
                D:/Chronimage/cache · 84.2 GB
              </div>
            </div>

            <div
              className="set-row"
              style={{ display: 'flex', alignItems: 'center', gap: 24, padding: '12px 0' }}
            >
              <div className="lbl" style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>
                Nightly re-index
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                {/* TODO(cc): persist via tauri-plugin-store */}
                <Toggle on={nightlyReindex} onChange={setNightlyReindex} label="Enable nightly re-index" />
                <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
                  02:00 · Wake from sleep
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
