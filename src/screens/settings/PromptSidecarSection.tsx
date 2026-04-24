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
  promptSidecarCommandGet,
  promptSidecarCommandSet,
  promptSidecarGet,
  promptSidecarModelGet,
  promptSidecarModelSet,
  promptSidecarPing,
  promptSidecarProcStart,
  promptSidecarProcStatus,
  promptSidecarProcStop,
  promptSidecarSet,
  type SidecarProcStatus,
  type SidecarStatus,
} from '../../tauri/invoke';

export function PromptSidecarSection() {
  const [url, setUrl] = useState<string>('');
  const [model, setModel] = useState<string>('');
  const [command, setCommand] = useState<string>('');
  const [status, setStatus] = useState<SidecarStatus | null>(null);
  const [procStatus, setProcStatus] = useState<SidecarProcStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [pinging, setPinging] = useState(false);
  const [procBusy, setProcBusy] = useState(false);
  const [procError, setProcError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([promptSidecarGet(), promptSidecarModelGet(), promptSidecarCommandGet()])
      .then(([u, m, c]) => {
        setUrl(u ?? '');
        setModel(m ?? '');
        setCommand(c ?? '');
      })
      .catch(() => {});
    promptSidecarPing()
      .then(setStatus)
      .catch(() => {});
    promptSidecarProcStatus()
      .then(setProcStatus)
      .catch(() => {});
  }, []);

  const saveCommand = useCallback(async () => {
    setProcBusy(true);
    setProcError(null);
    try {
      await promptSidecarCommandSet(command.trim() || null);
      setProcStatus(await promptSidecarProcStatus());
    } catch (e) {
      setProcError(String(e));
    } finally {
      setProcBusy(false);
    }
  }, [command]);

  const startProc = useCallback(async () => {
    setProcBusy(true);
    setProcError(null);
    try {
      await promptSidecarProcStart();
      setProcStatus(await promptSidecarProcStatus());
      // Give the sidecar a breath before the URL-side ping so the
      // connected-badge doesn't flash unreachable for a second.
      globalThis.setTimeout(async () => {
        setStatus(await promptSidecarPing());
      }, 1500);
    } catch (e) {
      setProcError(String(e));
    } finally {
      setProcBusy(false);
    }
  }, []);

  const stopProc = useCallback(async () => {
    setProcBusy(true);
    setProcError(null);
    try {
      await promptSidecarProcStop();
      setProcStatus(await promptSidecarProcStatus());
      setStatus(await promptSidecarPing());
    } catch (e) {
      setProcError(String(e));
    } finally {
      setProcBusy(false);
    }
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

      {/* Launch command + process supervisor */}
      <div style={{ padding: '14px 0 6px' }}>
        <div
          className="mono"
          style={{
            fontSize: 10.5,
            letterSpacing: '0.08em',
            color: 'var(--fg-mute)',
            textTransform: 'uppercase',
            marginBottom: 6,
          }}
        >
          Launch command (optional)
        </div>
        <div style={{ fontSize: 11.5, color: 'var(--fg-mute)', lineHeight: 1.5, marginBottom: 12 }}>
          Save the shell command that starts your sidecar ({`python -m comfyui`}, {`.\\run.bat`}, etc.) so you
          can start/stop it from here instead of juggling a terminal. Chronimage never downloads weights — it
          just spawns the process you point at.
        </div>
      </div>

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
        <label htmlFor="sidecar-cmd" style={{ fontSize: 12, color: 'var(--fg-dim)' }}>
          Command
        </label>
        <input
          id="sidecar-cmd"
          type="text"
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          placeholder="python -m comfyui"
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
        <button
          type="button"
          className="btn"
          onClick={saveCommand}
          disabled={procBusy}
          title="Persist the command to the database"
        >
          {procBusy ? '…' : 'Save'}
        </button>
      </div>

      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          padding: '12px 0',
        }}
      >
        {(() => {
          const runningLabel = procStatus?.running
            ? `Running · PID ${procStatus.pid ?? '?'}`
            : procStatus?.last_exit_code != null
              ? `Stopped (exit ${procStatus.last_exit_code})`
              : procStatus?.configured
                ? 'Idle'
                : 'No command configured';
          const runningColor = procStatus?.running ? 'var(--accent)' : 'var(--fg-mute)';
          return (
            <div
              className="mono"
              style={{
                fontSize: 11,
                padding: '4px 10px',
                borderRadius: 4,
                color: runningColor,
                border: `1px solid ${runningColor}`,
                background: 'color-mix(in oklch, currentColor 8%, transparent)',
              }}
            >
              {runningLabel}
            </div>
          );
        })()}
        <div style={{ flex: 1 }} />
        <button
          type="button"
          className="btn"
          onClick={startProc}
          disabled={procBusy || !procStatus?.configured || procStatus?.running}
          title="Spawn the configured command"
        >
          Start
        </button>
        <button
          type="button"
          className="btn"
          onClick={stopProc}
          disabled={procBusy || !procStatus?.running}
          title="Kill the spawned process"
        >
          Stop
        </button>
      </div>
      {procError && (
        <div
          className="mono"
          style={{
            margin: '4px 0 0',
            padding: '6px 10px',
            borderRadius: 4,
            color: 'var(--danger, #d66)',
            background: 'color-mix(in oklch, var(--danger, #d66) 12%, transparent)',
            fontSize: 11,
          }}
        >
          {procError}
        </div>
      )}
    </div>
  );
}
