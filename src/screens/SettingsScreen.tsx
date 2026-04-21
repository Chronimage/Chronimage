/**
 * SettingsScreen — ported from design-handoff/project/src/screens_misc.jsx lines 162–211.
 *
 * Four sections:
 *   1. Identity          — app name (in-memory; TODO persist)
 *   2. AI Models         — live status from ai_models_status()
 *   3. Culling thresholds — in-memory sliders/toggles
 *   4. Storage & indexing — in-memory toggles
 */

import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import {
  type ModelSource,
  type ModelStatus,
  useAiModelsStatus,
  useAiReindex,
  useDownloadModels,
} from '../state/queries';
import { useUi } from '../state/ui';
import { debug } from '../util/log';
import { GooglePhotosPanel } from './GooglePhotosPanel';

/**
 * Rust `KNOWN_MODELS.kind` values ↔ the `ai_reindex(kind)` accepted values.
 * Kept in lockstep with `src-tauri/src/commands.rs::ai_reindex`.
 */
const KIND_TO_REINDEX: Record<string, string> = {
  embedding: 'embeddings',
  aesthetic: 'aesthetic',
  'face-detect': 'face-detect',
  'face-embed': 'face-embed',
  'caption-gguf': 'captions',
};

/** Human-readable feature label per `kind`. */
const KIND_LABEL: Record<string, string> = {
  embedding: 'Embeddings',
  aesthetic: 'Aesthetic score',
  'face-detect': 'Face detection',
  'face-embed': 'Face embedding',
  'caption-gguf': 'Captions',
};

interface Preset {
  name: string;
  repo: string;
  filename: string;
  sizeBytes: number;
  license: string;
  note?: string;
}

/**
 * Curated presets per feature. The first entry is the default bundled model;
 * additional entries are vetted community alternatives users can swap to.
 * Custom URLs live in a separate input below the preset list.
 */
const PRESETS_BY_KIND: Record<string, Preset[]> = {
  embedding: [
    {
      name: 'siglip2-b16-image',
      repo: 'onnx-community/siglip2-base-patch16-224-ONNX',
      filename: 'onnx/vision_model.onnx',
      sizeBytes: 371_807_752,
      license: 'Apache-2.0',
      note: 'Default · bundled',
    },
  ],
  aesthetic: [
    {
      name: 'nima-aesthetic',
      repo: 'cromsc/nima-mobilenet-aesthetic',
      filename: 'nima_mobilenet_aesthetic.onnx',
      sizeBytes: 12_867_270,
      license: 'permissive',
      note: 'Default · bundled',
    },
  ],
  'face-detect': [
    {
      name: 'scrfd-10g',
      repo: 'deepinsight/insightface (buffalo_l.zip)',
      filename: 'det_10g.onnx',
      sizeBytes: 16_923_827,
      license: 'MIT',
      note: 'Default · bundled',
    },
  ],
  'face-embed': [
    {
      name: 'arcface-w600k-r50',
      repo: 'deepinsight/insightface (buffalo_l.zip)',
      filename: 'w600k_r50.onnx',
      sizeBytes: 174_383_860,
      license: 'MIT',
      note: 'Default · bundled',
    },
  ],
  'caption-gguf': [
    {
      name: 'moondream2-q4',
      repo: 'moondream/moondream2-gguf',
      filename: 'moondream2-text-model-f16.gguf',
      sizeBytes: 2_839_534_976,
      license: 'Apache-2.0',
      note: 'Default · on-demand download',
    },
  ],
};

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

function sourceBadge(source: ModelSource): { label: string; accent: 'ok' | 'info' | 'warn' } {
  switch (source) {
    case 'bundled':
      return { label: 'Bundled', accent: 'ok' };
    case 'downloaded':
      return { label: 'Installed', accent: 'info' };
    default:
      return { label: 'Missing', accent: 'warn' };
  }
}

function ModelRow({ model, onSwap }: { model: ModelStatus; onSwap: () => void }) {
  const badge = sourceBadge(model.source);
  const featureLabel = KIND_LABEL[model.kind] ?? model.kind;
  const presets = PRESETS_BY_KIND[model.kind];
  const swappable = model.kind in KIND_TO_REINDEX && (presets?.length ?? 0) > 0;

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
          {featureLabel} · {model.filename} · {fmtBytes(model.sizeBytes)}
        </div>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <span
          className="mono"
          style={{
            fontSize: 10,
            padding: '2px 7px',
            borderRadius: 'var(--radius-sm)',
            background:
              badge.accent === 'ok'
                ? 'color-mix(in oklch, var(--accent) 18%, var(--bg-elev))'
                : badge.accent === 'info'
                  ? 'color-mix(in oklch, var(--info, #5aa7ff) 18%, var(--bg-elev))'
                  : 'var(--bg-elev)',
            color:
              badge.accent === 'ok'
                ? 'var(--accent)'
                : badge.accent === 'info'
                  ? 'var(--info, #5aa7ff)'
                  : 'var(--fg-mute)',
            border: `1px solid ${badge.accent === 'warn' ? 'var(--stroke)' : 'color-mix(in oklch, var(--accent) 30%, var(--stroke))'}`,
          }}
        >
          {badge.label}
        </span>
        <button
          type="button"
          onClick={onSwap}
          disabled={!swappable}
          title={swappable ? 'Swap to a different model' : 'No alternatives available yet'}
          style={{
            fontSize: 11,
            padding: '3px 10px',
            borderRadius: 'var(--radius-sm)',
            border: '1px solid var(--stroke)',
            background: 'var(--bg-elev)',
            color: swappable ? 'var(--fg)' : 'var(--fg-mute)',
            cursor: swappable ? 'pointer' : 'not-allowed',
            fontFamily: 'var(--mono-font)',
          }}
        >
          Swap…
        </button>
      </div>
    </div>
  );
}

// ── Picker modal (Radix-free minimal dialog) ──────────────────────────────────

interface PickerProps {
  open: boolean;
  feature: ModelStatus | null;
  onClose: () => void;
  onSwapped: () => void;
}

function ModelPickerModal({ open, feature, onClose, onSwapped }: PickerProps) {
  const download = useDownloadModels();
  const reindex = useAiReindex();
  const [selected, setSelected] = useState<string | null>(null);
  const [customRepo, setCustomRepo] = useState('');
  const [customFilename, setCustomFilename] = useState('');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!open || !feature) return null;

  const presets = PRESETS_BY_KIND[feature.kind] ?? [];
  const activePreset = presets.find((p) => p.name === (selected ?? feature.name)) ?? presets[0];

  async function handleSwap() {
    if (!feature) return;
    setErrorMsg(null);
    try {
      const reindexKind = KIND_TO_REINDEX[feature.kind];
      if (!reindexKind) {
        throw new Error(`unsupported kind ${feature.kind}`);
      }
      if (activePreset && activePreset.name !== feature.name) {
        // Selected a different preset — ensure it's downloaded first.
        await download.mutateAsync([activePreset.name]);
      } else if (customRepo.trim() && customFilename.trim()) {
        // Custom HF URL flow — Phase 1b will land the register-custom flow.
        // For now, surface a friendly "not yet available" to avoid silent no-op.
        throw new Error('Custom HF URLs land in Phase 1b — pick a preset for now.');
      }
      await reindex.mutateAsync(reindexKind);
      onSwapped();
      onClose();
    } catch (e) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    }
  }

  const busy = download.isPending || reindex.isPending;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`Swap ${KIND_LABEL[feature.kind] ?? 'model'}`}
      onClick={onClose}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.55)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
        role="document"
        style={{
          background: 'var(--bg)',
          border: '1px solid var(--stroke)',
          borderRadius: 'var(--radius-md)',
          padding: 24,
          width: 560,
          maxHeight: '80vh',
          overflow: 'auto',
        }}
      >
        <h2 style={{ marginTop: 0, fontSize: 16 }}>Swap {KIND_LABEL[feature.kind] ?? feature.kind} model</h2>
        <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', marginBottom: 16 }}>
          Current: {feature.name} ({sourceBadge(feature.source).label.toLowerCase()})
        </div>

        {presets.map((p) => {
          const isActive = p.name === (selected ?? feature.name);
          return (
            <label
              key={p.name}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                padding: '8px 10px',
                border: `1px solid ${isActive ? 'var(--accent)' : 'var(--stroke)'}`,
                borderRadius: 'var(--radius-sm)',
                marginBottom: 8,
                cursor: 'pointer',
                background: isActive ? 'color-mix(in oklch, var(--accent) 8%, var(--bg))' : 'transparent',
              }}
            >
              <input type="radio" name="preset" checked={isActive} onChange={() => setSelected(p.name)} />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 13, fontFamily: 'var(--mono-font)' }}>{p.name}</div>
                <div style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 2 }}>
                  {p.repo} · {fmtBytes(p.sizeBytes)} · {p.license}
                  {p.note ? ` · ${p.note}` : ''}
                </div>
              </div>
            </label>
          );
        })}

        {presets.length <= 1 && (
          <div
            className="mono"
            style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 12, marginBottom: 12 }}
          >
            Only the default is bundled for this feature. Curated alternates land in Phase 2.
          </div>
        )}

        <details style={{ marginTop: 14 }}>
          <summary style={{ cursor: 'pointer', fontSize: 12 }}>Add custom HF URL…</summary>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 10 }}>
            <input
              type="text"
              placeholder="repo_id (e.g. onnx-community/siglip2-large-patch16-384-ONNX)"
              value={customRepo}
              onChange={(e) => setCustomRepo(e.target.value)}
              style={{
                fontSize: 12,
                padding: '6px 10px',
                borderRadius: 6,
                border: '1px solid var(--stroke)',
                background: 'var(--bg-elev)',
                color: 'var(--fg)',
                fontFamily: 'var(--mono-font)',
              }}
            />
            <input
              type="text"
              placeholder="filename (e.g. onnx/vision_model.onnx)"
              value={customFilename}
              onChange={(e) => setCustomFilename(e.target.value)}
              style={{
                fontSize: 12,
                padding: '6px 10px',
                borderRadius: 6,
                border: '1px solid var(--stroke)',
                background: 'var(--bg-elev)',
                color: 'var(--fg)',
                fontFamily: 'var(--mono-font)',
              }}
            />
            <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)' }}>
              Download SHA-verifies against the first published hash. Custom flow lands in Phase 1b.
            </div>
          </div>
        </details>

        {errorMsg && (
          <div
            className="mono"
            style={{
              fontSize: 12,
              color: 'var(--danger)',
              marginTop: 12,
              padding: '8px 10px',
              border: '1px solid var(--danger)',
              borderRadius: 'var(--radius-sm)',
            }}
          >
            {errorMsg}
          </div>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 20 }}>
          <button
            type="button"
            onClick={onClose}
            disabled={busy}
            style={{
              fontSize: 12,
              padding: '6px 14px',
              border: '1px solid var(--stroke)',
              borderRadius: 'var(--radius-sm)',
              background: 'transparent',
              color: 'var(--fg)',
              cursor: busy ? 'not-allowed' : 'pointer',
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSwap}
            disabled={busy}
            style={{
              fontSize: 12,
              padding: '6px 14px',
              border: '1px solid var(--accent)',
              borderRadius: 'var(--radius-sm)',
              background: 'var(--accent)',
              color: 'var(--bg)',
              cursor: busy ? 'not-allowed' : 'pointer',
            }}
          >
            {busy ? 'Working…' : 'Swap & re-index'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Root ──────────────────────────────────────────────────────────────────────

export function SettingsScreen() {
  // All persisted tweaks live in useUi (backed by @tauri-apps/plugin-store
  // via hydrateFromStore + savePersisted). Local mirror state is only used
  // for the app-name input which needs onBlur-commit semantics.
  const appName = useUi((s) => s.tweaks.appName);
  const dupeSimilarity = useUi((s) => s.tweaks.dupeSimilarity);
  const sharpnessCutoff = useUi((s) => s.tweaks.sharpnessCutoff);
  const requireReview = useUi((s) => s.tweaks.requireReview);
  const nightlyReindex = useUi((s) => s.tweaks.nightlyReindex);
  const cachePath = useUi((s) => s.tweaks.cachePath);
  const preferredChannel = useUi((s) => s.tweaks.preferredChannel);
  const setTweaks = useUi((s) => s.setTweaks);

  async function pickCachePath() {
    try {
      const selected = await openDialog({ directory: true, multiple: false });
      if (typeof selected === 'string' && selected.length > 0) {
        setTweaks({ cachePath: selected });
      }
    } catch (err) {
      debug('settings: cache picker failed', err);
    }
  }

  const [localAppName, setLocalAppName] = useState<string>(appName);

  const {
    data: models = [],
    isLoading: modelsLoading,
    isError: modelsError,
    refetch: refetchModels,
  } = useAiModelsStatus();

  const [pickerFor, setPickerFor] = useState<ModelStatus | null>(null);

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

          {/* ── 1b. Cloud sources ── */}
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
              Cloud sources
            </h3>
            <GooglePhotosPanel />
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
              <ModelRow key={m.filename} model={m} onSwap={() => setPickerFor(m)} />
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
              <Slider
                label="Duplicate similarity threshold"
                value={dupeSimilarity}
                onChange={(v) => setTweaks({ dupeSimilarity: v })}
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
              <Slider
                label="Sharpness cutoff score"
                value={sharpnessCutoff}
                onChange={(v) => setTweaks({ sharpnessCutoff: v })}
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
                <Toggle
                  on={requireReview}
                  onChange={(v) => setTweaks({ requireReview: v })}
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
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <div
                  style={{
                    fontFamily: 'var(--mono-font)',
                    fontSize: 12,
                    color: 'var(--fg-dim)',
                    maxWidth: 340,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                  title={cachePath ?? 'Default (catalog root)'}
                >
                  {cachePath ?? 'Default (catalog root)'}
                </div>
                <button
                  type="button"
                  className="btn2"
                  style={{ padding: '4px 10px', fontSize: 11 }}
                  onClick={() => {
                    void pickCachePath();
                  }}
                >
                  Change…
                </button>
                {cachePath && (
                  <button
                    type="button"
                    className="btn2 ghost"
                    style={{ padding: '4px 10px', fontSize: 11 }}
                    onClick={() => setTweaks({ cachePath: null })}
                  >
                    Reset
                  </button>
                )}
              </div>
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
                Nightly re-index
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <Toggle
                  on={nightlyReindex}
                  onChange={(v) => setTweaks({ nightlyReindex: v })}
                  label="Enable nightly re-index"
                />
                <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
                  02:00 · Wake from sleep
                </span>
              </div>
            </div>
            <div
              className="set-row"
              style={{ display: 'flex', alignItems: 'center', gap: 24, padding: '12px 0' }}
            >
              <div className="lbl" style={{ flex: 1, fontSize: 13, color: 'var(--fg)' }}>
                Update channel
                <div
                  className="mono"
                  style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 'var(--space-1)' }}
                >
                  Which release train this installation follows
                </div>
              </div>
              <select
                value={preferredChannel}
                onChange={(e) =>
                  setTweaks({
                    preferredChannel: e.target.value as 'stable' | 'beta' | 'nightly' | 'insider',
                  })
                }
                aria-label="Update channel"
                style={{
                  padding: '5px 10px',
                  fontSize: 12,
                  fontFamily: 'var(--mono-font)',
                  background: 'var(--bg-elev)',
                  border: '1px solid var(--stroke)',
                  borderRadius: 'var(--radius-sm)',
                  color: 'var(--fg)',
                }}
              >
                <option value="stable">Stable — monthly</option>
                <option value="beta">Beta — fortnightly</option>
                <option value="nightly">Nightly — daily</option>
                <option value="insider">Insider — continuous</option>
              </select>
            </div>
          </div>
        </div>
      </div>
      <ModelPickerModal
        open={pickerFor !== null}
        feature={pickerFor}
        onClose={() => setPickerFor(null)}
        onSwapped={() => {
          refetchModels().catch(() => {});
        }}
      />
    </div>
  );
}
