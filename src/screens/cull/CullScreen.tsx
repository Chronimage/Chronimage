/**
 * CullScreen — Phase 2 cull-review surface. Compare / Grid / Swipe modes with
 * AI-pick chips, issue flags, and keyboard verdicts. Fully interactive UI stub
 * over live catalog photos; verdict buttons are phase-gated until the Cull
 * verdict engine lands (Phase 2 §2).
 */

import { useCallback, useEffect, useMemo } from 'react';
import { Chip } from '../../primitives/Chip';
import { Icon } from '../../primitives/Icon';
import { Placeholder } from '../../primitives/Placeholder';
import { Seg } from '../../primitives/Seg';
import { Thumbnail } from '../../primitives/Thumbnail';
import { useCull } from '../../state/cull';
import { useCullApplyVerdict, usePhotos } from '../../state/queries';
import type { PhotoRow } from '../../tauri/invoke';
import type { CullMode, CullPair } from './types';

const FALLBACK_ISSUES_A = ['near-duplicate', 'blur 0.18'];
const FALLBACK_ISSUES_B = ['blur 0.41'];

/**
 * Build mock cull-pairs from adjacent photos in the catalog. Once the real
 * `list_cull_pairs` command lands (Phase 2 §2), this becomes a backend fetch.
 */
function buildPairs(photos: PhotoRow[]): CullPair[] {
  const pairs: CullPair[] = [];
  for (let i = 0; i < photos.length - 1; i += 2) {
    const a = photos[i];
    const b = photos[i + 1];
    if (!a || !b) break;
    const reasons = ['burst', 'near-duplicate', 'same subject', 'series'] as const;
    pairs.push({
      ids: [a.id, b.id],
      reason: reasons[i % reasons.length] ?? 'burst',
      similarity: 0.7 + ((i * 13) % 30) / 100,
      keep: (i % 2) as 0 | 1,
      issues_a: FALLBACK_ISSUES_A,
      issues_b: FALLBACK_ISSUES_B,
    });
  }
  return pairs;
}

interface CullStageProps {
  pair: CullPair;
  photosById: Map<number, PhotoRow>;
}

function CullCompare({ pair, photosById }: CullStageProps) {
  const a = photosById.get(pair.ids[0]);
  const b = photosById.get(pair.ids[1]);
  const cards = [
    { photo: a, issues: pair.issues_a, winner: pair.keep === 0, loser: pair.keep === 1, sharp: 0.62 },
    { photo: b, issues: pair.issues_b, winner: pair.keep === 1, loser: pair.keep === 0, sharp: 0.91 },
  ];

  return (
    <div className="cull-stage">
      {cards.map((card, i) => {
        const photo = card.photo;
        if (!photo) return null;
        const swap = photo.orientation >= 5 && photo.orientation <= 8;
        const w = swap ? photo.height : photo.width;
        const h = swap ? photo.width : photo.height;
        const aspectRatio = w > 0 && h > 0 ? `${w} / ${h}` : '3 / 2';
        return (
          <div
            key={photo.id}
            className={`cull-card ${card.winner ? 'winner' : ''} ${card.loser ? 'loser' : ''}`}
          >
            <div className="label">
              <span className="mono">{photo.filename}</span>
              <span className="mono">
                {[
                  photo.camera_model,
                  photo.focal_mm ? `${photo.focal_mm}mm` : null,
                  photo.aperture ? `f/${photo.aperture}` : null,
                  photo.shutter,
                  photo.iso ? `ISO ${photo.iso}` : null,
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </span>
            </div>
            <div className="frame" style={{ aspectRatio }}>
              <Thumbnail
                photoId={photo.id}
                sizePx={1280}
                photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
              />
            </div>
            <div className="tag-row">
              {card.winner && <Chip variant="solid">AI pick · keep</Chip>}
              {card.loser && <Chip tone="warn">Suggested reject</Chip>}
              {card.issues.map((is) => (
                <Chip key={is} tone="warn">
                  {is}
                </Chip>
              ))}
              <span className="mono" style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--fg-mute)' }}>
                sharpness {card.sharp.toFixed(2)} · eyes-open {(0.95 - i * 0.45).toFixed(2)}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function CullGrid({ photos }: { photos: PhotoRow[] }) {
  const sample = photos.slice(0, 18);
  return (
    <div style={{ padding: 20, flex: 1, overflow: 'auto' }}>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 10 }}>
        {sample.map((p, i) => {
          const swap = p.orientation >= 5 && p.orientation <= 8;
          const w = swap ? p.height : p.width;
          const h = swap ? p.width : p.height;
          const aspectRatio = w > 0 && h > 0 ? `${w} / ${h}` : '4 / 3';
          return (
            <div key={p.id} style={{ position: 'relative' }}>
              <div style={{ aspectRatio }}>
                <Thumbnail
                  photoId={p.id}
                  sizePx={320}
                  photo={{ hue: (p.id * 31) % 360, filename: p.filename, id: String(p.id) }}
                />
              </div>
              <div
                style={{
                  position: 'absolute',
                  top: 8,
                  left: 8,
                  display: 'flex',
                  gap: 4,
                  flexWrap: 'wrap',
                  maxWidth: 'calc(100% - 40px)',
                }}
              >
                {i % 3 === 0 && <Chip tone="warn">duplicate</Chip>}
                {i % 4 === 1 && <Chip tone="warn">blur</Chip>}
                {i % 7 === 2 && <Chip tone="warn">eyes closed</Chip>}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function CullSwipe({ pair, photosById }: CullStageProps) {
  const photo = photosById.get(pair.ids[0]);
  if (!photo) return null;
  const swap = photo.orientation >= 5 && photo.orientation <= 8;
  const w = swap ? photo.height : photo.width;
  const h = swap ? photo.width : photo.height;
  const aspectRatio = w > 0 && h > 0 ? `${w} / ${h}` : '3 / 4';
  return (
    <div
      style={{
        flex: 1,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: 40,
        position: 'relative',
      }}
    >
      <div
        style={{
          width: 'min(640px, 80%)',
          aspectRatio,
          position: 'relative',
          transform: 'rotate(-2deg)',
        }}
      >
        <Thumbnail
          photoId={photo.id}
          sizePx={1280}
          photo={{ hue: (photo.id * 31) % 360, filename: photo.filename, id: String(photo.id) }}
        />
        <div
          style={{
            position: 'absolute',
            inset: 0,
            border: '1px solid var(--stroke)',
            borderRadius: 6,
            pointerEvents: 'none',
          }}
        />
        <div
          className="mono"
          style={{
            position: 'absolute',
            top: 20,
            left: 20,
            padding: '6px 12px',
            background: 'var(--warn)',
            color: 'var(--bg)',
            fontSize: 12,
            letterSpacing: '0.1em',
            textTransform: 'uppercase',
            fontWeight: 600,
            borderRadius: 4,
            transform: 'rotate(-6deg)',
          }}
        >
          REJECT
        </div>
      </div>
      <div
        className="mono"
        style={{
          position: 'absolute',
          bottom: 40,
          left: 0,
          right: 0,
          textAlign: 'center',
          fontSize: 12,
          color: 'var(--fg-mute)',
          letterSpacing: '0.05em',
        }}
      >
        Drag left to reject · drag right to keep · space to skip
      </div>
    </div>
  );
}

export function CullScreen() {
  const mode = useCull((s) => s.mode);
  const setMode = useCull((s) => s.setMode);
  const idx = useCull((s) => s.idx);
  const kept = useCull((s) => s.kept);
  const rejected = useCull((s) => s.rejected);
  const recordVerdictLocal = useCull((s) => s.recordVerdict);
  const onPrev = useCull((s) => s.prev);
  const onNext = useCull((s) => s.next);

  const { data: photos = [], isLoading } = usePhotos();
  const applyVerdict = useCullApplyVerdict();

  const pairs = useMemo(() => buildPairs(photos), [photos]);
  const photosById = useMemo(() => new Map(photos.map((p) => [p.id, p])), [photos]);

  const totalPairs = pairs.length;
  const pair = totalPairs > 0 ? pairs[idx % totalPairs] : null;

  const recordVerdict = useCallback(
    (uiVerdict: import('./types').CullVerdict) => {
      if (!pair) return;
      recordVerdictLocal(uiVerdict);
      if (uiVerdict === 'accept_ai') {
        const loserIdx = pair.keep === 0 ? 1 : 0;
        applyVerdict.mutate({
          photoId: pair.ids[loserIdx],
          verdict: 'reject_a',
          reason: 'near_dup',
        });
      } else if (uiVerdict === 'reject_a') {
        applyVerdict.mutate({
          photoId: pair.ids[0],
          verdict: 'reject_a',
          reason: 'user',
        });
      } else if (uiVerdict === 'reject_b') {
        applyVerdict.mutate({
          photoId: pair.ids[1],
          verdict: 'reject_a',
          reason: 'user',
        });
      } else if (uiVerdict === 'reject_both') {
        applyVerdict.mutate({
          photoId: pair.ids[0],
          verdict: 'reject_a',
          reason: 'near_dup',
        });
        applyVerdict.mutate({
          photoId: pair.ids[1],
          verdict: 'reject_a',
          reason: 'near_dup',
        });
      }
    },
    [pair, recordVerdictLocal, applyVerdict],
  );

  useEffect(() => {
    if (!pair) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        onPrev();
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        onNext();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        recordVerdict('accept_ai');
      } else if (e.key === 'a' || e.key === 'A') {
        e.preventDefault();
        recordVerdict('reject_a');
      } else if (e.key === 'b' || e.key === 'B') {
        e.preventDefault();
        recordVerdict('reject_b');
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [pair, onPrev, onNext, recordVerdict]);

  if (isLoading) {
    return (
      <div className="canvas">
        <div style={{ padding: 40, color: 'var(--fg-mute)', fontSize: 13 }}>Loading cull queue…</div>
      </div>
    );
  }

  if (!pair) {
    return (
      <div className="canvas">
        <div
          style={{
            flex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 14,
            padding: 48,
            textAlign: 'center',
          }}
        >
          <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', letterSpacing: '0.1em' }}>
            CULL · NO PAIRS DETECTED
          </div>
          <h1 className="page-title">
            Nothing to cull
            <em>.</em>
          </h1>
          <p style={{ maxWidth: 540, color: 'var(--fg-dim)', fontSize: 13, lineHeight: 1.5 }}>
            Import more photos or adjust the duplicate-similarity threshold in Settings. The backend cull-pair
            detector lands in Phase 2 §2.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="canvas">
      <div className="toolbar">
        <div>
          <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
            CULL · {pair.reason.toUpperCase()}
          </div>
          <div style={{ fontSize: 14, marginTop: 2 }}>
            Similarity{' '}
            <span className="mono" style={{ color: 'var(--accent)' }}>
              {(pair.similarity * 100).toFixed(0)}%
            </span>{' '}
            · pair {idx + 1} of {totalPairs}
          </div>
        </div>
        <div style={{ flex: 1 }} />
        <Seg<CullMode>
          value={mode}
          onChange={setMode}
          options={[
            { value: 'compare', label: 'Compare' },
            { value: 'grid', label: 'Grid' },
            { value: 'swipe', label: 'Swipe' },
          ]}
        />
        <div className="divider" />
        <button type="button" className="btn" onClick={onPrev} title="Previous (←)">
          <Icon name="chevL" size={13} /> Prev
        </button>
        <button type="button" className="btn" onClick={onNext} title="Next (→)">
          Next <Icon name="chevR" size={13} />
        </button>
      </div>

      {mode === 'compare' && <CullCompare pair={pair} photosById={photosById} />}
      {mode === 'grid' && <CullGrid photos={photos} />}
      {mode === 'swipe' && <CullSwipe pair={pair} photosById={photosById} />}

      <div className="cull-filmstrip">
        {photos.slice(0, 24).map((p, i) => {
          const isActive = pair.ids.includes(p.id);
          const isDone = i < idx * 2;
          return (
            <div key={p.id} className={`thumb ${isActive ? 'active' : ''} ${isDone ? 'done' : ''}`}>
              <Placeholder
                photo={{ hue: (p.id * 31) % 360, filename: p.filename, id: String(p.id) }}
                showLabel={false}
              />
            </div>
          );
        })}
      </div>

      <div className="cull-verdict">
        <button
          type="button"
          className="btn"
          title="Reject both (Ctrl+R)"
          onClick={() => recordVerdict('reject_both')}
        >
          <Icon name="reject" size={14} /> Reject both <span className="kbd">⌃R</span>
        </button>
        <button type="button" className="btn" title="Reject A (A)" onClick={() => recordVerdict('reject_a')}>
          <Icon name="reject" size={14} /> Reject A <span className="kbd">A</span>
        </button>
        <button type="button" className="btn" title="Reject B (B)" onClick={() => recordVerdict('reject_b')}>
          <Icon name="reject" size={14} /> Reject B <span className="kbd">B</span>
        </button>
        <div style={{ flex: 1 }} />
        <button
          type="button"
          className="btn primary"
          onClick={() => recordVerdict('accept_ai')}
          title="Accept AI verdict (↵)"
        >
          <Icon name="keep" size={14} /> Accept AI verdict <span className="kbd">↵</span>
        </button>
      </div>
      <div
        className="mono"
        style={{
          padding: '8px 18px',
          fontSize: 10.5,
          color: 'var(--fg-mute)',
          borderTop: '1px solid var(--stroke)',
          letterSpacing: '0.06em',
          display: 'flex',
          gap: 14,
          justifyContent: 'center',
        }}
      >
        <span>✓ kept {kept}</span>
        <span>⊗ rejected {rejected}</span>
        <span>← / → navigate · A/B reject · ↵ accept AI</span>
      </div>
    </div>
  );
}
