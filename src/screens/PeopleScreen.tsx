/**
 * PeopleScreen — face cluster grid.
 *
 * Wired to the backend commands:
 *   face_clusters_list  → useFaceClusters()
 *   face_cluster_name   → useFaceClusterName()
 *   face_cluster_merge  → useFaceClusterMerge()
 */

import { useEffect, useState } from 'react';
import { Icon } from '../primitives/Icon';
import { Thumbnail } from '../primitives/Thumbnail';
import {
  type ClusterRow,
  useFaceClusterMerge,
  useFaceClusterName,
  useFaceClusters,
  usePhotosForCluster,
} from '../state/queries';

// ── Types ─────────────────────────────────────────────────────────────────────

type FilterTab = 'all' | 'named' | 'unnamed';

// ── Helpers ───────────────────────────────────────────────────────────────────

function coverHue(id: number): number {
  return (id * 47) % 360;
}

// ── Sub-components ────────────────────────────────────────────────────────────

interface ClusterCardProps {
  cluster: ClusterRow;
  onNameBlur: (clusterId: number, name: string) => void;
  onMergeClick: (clusterId: number) => void;
  onOpen: (clusterId: number) => void;
  merging: boolean;
}

function ClusterCard({ cluster, onNameBlur, onMergeClick, onOpen, merging }: ClusterCardProps) {
  const [draft, setDraft] = useState<string>(cluster.name ?? '');

  function handleBlur() {
    const trimmed = draft.trim();
    if (trimmed !== (cluster.name ?? '')) {
      onNameBlur(cluster.id, trimmed);
    }
  }

  const hue = coverHue(cluster.id);

  return (
    <div
      className="person-card"
      style={{
        border: '1px solid var(--stroke)',
        borderRadius: 'var(--radius-md)',
        background: 'var(--bg-elev)',
        padding: 'var(--space-3)',
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--space-2)',
      }}
    >
      {/* Cover — click to open cluster drill-down */}
      <button
        type="button"
        className="faces"
        onClick={() => onOpen(cluster.id)}
        aria-label={`Open cluster ${cluster.name ?? cluster.id}`}
        style={{
          aspectRatio: '1',
          borderRadius: 'var(--radius-sm)',
          overflow: 'hidden',
          background: `oklch(0.45 0.14 ${hue})`,
          backgroundImage:
            'repeating-linear-gradient(-45deg, transparent 0 6px, rgba(255,255,255,0.07) 6px 7px)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: 'rgba(255,255,255,0.4)',
          border: 'none',
          cursor: 'pointer',
          padding: 0,
        }}
      >
        <Icon name="faces" size={28} />
      </button>

      {/* Face count badge */}
      <div
        className="mono"
        style={{
          fontSize: 10.5,
          color: 'var(--fg-mute)',
          letterSpacing: '0.06em',
        }}
      >
        {cluster.faceCount.toLocaleString()} PHOTOS
      </div>

      {/* Inline name edit */}
      <input
        className="tx-input"
        value={draft}
        placeholder={`Unnamed · cluster ${cluster.id}`}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={handleBlur}
        aria-label={`Name for cluster ${cluster.id}`}
        style={{
          background: 'var(--surface-2, var(--bg))',
          border: '1px solid var(--stroke)',
          borderRadius: 'var(--radius-sm)',
          padding: '5px 8px',
          color: 'var(--fg)',
          fontFamily: 'var(--mono-font)',
          fontSize: 12,
          width: '100%',
          boxSizing: 'border-box',
        }}
      />

      {/* Merge secondary action */}
      <button
        type="button"
        className="btn2"
        onClick={() => onMergeClick(cluster.id)}
        disabled={merging}
        style={{
          fontSize: 11,
          padding: '4px 8px',
          color: 'var(--fg-mute)',
          width: '100%',
          justifyContent: 'center',
        }}
        title="Merge this cluster with another"
      >
        Merge with…
      </button>
    </div>
  );
}

// ── Root ──────────────────────────────────────────────────────────────────────

export function PeopleScreen() {
  const [filter, setFilter] = useState<FilterTab>('all');
  // clusterId pending a merge-target pick; null when no merge in progress
  const [mergePending, setMergePending] = useState<number | null>(null);
  const [openedClusterId, setOpenedClusterId] = useState<number | null>(null);

  const { data: clusters = [], isLoading, isError } = useFaceClusters(60);
  const nameCluster = useFaceClusterName();
  const mergeCluster = useFaceClusterMerge();

  const openedCluster =
    openedClusterId === null ? null : (clusters.find((c) => c.id === openedClusterId) ?? null);

  const filtered =
    filter === 'named'
      ? clusters.filter((c) => c.isNamed)
      : filter === 'unnamed'
        ? clusters.filter((c) => !c.isNamed)
        : clusters;

  function handleNameBlur(clusterId: number, name: string) {
    nameCluster.mutate({ clusterId, name });
  }

  function handleMergeClick(clusterId: number) {
    if (mergePending === null) {
      // First click — remember which cluster to merge from
      setMergePending(clusterId);
    } else if (mergePending !== clusterId) {
      // Second click on a different card — execute merge
      mergeCluster.mutate({ a: mergePending, b: clusterId }, { onSettled: () => setMergePending(null) });
    } else {
      // Clicked the same card again — cancel
      setMergePending(null);
    }
  }

  const TABS: { id: FilterTab; label: string }[] = [
    { id: 'all', label: 'All' },
    { id: 'named', label: 'Named' },
    { id: 'unnamed', label: 'Unnamed' },
  ];

  return (
    <div className="canvas">
      <div className="canvas-scroll">
        <div
          style={{
            maxWidth: 1200,
            padding: 'var(--space-6) var(--space-6) 40px',
            margin: '0 auto',
          }}
        >
          {/* Header */}
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              justifyContent: 'space-between',
              marginBottom: 'var(--space-5)',
              gap: 'var(--space-4)',
              flexWrap: 'wrap',
            }}
          >
            <h1 style={{ margin: 0 }}>
              People<em>.</em>
            </h1>

            {/* Segmented filter */}
            <fieldset
              aria-label="Filter clusters"
              style={{
                display: 'flex',
                gap: 2,
                background: 'var(--bg-elev)',
                border: '1px solid var(--stroke)',
                borderRadius: 'var(--radius-md)',
                padding: 3,
                margin: 0,
              }}
            >
              {TABS.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  className={filter === tab.id ? 'btn2 primary' : 'btn2'}
                  onClick={() => setFilter(tab.id)}
                  style={{
                    padding: '5px 14px',
                    fontSize: 12,
                    fontFamily: 'var(--mono-font)',
                    border: 'none',
                    background: filter === tab.id ? 'var(--accent)' : 'transparent',
                    color: filter === tab.id ? 'var(--accent-ink)' : 'var(--fg-dim)',
                    borderRadius: 'var(--radius-sm)',
                  }}
                  aria-pressed={filter === tab.id}
                >
                  {tab.label}
                </button>
              ))}
            </fieldset>
          </div>

          {/* Merge hint banner */}
          {mergePending !== null && (
            <div
              className="mono"
              style={{
                fontSize: 11,
                color: 'var(--accent)',
                background: 'color-mix(in oklch, var(--accent) 8%, var(--bg-elev))',
                border: '1px solid color-mix(in oklch, var(--accent) 30%, var(--stroke))',
                borderRadius: 'var(--radius-md)',
                padding: '8px 14px',
                marginBottom: 'var(--space-4)',
                display: 'flex',
                alignItems: 'center',
                gap: 'var(--space-2)',
              }}
            >
              <Icon name="faces" size={13} />
              Click another cluster to merge into it, or click the same one to cancel.
              <button
                type="button"
                style={{
                  marginLeft: 'auto',
                  color: 'var(--fg-mute)',
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  fontSize: 11,
                }}
                onClick={() => setMergePending(null)}
              >
                Cancel
              </button>
            </div>
          )}

          {/* States */}
          {isLoading && (
            <div
              className="mono"
              style={{ fontSize: 12, color: 'var(--fg-mute)', padding: '40px 0', textAlign: 'center' }}
            >
              Loading clusters…
            </div>
          )}

          {isError && (
            <div
              className="mono"
              style={{ fontSize: 12, color: 'var(--danger)', padding: '40px 0', textAlign: 'center' }}
            >
              Failed to load face clusters.
            </div>
          )}

          {!isLoading && !isError && clusters.length === 0 && (
            <div
              style={{
                padding: '60px 20px',
                textAlign: 'center',
                border: '1px dashed var(--stroke)',
                borderRadius: 'var(--radius-lg)',
                color: 'var(--fg-mute)',
              }}
            >
              <Icon name="faces" size={36} />
              <div style={{ marginTop: 'var(--space-3)', fontSize: 14 }}>
                No face clusters yet — run an import to detect faces.
              </div>
            </div>
          )}

          {!isLoading && !isError && filtered.length === 0 && clusters.length > 0 && (
            <div
              className="mono"
              style={{ fontSize: 12, color: 'var(--fg-mute)', padding: '40px 0', textAlign: 'center' }}
            >
              No clusters match this filter.
            </div>
          )}

          {/* Cluster grid */}
          {filtered.length > 0 && (
            <div
              className="person-grid"
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
                gap: 'var(--space-4)',
              }}
            >
              {filtered.map((cluster) => (
                <ClusterCard
                  key={cluster.id}
                  cluster={cluster}
                  onNameBlur={handleNameBlur}
                  onMergeClick={handleMergeClick}
                  onOpen={setOpenedClusterId}
                  merging={mergeCluster.isPending && mergePending !== null}
                />
              ))}
            </div>
          )}
        </div>
      </div>
      {openedCluster && <ClusterDrillDown cluster={openedCluster} onClose={() => setOpenedClusterId(null)} />}
    </div>
  );
}

interface ClusterDrillDownProps {
  cluster: ClusterRow;
  onClose: () => void;
}

function ClusterDrillDown({ cluster, onClose }: ClusterDrillDownProps) {
  const { data: photos = [], isLoading, isError } = usePhotosForCluster(cluster.id, 200);
  const title = cluster.name?.trim() || `Cluster #${cluster.id}`;

  // Escape closes the drill-down.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={`Photos of ${title}`}
      style={{
        position: 'absolute',
        inset: 0,
        background: 'var(--bg)',
        display: 'flex',
        flexDirection: 'column',
        zIndex: 10,
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: 'var(--space-4) var(--space-6)',
          borderBottom: '1px solid var(--stroke)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'baseline', gap: 'var(--space-3)' }}>
          <button
            type="button"
            className="btn2"
            onClick={onClose}
            style={{ padding: '4px 10px', fontSize: 12 }}
          >
            <Icon name="chevL" size={12} /> Back
          </button>
          <h2 style={{ margin: 0, fontSize: 22 }}>
            {title}
            <em>.</em>
          </h2>
          <span className="mono" style={{ fontSize: 11.5, color: 'var(--fg-mute)', letterSpacing: '0.06em' }}>
            {cluster.faceCount.toLocaleString()} FACES
          </span>
        </div>
      </div>
      <div style={{ flex: 1, overflowY: 'auto', padding: 'var(--space-5) var(--space-6)' }}>
        {isLoading && (
          <div className="mono" style={{ fontSize: 12, color: 'var(--fg-mute)' }}>
            Loading photos…
          </div>
        )}
        {isError && (
          <div className="mono" style={{ fontSize: 12, color: 'var(--danger)' }}>
            Failed to load photos for this cluster.
          </div>
        )}
        {!isLoading && !isError && photos.length === 0 && (
          <div
            style={{
              padding: '60px var(--space-5)',
              textAlign: 'center',
              border: '1px dashed var(--stroke)',
              borderRadius: 'var(--radius-lg)',
              color: 'var(--fg-mute)',
            }}
          >
            <Icon name="faces" size={36} />
            <div style={{ marginTop: 'var(--space-3)', fontSize: 14 }}>
              No photos tagged with this cluster yet.
            </div>
          </div>
        )}
        {photos.length > 0 && (
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(160px, 1fr))',
              gap: 'var(--space-1)',
            }}
          >
            {photos.map((p) => (
              <div key={p.id} className="cell" style={{ aspectRatio: '3/2', position: 'relative' }}>
                <Thumbnail
                  photoId={p.id}
                  photo={{ hue: (p.id * 31) % 360, filename: p.filename, id: String(p.id) }}
                  subtle
                />
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
