/**
 * Settings → Prompt sidecar — Phase 4 §1/§2 config surface.
 *
 * Chronimage doesn't bundle Flux/SDXL; the user points at their own
 * OpenAI-compatible endpoint (a running `diffusers` / `comfyui` /
 * `candle` sidecar). This row stores the base URL + preferred model
 * and surfaces the live ping status so the Prompt tab knows whether
 * Generate will work.
 */

import { useCallback, useEffect, useState } from 'react';
import {
  promptSidecarGet,
  promptSidecarModelGet,
  promptSidecarModelSet,
  promptSidecarPing,
  promptSidecarSet,
  type SidecarStatus,
} from '../../tauri/invoke';

export function PromptSidecarSection() {
  const [url, setUrl] = useState<string>('');
  const [model, setModel] = useState<string>('');
  const [status, setStatus] = useState<SidecarStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [pinging, setPinging] = useState(false);

  useEffect(() => {
    Promise.all([promptSidecarGet(), promptSidecarModelGet()])
      .then(([u, m]) => {
        setUrl(u ?? '');
        setModel(m ?? '');
      })
      .catch(() => {});
    promptSidecarPing()
      .then(setStatus)
      .catch(() => {});
  }, []);

  const save = useCallback(async () => {
    setSaving(true);
    try {
      await promptSidecarSet(url.trim() || null);
      await promptSidecarModelSet(model.trim() || null);
      const s = await promptSidecarPing();
      setStatus(s);
    } finally {
      setSaving(false);
    }
  }, [url, model]);

  const ping = useCallback(async () => {
    setPinging(true);
    try {
      setStatus(await promptSidecarPing());
    } finally {
      setPinging(false);
    }
  }, []);

  let badgeColor = 'var(--fg-mute)';
  let badgeText = 'Not configured';
  if (status?.configured && status?.reachable) {
    badgeColor = 'var(--accent)';
    badgeText = `Connected · ${status.model ?? 'flux-dev'}`;
  } else if (status?.configured) {
    badgeColor = 'var(--danger, #d66)';
    badgeText = `Unreachable${status.error ? ` · ${status.error}` : ''}`;
  }

  return (
    <div className="set-section" style={{ marginTop: 40 }}>
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
        Prompt sidecar (AI edits)
      </h3>
      <p style={{ margin: '0 0 14px', color: 'var(--fg-mute)', fontSize: 12, lineHeight: 1.5 }}>
        Chronimage doesn't bundle generative models. Run a Flux-dev / SDXL-Inpaint sidecar yourself (the
        OpenAI-compatible variants expose <code>/v1/models</code> and <code>/v1/edit</code>) and point at it
        below. Leave blank to disable the Prompt tab's Generate button.
      </p>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '140px 1fr auto',
          gap: 10,
          alignItems: 'center',
          paddingBottom: 12,
          borderBottom: '1px solid var(--stroke)',
        }}
      >
        <label htmlFor="sidecar-url" style={{ fontSize: 12, color: 'var(--fg-dim)' }}>
          Sidecar URL
        </label>
        <input
          id="sidecar-url"
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="http://localhost:17183"
          style={{
            padding: '6px 10px',
            fontSize: 12,
            fontFamily: 'var(--mono-font)',
            background: 'var(--bg-elev)',
            border: '1px solid var(--stroke)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--fg)',
          }}
        />
        <div
          className="mono"
          style={{
            fontSize: 11,
            padding: '4px 10px',
            borderRadius: 4,
            color: badgeColor,
            border: `1px solid ${badgeColor}`,
            background: 'color-mix(in oklch, currentColor 8%, transparent)',
          }}
        >
          {badgeText}
        </div>
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '140px 1fr auto',
          gap: 10,
          alignItems: 'center',
          padding: '12px 0',
        }}
      >
        <label htmlFor="sidecar-model" style={{ fontSize: 12, color: 'var(--fg-dim)' }}>
          Preferred model
        </label>
        <input
          id="sidecar-model"
          type="text"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="flux-dev"
          style={{
            padding: '6px 10px',
            fontSize: 12,
            fontFamily: 'var(--mono-font)',
            background: 'var(--bg-elev)',
            border: '1px solid var(--stroke)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--fg)',
          }}
        />
        <div style={{ display: 'flex', gap: 6 }}>
          <button type="button" className="btn" onClick={ping} disabled={pinging}>
            {pinging ? 'Pinging…' : 'Test'}
          </button>
          <button type="button" className="btn primary" onClick={save} disabled={saving}>
            {saving ? 'Saving…' : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
}
