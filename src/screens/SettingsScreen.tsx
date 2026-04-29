/**
 * SettingsScreen — tweaks + AI-model status surface.
 *
 * Four sections:
 *   1. Identity          — app name (persisted via useUi tweaks store)
 *   2. AI Models         — live status from ai_models_status()
 *   3. Culling thresholds — persisted via useUi tweaks store
 *   4. Storage & indexing — persisted via useUi tweaks store
 */

import { listen } from '@tauri-apps/api/event';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { Switch } from '@/components/ui/switch';
import { Slider as PrimitiveSlider } from '../primitives/Slider';
import {
  type ModelSource,
  type ModelStatus,
  useAiModelsStatus,
  useAiReindex,
  useDefaultCatalogPath,
  useDownloadModels,
} from '../state/queries';
import { useCatalogHome } from '../state/settings';
import { useUi } from '../state/ui';
import { DOWNLOAD_PROGRESS_EVENT, type DownloadProgressEvent } from '../tauri/invoke';
import { debug } from '../util/log';
import { GooglePhotosPanel } from './GooglePhotosPanel';
import { PromptSidecarSection } from './settings/PromptSidecarSection';
import { ShortcutsSection } from './settings/ShortcutsSection';

/**
 * Rust `KNOWN_MODELS.kind` values ↔ the `ai_reindex(kind)` accepted values.
 * Kept in lockstep with `src-tauri/src/commands.rs::ai_reindex`.
 *
 * `null` means "no reindex needed after swap" — applies to kinds that
 * only affect query-time processing (text encoder) and don't invalidate
 * anything already stored on disk. The tokenizer is deliberately absent:
 * it's paired with the text encoder and isn't independently swappable.
 */
const KIND_TO_REINDEX: Record<string, string | null> = {
  embedding: 'embeddings',
  'embedding-text': null,
  aesthetic: 'aesthetic',
  'face-detect': 'face-detect',
  'face-embed': 'face-embed',
  'caption-gguf': 'captions',
  'mask-runtime': null,
};

/**
 * Beginner-friendly feature labels per `kind`. These show in the AI-models
 * list, the swap modal title, and anywhere else the kind appears — so keep
 * them short, unambiguous, and in plain English rather than ML jargon.
 */
const KIND_LABEL: Record<string, string> = {
  embedding: 'Understands your photos',
  'embedding-text': 'Understands your search words',
  aesthetic: 'Scores photo quality',
  'face-detect': 'Finds faces in photos',
  'face-embed': 'Tells people apart',
  'mask-runtime': 'Selects masks like Lightroom',
  'caption-gguf': 'Writes photo descriptions',
  tokenizer: 'Search text tokenizer',
};

/**
 * One-line description under the kind label, rendered on each model row
 * so a user who's never heard of "embeddings" understands what the model
 * actually does for them.
 */
const KIND_DESCRIPTION: Record<string, string> = {
  embedding:
    'Turns each photo into a fingerprint the app can match against words you type — the thing that makes "golden hour portrait" find the right shot.',
  'embedding-text':
    'Turns the words you type in the search bar into the same fingerprint format as your photos, so they can be matched.',
  aesthetic:
    'Predicts how technically good each photo looks (sharpness, composition, exposure) so "best of" and rediscovery rows surface the strong ones.',
  'face-detect':
    "Finds where faces are in each photo so the app can group them. Doesn't identify anyone on its own — just locates the faces.",
  'face-embed':
    'Converts each detected face into a fingerprint so the app can group all photos of the same person together.',
  'mask-runtime':
    'Creates local masks for Subject, Sky, Object, Person, Foreground, and Background in the Develop tab.',
  'caption-gguf':
    'Writes a short natural-language description of each photo. Optional — search works without it.',
  tokenizer:
    'Breaks your search text into word pieces the embedding model expects. Not user-swappable; ships with the embedding pair.',
};

interface Preset {
  name: string;
  repo: string;
  filename: string;
  sizeBytes: number;
  license: string;
  installNames?: string[];
  note?: string;
  /**
   * Short plain-English blurb rendered under the name in the picker so a
   * non-ML user can choose "smaller" vs. "more accurate" without needing
   * to know what int8 or fp16 means.
   */
  tagline?: string;
  /**
   * When present, flags this preset as changing the embedding vector
   * dimension vs. the bundled default. Dim changes require a full
   * rebuild of the `vec_photo_embeddings` virtual table and every photo
   * re-embedded, so we show an extra warning before committing. Phase 1
   * offers only same-dim variants to keep this unused; it's here for
   * Phase 2 when we add SigLIP-L / SO400M.
   */
  breakingDimChange?: boolean;
}

/**
 * Curated presets per feature. The first entry is the default bundled model
 * (or first-run download, for captions); additional entries are vetted
 * community alternatives users can swap to without breaking anything else
 * in the pipeline. Custom URLs live in a separate input below the preset
 * list.
 *
 * Same-architecture-only invariant: every preset here produces the same
 * embedding dimension as the default, so swapping doesn't require dropping
 * the sqlite-vec virtual tables. Presets that would change the dimension
 * get flagged with `breakingDimChange: true` + an extra modal warning.
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
      tagline: 'Balanced — ships in the installer, fits most libraries.',
    },
    {
      name: 'siglip2-b16-image-fp16',
      repo: 'onnx-community/siglip2-base-patch16-224-ONNX',
      filename: 'onnx/vision_model_fp16.onnx',
      sizeBytes: 186_000_000,
      license: 'Apache-2.0',
      tagline: 'Half the disk, near-identical quality — best for tight SSDs.',
    },
    {
      name: 'siglip2-b16-image-quantized',
      repo: 'onnx-community/siglip2-base-patch16-224-ONNX',
      filename: 'onnx/vision_model_quantized.onnx',
      sizeBytes: 95_000_000,
      license: 'Apache-2.0',
      tagline: 'Smallest — ~2% accuracy hit; good for spinning disks + low RAM.',
    },
  ],
  'embedding-text': [
    {
      name: 'siglip2-b16-text-quantized',
      repo: 'onnx-community/siglip2-base-patch16-224-ONNX',
      filename: 'onnx/text_model_quantized.onnx',
      sizeBytes: 283_000_000,
      license: 'Apache-2.0',
      note: 'Default · bundled',
      tagline: 'Balanced — small, near-native retrieval quality.',
    },
    {
      name: 'siglip2-b16-text-fp16',
      repo: 'onnx-community/siglip2-base-patch16-224-ONNX',
      filename: 'onnx/text_model_fp16.onnx',
      sizeBytes: 565_000_000,
      license: 'Apache-2.0',
      tagline: 'More precise — swap here if search results feel off.',
    },
    {
      name: 'siglip2-b16-text-fp32',
      repo: 'onnx-community/siglip2-base-patch16-224-ONNX',
      filename: 'onnx/text_model.onnx',
      sizeBytes: 1_130_000_000,
      license: 'Apache-2.0',
      tagline: 'Highest quality, 4× the disk. Only for big catalogs + NVMe.',
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
      tagline: 'Tiny, fast — good enough for rediscovery ranking.',
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
      tagline: 'Standard — works for most lighting + poses.',
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
      tagline: 'Industry-standard — groups same person across angles.',
    },
  ],
  'mask-runtime': [
    {
      name: 'sam2.1-hiera-large',
      repo: 'vietanhdev/segment-anything-2.1-onnx-models',
      filename: 'sam2.1_hiera_large_20260221.zip',
      sizeBytes: 900_000_000,
      license: 'Apache-2.0',
      note: 'Default · bundled',
      tagline: 'Best default quality for Lightroom-style Subject, Sky, Person, and Object selections.',
      installNames: ['sam2.1-hiera-large', 'sam2.1-hiera-large-decoder'],
    },
    {
      name: 'sam2.1-hiera-tiny',
      repo: 'vietanhdev/segment-anything-2.1-onnx-models',
      filename: 'sam2.1_hiera_tiny_20260221.zip',
      sizeBytes: 180_000_000,
      license: 'Apache-2.0',
      tagline: 'Fastest fallback for low-RAM machines; lower mask quality.',
      installNames: ['sam2.1-hiera-tiny', 'sam2.1-hiera-tiny-decoder'],
    },
    {
      name: 'sam3-vith',
      repo: 'vietanhdev/segment-anything-3-onnx-models',
      filename: 'sam3_vit_h.zip',
      sizeBytes: 3_700_000_000,
      license: 'SAM License',
      tagline: 'Optional text-prompt masks like "red car" or "person with hat"; much larger and slower.',
      installNames: [
        'sam3-vith-image-encoder',
        'sam3-vith-image-encoder-data',
        'sam3-vith-language-encoder',
        'sam3-vith-language-encoder-data',
        'sam3-vith-decoder',
        'sam3-vith-decoder-data',
      ],
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
      tagline: '~2.7 GB — decent CPU speed, good captions.',
    },
    {
      name: 'moondream2-f16',
      repo: 'moondream/moondream2-gguf',
      filename: 'moondream2-text-model-fp16.gguf',
      sizeBytes: 3_700_000_000,
      license: 'Apache-2.0',
      tagline: 'Higher fidelity, needs a GPU or a patient CPU.',
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
  return <Switch checked={on} onCheckedChange={onChange} aria-label={label} />;
}

interface SliderProps {
  value: number;
  onChange: (v: number) => void;
  min: number;
  max: number;
  suffix?: string;
  label: string;
}

// ── Appearance / theme tweaks ──────────────────────────────────────────────

const RADII_OPTIONS = [
  { value: 'sharp' as const, label: 'Sharp' },
  { value: 'soft' as const, label: 'Soft' },
  { value: 'pillowy' as const, label: 'Pillowy' },
];
const STROKE_OPTIONS = [
  { value: 'hairline' as const, label: 'Hairline' },
  { value: 'standard' as const, label: 'Standard' },
  { value: 'bold' as const, label: 'Bold' },
];
const SHADOW_OPTIONS = [
  { value: 'flat' as const, label: 'Flat' },
  { value: 'subtle' as const, label: 'Subtle' },
  { value: 'pronounced' as const, label: 'Pronounced' },
];
const THEME_OPTIONS = [
  { value: 'dark' as const, label: 'Dark' },
  { value: 'light' as const, label: 'Light' },
];
const ACCENT_OPTIONS = [
  { value: 'mint' as const, label: 'Mint' },
  { value: 'ember' as const, label: 'Ember' },
  { value: 'violet' as const, label: 'Violet' },
  { value: 'sky' as const, label: 'Sky' },
  { value: 'gold' as const, label: 'Gold' },
];

function ThemeTweaksSection() {
  const tweaks = useUi((s) => s.tweaks);
  const setTweaks = useUi((s) => s.setTweaks);

  return (
    <div className="set-section theme-tweaks-section" style={{ marginTop: 28 }}>
      <h3 className="theme-tweaks-head">Appearance</h3>
      <div className="theme-tweaks-grid">
        <ThemeTweakRow
          label="Theme"
          hint="Light or dark surface"
          value={tweaks.theme}
          options={THEME_OPTIONS}
          onChange={(v) => setTweaks({ theme: v })}
        />
        <ThemeTweakRow
          label="Accent"
          hint="Drives every active state"
          value={tweaks.accent}
          options={ACCENT_OPTIONS}
          onChange={(v) => setTweaks({ accent: v })}
        />
        <ThemeTweakRow
          label="Radii"
          hint="Sharper = more editorial"
          value={tweaks.radii}
          options={RADII_OPTIONS}
          onChange={(v) => setTweaks({ radii: v })}
        />
        <ThemeTweakRow
          label="Strokes"
          hint="Border weight, app-wide"
          value={tweaks.stroke}
          options={STROKE_OPTIONS}
          onChange={(v) => setTweaks({ stroke: v })}
        />
        <ThemeTweakRow
          label="Shadows"
          hint="Ambient elevation depth"
          value={tweaks.shadow}
          options={SHADOW_OPTIONS}
          onChange={(v) => setTweaks({ shadow: v })}
        />
      </div>
    </div>
  );
}

interface ThemeTweakRowProps<T extends string> {
  readonly label: string;
  readonly hint: string;
  readonly value: T;
  readonly options: { value: T; label: string }[];
  readonly onChange: (value: T) => void;
}

function ThemeTweakRow<T extends string>({ label, hint, value, options, onChange }: ThemeTweakRowProps<T>) {
  return (
    <div className="theme-tweak-row">
      <div className="theme-tweak-meta">
        <div className="theme-tweak-label">{label}</div>
        <div className="theme-tweak-hint">{hint}</div>
      </div>
      <div className="theme-tweak-options">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            className={`theme-tweak-pill${value === o.value ? ' is-active' : ''}`}
            onClick={() => onChange(o.value)}
            aria-pressed={value === o.value}
            data-accent={label === 'Accent' ? o.value : undefined}
          >
            {label === 'Accent' && <span className="theme-tweak-swatch" aria-hidden="true" />}
            {o.label}
          </button>
        ))}
      </div>
    </div>
  );
}

function Slider({ value, onChange, min, max, suffix, label }: SliderProps) {
  return (
    <PrimitiveSlider
      label={label}
      value={value}
      onChange={onChange}
      min={min}
      max={max}
      suffix={suffix ?? ''}
      className="settings-slider"
    />
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

function ModelRow({
  model,
  onSwap,
  onInstall,
  installing,
  progressPct,
}: {
  model: ModelStatus;
  onSwap: () => void;
  onInstall: () => void;
  installing: boolean;
  /** 0-100 while a download is in flight; `null` when idle or complete. */
  progressPct: number | null;
}) {
  const badge = sourceBadge(model.source);
  const featureLabel = KIND_LABEL[model.kind] ?? model.kind;
  const presets = PRESETS_BY_KIND[model.kind];
  // Swap eligibility is solely about having alternatives — having a
  // reindex target is a backend concern handled by the picker modal.
  // Kinds without alternatives (e.g. tokenizer) get a disabled button
  // with an explanation in the tooltip.
  const swappable = (presets?.length ?? 0) > 0;
  const unswappableReason =
    model.kind === 'tokenizer'
      ? 'The tokenizer ships paired with the text encoder — swap the encoder instead.'
      : 'No alternatives available yet for this model.';
  // A bundled model can still be "missing" on disk if the installer copy
  // failed or the dev ran `pnpm tauri dev` without running
  // `scripts/fetch-bundled-models.ps1`. Expose a one-click manual install
  // that fetches it from the pinned URL at build time.
  const missing = model.source === 'missing';

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
        <div style={{ fontSize: 13, color: 'var(--fg)' }}>{featureLabel}</div>
        <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2, maxWidth: 460 }}>
          {KIND_DESCRIPTION[model.kind] ?? model.kind}
        </div>
        <div className="mono" style={{ fontSize: 10, color: 'var(--fg-dim)', marginTop: 4, opacity: 0.8 }}>
          {model.name} · {model.filename} · {fmtBytes(model.sizeBytes)}
        </div>
        {installing && (
          <div
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progressPct ?? 0}
            aria-label={`Installing ${model.name}`}
            style={{
              marginTop: 6,
              height: 4,
              background: 'color-mix(in oklch, var(--accent) 15%, var(--bg-elev))',
              borderRadius: 2,
              overflow: 'hidden',
              maxWidth: 320,
            }}
          >
            <div
              style={{
                width: `${progressPct ?? 0}%`,
                height: '100%',
                background: 'var(--accent)',
                // Indeterminate-feeling shimmer when the backend hasn't
                // emitted a progress event yet (progressPct still null).
                transition: 'width 200ms ease-out',
              }}
            />
          </div>
        )}
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
        {missing ? (
          <button
            type="button"
            onClick={onInstall}
            disabled={installing}
            title={installing ? 'Downloading…' : 'Download this model from its pinned URL'}
            style={{
              fontSize: 11,
              padding: '3px 10px',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid color-mix(in oklch, var(--accent) 40%, var(--stroke))',
              background: 'color-mix(in oklch, var(--accent) 18%, var(--bg-elev))',
              color: 'var(--accent)',
              cursor: installing ? 'wait' : 'pointer',
              fontFamily: 'var(--mono-font)',
            }}
          >
            {installing ? 'Installing…' : 'Install'}
          </button>
        ) : (
          <button
            type="button"
            onClick={onSwap}
            disabled={!swappable}
            title={swappable ? 'Swap to a different model' : unswappableReason}
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
        )}
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
      // `in` check (not truthy) so `null` (= "no reindex needed") still
      // counts as a supported kind. `undefined` means the kind is
      // genuinely unsupported (tokenizer etc.) and the Swap button
      // should've been disabled before we got here.
      if (!(feature.kind in KIND_TO_REINDEX)) {
        throw new Error(`unsupported kind ${feature.kind}`);
      }
      const reindexKind = KIND_TO_REINDEX[feature.kind];
      if (activePreset && activePreset.name !== feature.name) {
        // Selected a different preset — ensure it's downloaded first.
        await download.mutateAsync(activePreset.installNames ?? [activePreset.name]);
      } else if (customRepo.trim() && customFilename.trim()) {
        // Custom HF URL flow — Phase 1b will land the register-custom flow.
        // For now, surface a friendly "not yet available" to avoid silent no-op.
        throw new Error('Custom HF URLs land in Phase 1b — pick a preset for now.');
      }
      // `null` reindex = query-time-only swap (text encoder). Skip the
      // backend reindex since no stored data is invalidated by the swap.
      // `undefined` can't happen because we early-returned above if the
      // kind wasn't in the map — narrow it here for the compiler.
      if (reindexKind !== null && reindexKind !== undefined) {
        await reindex.mutateAsync(reindexKind);
      }
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
        <h2 style={{ marginTop: 0, fontSize: 16 }}>
          Swap the model for "{KIND_LABEL[feature.kind] ?? feature.kind}"
        </h2>
        <div style={{ fontSize: 11.5, color: 'var(--fg-mute)', marginBottom: 8, maxWidth: 500 }}>
          {KIND_DESCRIPTION[feature.kind] ?? ''}
        </div>
        <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-dim)', marginBottom: 16 }}>
          Current: {feature.name} ({sourceBadge(feature.source).label.toLowerCase()})
        </div>

        {/* Re-index warning — show whenever the selection differs from the
           current model, since any swap re-fingerprints every photo for
           this feature. Users with large libraries need to see the cost
           before they commit. */}
        {selected && selected !== feature.name && (
          <div
            style={{
              marginBottom: 12,
              padding: '10px 12px',
              border: '1px solid color-mix(in oklch, #f0b429 40%, var(--stroke))',
              background: 'color-mix(in oklch, #f0b429 8%, var(--bg-elev))',
              borderRadius: 'var(--radius-sm)',
              fontSize: 11.5,
              color: 'var(--fg)',
            }}
          >
            {KIND_TO_REINDEX[feature.kind] === null ? (
              <>
                <strong>Heads-up:</strong> this swap only changes how your <em>future</em> search queries are
                interpreted. Nothing on disk is cleared or re-fingerprinted — your catalog stays exactly
                as-is, and the new model takes effect on the very next search.
              </>
            ) : (
              <>
                <strong>Heads-up:</strong> swapping will clear your current{' '}
                {(KIND_LABEL[feature.kind] ?? 'model').toLowerCase()} data and rebuild it from scratch the
                next time the catalog runs. On a 10 000-photo library that's about 5–10 minutes of background
                work; 100 000 photos, closer to an hour. You can keep using Chronimage while it runs — search
                results for this feature just won't update until it finishes.
              </>
            )}
            {presets.find((p) => p.name === selected)?.breakingDimChange && (
              <div style={{ marginTop: 6, color: 'var(--danger)' }}>
                <strong>Architecture change:</strong> this model uses a different embedding size, so every
                existing photo's fingerprint is discarded too. Make sure you have time for a full reimport
                before confirming.
              </div>
            )}
          </div>
        )}

        {presets.map((p) => {
          const isActive = p.name === (selected ?? feature.name);
          return (
            <label
              key={p.name}
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 10,
                padding: '10px 12px',
                border: `1px solid ${isActive ? 'var(--accent)' : 'var(--stroke)'}`,
                borderRadius: 'var(--radius-sm)',
                marginBottom: 8,
                cursor: 'pointer',
                background: isActive ? 'color-mix(in oklch, var(--accent) 8%, var(--bg))' : 'transparent',
              }}
            >
              <input
                type="radio"
                name="preset"
                checked={isActive}
                onChange={() => setSelected(p.name)}
                style={{ marginTop: 3 }}
              />
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: 13 }}>
                  {p.tagline ?? p.name}
                  {p.note ? (
                    <span
                      className="mono"
                      style={{
                        fontSize: 10,
                        marginLeft: 8,
                        padding: '1px 6px',
                        borderRadius: 4,
                        background: 'color-mix(in oklch, var(--accent) 18%, var(--bg-elev))',
                        color: 'var(--accent)',
                      }}
                    >
                      {p.note}
                    </span>
                  ) : null}
                </div>
                <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 3 }}>
                  {p.name} · {fmtBytes(p.sizeBytes)} · {p.license} · {p.repo}
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
  const cullBinRetentionDays = useUi((s) => s.tweaks.cullBinRetentionDays);
  const nightlyReindex = useUi((s) => s.tweaks.nightlyReindex);
  const cachePath = useUi((s) => s.tweaks.cachePath);
  const preferredChannel = useUi((s) => s.tweaks.preferredChannel);
  const setTweaks = useUi((s) => s.setTweaks);

  const [catalogHome, setCatalogHome] = useCatalogHome();
  const { data: defaultCatalogHome } = useDefaultCatalogPath();
  const effectiveHome = catalogHome ?? defaultCatalogHome ?? null;

  async function pickCatalogHome() {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Choose catalog home',
      });
      if (typeof selected === 'string' && selected.length > 0) {
        await setCatalogHome(selected);
      }
    } catch (err) {
      debug('settings: catalog-home picker failed', err);
    }
  }

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
  const visibleModels = models.filter((model) => model.kind !== 'mask-runtime-component');

  const [pickerFor, setPickerFor] = useState<ModelStatus | null>(null);
  // Tracks which model name is currently being installed so the row shows
  // an "Installing…" affordance without blocking the whole UI.
  const [installingName, setInstallingName] = useState<string | null>(null);
  // Maps model.name → percent (0-100) while a download is in flight. Gets
  // cleared on completion so the row falls back to the Installed/Bundled
  // badge without a stale progress bar.
  const [progressByName, setProgressByName] = useState<Record<string, number>>({});
  const installMutation = useDownloadModels();

  // Subscribe once to `chronimage://download-progress` — the backend emits
  // these from `download_models` regardless of whether the download was
  // triggered by this component or another caller (e.g., ModelPickerModal
  // swap). Cleanup on unmount.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let alive = true;
    listen<DownloadProgressEvent>(DOWNLOAD_PROGRESS_EVENT, (event) => {
      const p = event.payload;
      const percent =
        p.total_bytes > 0
          ? Math.min(100, Math.round((p.downloaded_bytes / p.total_bytes) * 100))
          : p.done
            ? 100
            : 0;
      setProgressByName((prev) => {
        if (p.done) {
          const { [p.model_name]: _removed, ...rest } = prev;
          return rest;
        }
        return { ...prev, [p.model_name]: percent };
      });
    })
      .then((off) => {
        if (alive) {
          unlisten = off;
        } else {
          off();
        }
      })
      .catch((err) => debug('settings: listen download-progress failed', err));
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  async function handleInstallModel(model: ModelStatus) {
    setInstallingName(model.name);
    try {
      const defaultPreset = PRESETS_BY_KIND[model.kind]?.[0];
      await installMutation.mutateAsync(defaultPreset?.installNames ?? [model.name]);
      await refetchModels();
    } catch (err) {
      debug('settings: install model failed', model.name, err);
    } finally {
      setInstallingName(null);
      // Progress cleared by the `done: true` event, but wipe defensively
      // in case the backend reported success without a final event.
      setProgressByName((prev) => {
        const { [model.name]: _removed, ...rest } = prev;
        return rest;
      });
    }
  }

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
        <div className="settings" style={{ padding: '28px 32px 48px' }}>
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

          {/* ── 1b. Appearance ── */}
          <ThemeTweaksSection />

          {/* ── 1a. Library ── */}
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
              Library
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
                <div style={{ fontSize: 13, color: 'var(--fg)' }}>Import mode</div>
                <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2 }}>
                  Every import copies photos into the catalog. The source-delete option lives on the
                  folder-picker confirmation dialog so you can decide per folder.
                </div>
              </div>
              <div
                className="mono"
                style={{
                  fontSize: 11,
                  color: 'var(--fg-dim)',
                  padding: '4px 10px',
                  border: '1px solid var(--stroke)',
                  borderRadius: 4,
                }}
              >
                copy to catalog
              </div>
            </div>

            <div
              className="set-row"
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 24,
                paddingBottom: 14,
                paddingTop: 14,
                borderBottom: '1px solid var(--stroke)',
              }}
            >
              <div className="lbl" style={{ flex: 1 }}>
                <div style={{ fontSize: 13, color: 'var(--fg)' }}>Catalog home</div>
                <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2 }}>
                  Destination for new photos when Consolidate mode is on. Existing photos are not migrated.
                </div>
                <code
                  style={{
                    fontSize: 11,
                    color: 'var(--fg-dim)',
                    display: 'block',
                    marginTop: 4,
                    wordBreak: 'break-all',
                  }}
                >
                  {effectiveHome ?? '…'}
                </code>
              </div>
              <button
                type="button"
                className="btn2 ghost"
                onClick={pickCatalogHome}
                style={{ fontSize: 12, padding: '6px 12px', whiteSpace: 'nowrap' }}
              >
                Change…
              </button>
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
            {!modelsLoading && !modelsError && visibleModels.length === 0 && (
              <div className="mono" style={{ fontSize: 12, color: 'var(--fg-mute)', padding: '12px 0' }}>
                No models registered yet.
              </div>
            )}
            {visibleModels.map((m) => (
              <ModelRow
                key={m.filename}
                model={m}
                onSwap={() => setPickerFor(m)}
                onInstall={() => handleInstallModel(m)}
                installing={installingName === m.name}
                progressPct={progressByName[m.name] ?? null}
              />
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
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 16,
                padding: '12px 0',
              }}
            >
              <div className="lbl" style={{ flex: 1 }}>
                <div style={{ fontSize: 13, color: 'var(--fg)' }}>Cull Bin retention</div>
                <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
                  Rejects stay recoverable this many days before the daily sweep permanently deletes
                </div>
              </div>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, width: 260 }}>
                <input
                  type="range"
                  min={1}
                  max={90}
                  step={1}
                  value={cullBinRetentionDays}
                  onChange={(e) => setTweaks({ cullBinRetentionDays: Number(e.target.value) })}
                  aria-label="Cull Bin retention days"
                  style={{ flex: 1, accentColor: 'var(--accent)' }}
                />
                <span
                  className="mono"
                  style={{ fontSize: 11, color: 'var(--fg)', width: 56, textAlign: 'right' }}
                >
                  {cullBinRetentionDays} days
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

          {/* ── 5. Prompt sidecar ── */}
          <PromptSidecarSection />

          {/* ── 6. Keyboard shortcuts ── */}
          <ShortcutsSection />
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
