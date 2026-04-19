/**
 * PeopleScreen — face cluster grid.
 *
 * Wired to the backend commands:
 *   face_clusters_list  → useFaceClusters()
 *   face_cluster_name   → useFaceClusterName()
 *   face_cluster_merge  → useFaceClusterMerge()
 */

import { useState } from 'react';
import { Icon } from '../primitives/Icon';
import { type ClusterRow, useFaceClusterMerge, useFaceClusterName, useFaceClusters } from '../state/queries';

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
  merging: boolean;
}

function ClusterCard({ cluster, onNameBlur, onMergeClick, merging }: ClusterCardProps) {
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
      {/* Cover thumbnail placeholder */}
      <div
        className="faces"
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
        }}
      >
        <Icon name="faces" size={28} />
      </div>

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

  const { data: clusters = [], isLoading, isError } = useFaceClusters(60);
  const nameCluster = useFaceClusterName();
  const mergeCluster = useFaceClusterMerge();

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
                  merging={mergeCluster.isPending && mergePending !== null}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
