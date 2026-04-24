/**
 * MapScreen — Phase 4 §5 trips view (partial scope).
 *
 * The full leaflet/maplibre map with OSM tile cache + reverse-geocoded
 * place names is a follow-up. This pass ships the backend trip list
 * rendered as a scrollable cards grid so users can see their clusters
 * the moment they import GPS-tagged photos. Clicking a trip drills
 * into a filmstrip of the trip's photos.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useMemo, useState } from 'react';
import { Icon } from '../../primitives/Icon';
import { Thumbnail } from '../../primitives/Thumbnail';
import { mapListTrips, mapPhotosInTrip, mapRecomputeTrips, type TripRow } from '../../tauri/invoke';

function fmtDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}

function fmtCoords(lat: number, lng: number): string {
  const latDir = lat >= 0 ? 'N' : 'S';
  const lngDir = lng >= 0 ? 'E' : 'W';
  return `${Math.abs(lat).toFixed(3)}°${latDir}, ${Math.abs(lng).toFixed(3)}°${lngDir}`;
}

export function MapScreen() {
  const qc = useQueryClient();
  const { data: trips = [], isLoading } = useQuery({
    queryKey: ['map_trips'],
    queryFn: mapListTrips,
  });

  const recompute = useMutation({
    mutationFn: mapRecomputeTrips,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['map_trips'] });
    },
  });

  const [focusedTripId, setFocusedTripId] = useState<number | null>(null);

  const focusedTrip = useMemo(
    () => trips.find((t) => t.id === focusedTripId) ?? null,
    [trips, focusedTripId],
  );

  const { data: tripPhotos = [] } = useQuery({
    queryKey: ['trip_photos', focusedTripId],
    queryFn: () => mapPhotosInTrip(focusedTripId as number),
    enabled: focusedTripId != null,
  });

  const onRecompute = useCallback(() => {
    recompute.mutate();
  }, [recompute]);

  if (isLoading) {
    return (
      <div className="canvas">
        <div style={{ padding: 40, color: 'var(--fg-mute)', fontSize: 13 }}>Loading trips…</div>
      </div>
    );
  }

  return (
    <div className="canvas">
      <div className="toolbar">
        <div>
          <div className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)', letterSpacing: '0.08em' }}>
            MAP · TRIPS
          </div>
          <div style={{ fontSize: 14, marginTop: 2 }}>
            {trips.length} trip{trips.length === 1 ? '' : 's'} · GPS-tagged photos clustered
          </div>
        </div>
        <div style={{ flex: 1 }} />
        <button
          type="button"
          className="btn"
          onClick={onRecompute}
          disabled={recompute.isPending}
          title="Recompute trip clusters over all GPS-tagged photos"
        >
          <Icon name="wand" size={13} /> {recompute.isPending ? 'Recomputing…' : 'Recompute'}
        </button>
      </div>

      {trips.length === 0 ? (
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
          <div
            className="mono"
            style={{
              fontSize: 10.5,
              color: 'var(--fg-mute)',
              letterSpacing: '0.1em',
            }}
          >
            NO TRIPS YET
          </div>
          <h1 className="page-title">
            No GPS-tagged photos
            <em>.</em>
          </h1>
          <p
            style={{
              maxWidth: 540,
              color: 'var(--fg-dim)',
              fontSize: 13,
              lineHeight: 1.5,
            }}
          >
            Photos with EXIF GPS (from iPhone, iCloud sync, Sony A7 IV geotagged via phone pairing) will
            cluster into trips here. Click <strong>Recompute</strong> once you've imported some, or re-import
            existing photos if they pre-date Phase 4.
          </p>
        </div>
      ) : (
        <div style={{ flex: 1, display: 'grid', gridTemplateColumns: '340px 1fr', minHeight: 0 }}>
          <div
            style={{
              overflowY: 'auto',
              borderRight: '1px solid var(--stroke)',
              padding: 12,
              display: 'flex',
              flexDirection: 'column',
              gap: 6,
            }}
          >
            {trips.map((t: TripRow) => (
              <button
                key={t.id}
                type="button"
                onClick={() => setFocusedTripId(t.id)}
                aria-pressed={focusedTripId === t.id}
                className={`trip-card ${focusedTripId === t.id ? 'on' : ''}`}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <div
                    className="mono"
                    style={{ fontSize: 11, color: 'var(--accent)', letterSpacing: '0.06em' }}
                  >
                    {fmtDate(t.start_at)}
                    {t.start_at !== t.end_at ? ` → ${fmtDate(t.end_at)}` : ''}
                  </div>
                  <span className="mono" style={{ fontSize: 11, color: 'var(--fg-mute)' }}>
                    {t.photo_count} photo{t.photo_count === 1 ? '' : 's'}
                  </span>
                </div>
                <div style={{ fontSize: 13.5, color: 'var(--fg)', marginTop: 4 }}>
                  {t.name ?? fmtCoords(t.center_lat, t.center_lng)}
                </div>
                <div className="mono" style={{ fontSize: 10.5, color: 'var(--fg-mute)', marginTop: 2 }}>
                  radius ≈ {t.radius_km.toFixed(1)} km
                </div>
              </button>
            ))}
          </div>

          <div style={{ overflowY: 'auto', padding: 16, minHeight: 0 }}>
            {focusedTrip ? (
              <>
                <h2 className="page-title" style={{ fontSize: 'clamp(24px, 3vw, 40px)', marginBottom: 12 }}>
                  {focusedTrip.name ?? fmtCoords(focusedTrip.center_lat, focusedTrip.center_lng)}
                  <em>.</em>
                </h2>
                <div className="mono" style={{ fontSize: 11.5, color: 'var(--fg-mute)', marginBottom: 14 }}>
                  {fmtDate(focusedTrip.start_at)} → {fmtDate(focusedTrip.end_at)} · {focusedTrip.photo_count}{' '}
                  photos · radius {focusedTrip.radius_km.toFixed(1)} km
                </div>
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
                    gap: 8,
                  }}
                >
                  {tripPhotos.map((pid) => (
                    <div key={pid} style={{ position: 'relative', aspectRatio: '3 / 2' }}>
                      <Thumbnail
                        photoId={pid}
                        sizePx={320}
                        photo={{ hue: (pid * 31) % 360, filename: `photo-${pid}`, id: String(pid) }}
                      />
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <div
                style={{
                  color: 'var(--fg-mute)',
                  fontSize: 13,
                  padding: 40,
                  textAlign: 'center',
                }}
              >
                Pick a trip from the list to see its photos.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
