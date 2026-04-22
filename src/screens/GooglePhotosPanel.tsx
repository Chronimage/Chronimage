/**
 * Google Photos connect + pick UI, mounted into SettingsScreen.
 *
 * Drives the OAuth loopback flow + picker session flow end-to-end:
 *   1. User clicks Connect → invoke gphotos_begin_oauth_flow → open auth_url
 *      in the system browser → poll gphotos_poll_oauth_flow until
 *      `completed` (or failed/timed_out).
 *   2. Once connected, "Pick photos" → invoke gphotos_create_picker_session
 *      → open pickerUri → poll gphotos_poll_picker_session until
 *      mediaItemsSet → invoke import_google_photos against the source row
 *      the backend auto-created on auth.
 *   3. Disconnect drops the keyring entry + the sources row.
 *
 * All state is local — this component doesn't use TanStack Query because
 * the flow is inherently procedural (start → poll → done). Polling lives
 * in `useEffect` with cancellation on unmount.
 */

import { open as openShell } from '@tauri-apps/plugin-shell';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { GphotosFlowStatus, GphotosPickerSession, GphotosUserInfo, SourceRow } from '../tauri/invoke';
import {
  gphotosAccountInfo,
  gphotosAuthStatus,
  gphotosBeginOauthFlow,
  gphotosCancelOauthFlow,
  gphotosCreatePickerSession,
  gphotosDeletePickerSession,
  gphotosManualCleanupInstructions,
  gphotosPollOauthFlow,
  gphotosPollPickerSession,
  gphotosSignOut,
  importGooglePhotos,
  listSources,
} from '../tauri/invoke';
import { debug, errorMessage } from '../util/log';

type ConnectState =
  | { kind: 'idle' }
  | { kind: 'connecting'; flowId: string }
  | { kind: 'connected'; email: string | null }
  | { kind: 'error'; message: string };

type PickState =
  | { kind: 'idle' }
  | { kind: 'session'; session: GphotosPickerSession }
  | { kind: 'importing'; sessionId: string; importId: number }
  | { kind: 'done'; importId: number }
  | { kind: 'error'; message: string };

const OAUTH_POLL_INTERVAL_MS = 1000;
const PICKER_POLL_INTERVAL_MS = 3000;

export function GooglePhotosPanel() {
  const [connect, setConnect] = useState<ConnectState>({ kind: 'idle' });
  const [pick, setPick] = useState<PickState>({ kind: 'idle' });
  const [checking, setChecking] = useState(true);
  const cancelledRef = useRef(false);

  // Initial status check: is there already a stored token?
  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const signedIn = await gphotosAuthStatus();
        if (!alive) return;
        if (signedIn) {
          // Fetch the email so the connected state shows identity.
          let email: string | null = null;
          try {
            const info: GphotosUserInfo = await gphotosAccountInfo();
            email = info.email ?? null;
          } catch (err) {
            debug('gphotos: account info fetch failed', err);
          }
          setConnect({ kind: 'connected', email });
        } else {
          setConnect({ kind: 'idle' });
        }
      } catch (err) {
        debug('gphotos: auth status failed', err);
      } finally {
        if (alive) setChecking(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  // OAuth flow poll loop.
  useEffect(() => {
    if (connect.kind !== 'connecting') return;
    cancelledRef.current = false;
    const flowId = connect.flowId;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const tick = async () => {
      if (cancelledRef.current) return;
      try {
        const status: GphotosFlowStatus = await gphotosPollOauthFlow(flowId);
        if (status.state === 'completed') {
          setConnect({ kind: 'connected', email: status.email ?? null });
          return;
        }
        if (status.state === 'failed') {
          setConnect({ kind: 'error', message: status.message });
          return;
        }
        if (status.state === 'timed_out') {
          setConnect({
            kind: 'error',
            message: 'Sign-in timed out. Try again.',
          });
          return;
        }
        // pending → poll again
        timer = setTimeout(tick, OAUTH_POLL_INTERVAL_MS);
      } catch (err) {
        setConnect({
          kind: 'error',
          message: errorMessage(err),
        });
      }
    };
    tick();

    return () => {
      cancelledRef.current = true;
      if (timer) clearTimeout(timer);
    };
  }, [connect]);

  // Picker session poll loop (only while we have an open session).
  useEffect(() => {
    if (pick.kind !== 'session') return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    let alive = true;
    const sessionId = pick.session.id;

    const tick = async () => {
      if (!alive) return;
      try {
        const snapshot = await gphotosPollPickerSession(sessionId);
        if (!alive) return;
        if (snapshot.mediaItemsSet) {
          // User finished picking. Kick off import and clean up the
          // session once the backend has enumerated it.
          try {
            const sources: SourceRow[] = await listSources();
            const row = sources.find((s) => s.kind === 'google_photos');
            if (!row) {
              setPick({
                kind: 'error',
                message: "Couldn't find the Google Photos source row — try reconnecting.",
              });
              return;
            }
            const resp = await importGooglePhotos(row.id, sessionId);
            setPick({ kind: 'importing', sessionId, importId: resp.import_id });
            // Fire-and-forget session delete so Google doesn't keep the
            // picker state around. Failure isn't fatal.
            try {
              await gphotosDeletePickerSession(sessionId);
            } catch (err) {
              debug('gphotos: delete picker session failed', err);
            }
          } catch (err) {
            setPick({
              kind: 'error',
              message: errorMessage(err),
            });
          }
          return;
        }
        timer = setTimeout(tick, PICKER_POLL_INTERVAL_MS);
      } catch (err) {
        setPick({
          kind: 'error',
          message: errorMessage(err),
        });
      }
    };
    tick();

    return () => {
      alive = false;
      if (timer) clearTimeout(timer);
    };
  }, [pick]);

  const handleConnect = useCallback(async () => {
    setConnect({ kind: 'connecting', flowId: '' });
    try {
      const resp = await gphotosBeginOauthFlow();
      await openShell(resp.auth_url);
      setConnect({ kind: 'connecting', flowId: resp.flow_id });
    } catch (err) {
      setConnect({
        kind: 'error',
        message: errorMessage(err),
      });
    }
  }, []);

  const handleCancelConnect = useCallback(async () => {
    if (connect.kind === 'connecting' && connect.flowId) {
      try {
        await gphotosCancelOauthFlow(connect.flowId);
      } catch (err) {
        debug('gphotos: cancel flow failed', err);
      }
    }
    setConnect({ kind: 'idle' });
  }, [connect]);

  const handleDisconnect = useCallback(async () => {
    try {
      await gphotosSignOut();
    } catch (err) {
      debug('gphotos: sign out failed', err);
    }
    setConnect({ kind: 'idle' });
    setPick({ kind: 'idle' });
  }, []);

  const handlePick = useCallback(async () => {
    setPick({ kind: 'idle' });
    try {
      const session = await gphotosCreatePickerSession();
      if (!session.pickerUri) {
        setPick({
          kind: 'error',
          message: 'Photo Picker returned no URL — check the Photo Picker API is enabled.',
        });
        return;
      }
      await openShell(session.pickerUri);
      setPick({ kind: 'session', session });
    } catch (err) {
      setPick({
        kind: 'error',
        message: errorMessage(err),
      });
    }
  }, []);

  const handleManualCleanup = useCallback(async () => {
    try {
      const info = await gphotosManualCleanupInstructions();
      // A browser window for the Google Photos UI is the least we can
      // do — the user deletes manually from there.
      await openShell(info.google_photos_url);
    } catch (err) {
      debug('gphotos: manual cleanup failed', err);
    }
  }, []);

  if (checking) {
    return <div style={{ fontSize: 12, color: 'var(--fg-mute)' }}>Checking Google Photos connection…</div>;
  }

  return (
    <div className="set-row" style={{ display: 'grid', gap: 10 }}>
      {connect.kind === 'idle' && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 13, color: 'var(--fg)' }}>Google Photos</div>
            <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2 }}>
              Sign in once; pick photos via Google's picker to import.
            </div>
          </div>
          <button type="button" className="btn2 primary" onClick={handleConnect}>
            Connect
          </button>
        </div>
      )}

      {connect.kind === 'connecting' && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 13 }}>Waiting for Google sign-in in your browser…</div>
            <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2 }}>
              Complete the consent screen; we'll auto-close this step.
            </div>
          </div>
          <button type="button" className="btn2" onClick={handleCancelConnect}>
            Cancel
          </button>
        </div>
      )}

      {connect.kind === 'connected' && (
        <div style={{ display: 'grid', gap: 8 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 13, color: 'var(--fg)' }}>
                Connected{connect.email ? ` as ${connect.email}` : ''}
              </div>
              <div style={{ fontSize: 11, color: 'var(--fg-mute)', marginTop: 2 }}>
                Pick photos to import, or disconnect to revoke access.
              </div>
            </div>
            <button type="button" className="btn2 primary" onClick={handlePick}>
              Pick photos
            </button>
            <button type="button" className="btn2" onClick={handleDisconnect}>
              Disconnect
            </button>
          </div>

          {pick.kind === 'session' && (
            <div
              style={{
                fontSize: 12,
                color: 'var(--fg-mute)',
                padding: 10,
                border: '1px solid var(--stroke)',
                borderRadius: 6,
              }}
            >
              Waiting for you to pick photos in the Google Photos tab…
            </div>
          )}
          {pick.kind === 'importing' && (
            <div
              style={{
                fontSize: 12,
                color: 'var(--fg)',
                padding: 10,
                border: '1px solid var(--stroke)',
                borderRadius: 6,
              }}
            >
              Downloading picked photos + importing (import #{pick.importId}). Check the import panel for
              progress.
            </div>
          )}
          {pick.kind === 'error' && (
            <div
              style={{
                fontSize: 12,
                color: '#e46',
                padding: 10,
                border: '1px solid #e46',
                borderRadius: 6,
              }}
            >
              Picker failed: {pick.message}
            </div>
          )}

          <div style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
            <button
              type="button"
              className="linkish"
              onClick={handleManualCleanup}
              style={{
                background: 'none',
                border: 0,
                padding: 0,
                color: 'var(--fg-dim)',
                cursor: 'pointer',
                textDecoration: 'underline',
              }}
            >
              Need to delete originals from Google Photos?
            </button>
          </div>
        </div>
      )}

      {connect.kind === 'error' && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ flex: 1, fontSize: 12, color: '#e46' }}>Sign-in failed: {connect.message}</div>
          <button type="button" className="btn2" onClick={() => setConnect({ kind: 'idle' })}>
            Try again
          </button>
        </div>
      )}
    </div>
  );
}
