/**
 * MapScreen — Phase 4 §5 trips view.
 *
 * A live Leaflet map pinned to each trip centroid + the trip list
 * + the photo grid. Clicking a marker or a trip card selects it; the
 * photo grid updates in-place. OpenStreetMap public tiles with
 * attribution per OSMF policy. Offline tile cache + reverse-geocoded
 * place names are follow-ups (Phase 4 week 3+).
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import L from 'leaflet';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { CircleMarker, MapContainer, Marker, Popup, useMap, useMapEvents } from 'react-leaflet';
import 'leaflet/dist/leaflet.css';
import { Icon } from '../../primitives/Icon';
import { Thumbnail } from '../../primitives/Thumbnail';
import { mapListTrips, mapPhotosInTrip, mapRecomputeTrips, mapTile, type TripRow } from '../../tauri/invoke';

interface PinCluster {
  center_lat: number;
  center_lng: number;
  trips: TripRow[];
}

/** Group trips into screen-space buckets using ~80 px cells at the
 *  current zoom. When two or more trip centroids project into the same
 *  cell they render as a single counted cluster pin. */
function clusterTrips(trips: TripRow[], map: L.Map | null): PinCluster[] {
  if (!map || trips.length === 0) {
    return trips.map((t) => ({
      center_lat: t.center_lat,
      center_lng: t.center_lng,
      trips: [t],
    }));
  }
  const CELL_PX = 80;
  const buckets = new Map<string, PinCluster>();
  for (const t of trips) {
    const pt = map.project([t.center_lat, t.center_lng], map.getZoom());
    const cx = Math.floor(pt.x / CELL_PX);
    const cy = Math.floor(pt.y / CELL_PX);
    const key = `${cx}:${cy}`;
    const existing = buckets.get(key);
    if (existing) {
      existing.trips.push(t);
      existing.center_lat =
        (existing.center_lat * (existing.trips.length - 1) + t.center_lat) / existing.trips.length;
      existing.center_lng =
        (existing.center_lng * (existing.trips.length - 1) + t.center_lng) / existing.trips.length;
    } else {
      buckets.set(key, { center_lat: t.center_lat, center_lng: t.center_lng, trips: [t] });
    }
  }
  return [...buckets.values()];
}

/** DivIcon for a cluster pin — accent circle with count. */
function clusterIcon(count: number): L.DivIcon {
  const size = Math.min(48, 22 + Math.sqrt(count) * 4);
  return L.divIcon({
    className: 'map-cluster-icon',
    html: `<div style="width:${size}px;height:${size}px;line-height:${size}px;">${count}</div>`,
    iconSize: [size, size],
    iconAnchor: [size / 2, size / 2],
  });
}

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

/** Scale a trip's photo count to a marker radius in pixels. */
function radiusForCount(count: number): number {
  return Math.max(8, Math.min(28, 6 + Math.sqrt(count) * 2.2));
}

/** Compute the bounding box that contains every trip centroid. */
function tripsBounds(trips: TripRow[]): L.LatLngBoundsExpression | null {
  if (trips.length === 0) return null;
  const latLngs = trips.map((t) => [t.center_lat, t.center_lng] as [number, number]);
  const bounds = L.latLngBounds(latLngs);
  return bounds.isValid() ? bounds : null;
}

/** Custom Leaflet TileLayer that routes every tile fetch through the
 *  Rust `map_tile` command. Hit-then-cache on disk at
 *  `{data_dir}/tiles/{z}/{x}/{y}.png`; network fallback identifies as
 *  Chronimage per OSMF fair-use policy. Attribution still rendered by
 *  the underlying layer. */
const CachedTileLayer = (() => {
  const TL = L.TileLayer.extend({
    createTile(coords: L.Coords, done: L.DoneCallback) {
      const img = document.createElement('img');
      img.alt = '';
      img.setAttribute('role', 'presentation');
      const self = this as unknown as L.TileLayer;
      self.fire('tileloadstart', { tile: img, coords });
      mapTile(coords.z, coords.x, coords.y)
        .then((bytes) => {
          const ab = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
          const blob = new Blob([ab as ArrayBuffer], { type: 'image/png' });
          img.src = URL.createObjectURL(blob);
          img.onload = () => done(undefined, img);
          img.onerror = () => done(new Error('decode failed'), img);
        })
        .catch((e) => done(e instanceof Error ? e : new Error(String(e)), img));
      return img;
    },
  });
  return function CachedTiles({ attribution }: { attribution: string }) {
    const map = useMap();
    useEffect(() => {
      const layer = new (TL as unknown as new (url: string, opts: L.TileLayerOptions) => L.TileLayer)('', {
        attribution,
        maxZoom: 19,
        minZoom: 0,
      });
      layer.addTo(map);
      return () => {
        layer.remove();
      };
    }, [map, attribution]);
    return null;
  };
})();

/** Render filtered trips with zoom-aware screen-space clustering. Single
 *  trips keep the CircleMarker look; shared cells collapse into a count
 *  badge that zooms in on click. */
function ClusteredMarkers({
  trips,
  focusedTripId,
  setFocusedTripId,
}: {
  trips: TripRow[];
  focusedTripId: number | null;
  setFocusedTripId: (id: number) => void;
}) {
  const map = useMap();
  const [zoom, setZoom] = useState<number>(map.getZoom());
  useMapEvents({
    zoomend: () => setZoom(map.getZoom()),
    moveend: () => setZoom(map.getZoom()),
  });
  const clusters = useMemo(() => clusterTrips(trips, map), [trips, map, zoom]);

  return (
    <>
      {clusters.map((c) => {
        if (c.trips.length > 1) {
          const totalPhotos = c.trips.reduce((s, t) => s + t.photo_count, 0);
          return (
            <Marker
              key={`cluster-${c.center_lat.toFixed(3)}-${c.center_lng.toFixed(3)}-${c.trips.length}`}
              position={[c.center_lat, c.center_lng]}
              icon={clusterIcon(c.trips.length)}
              eventHandlers={{
                click: () => map.flyTo([c.center_lat, c.center_lng], Math.min(map.getZoom() + 2, 14)),
              }}
            >
              <Popup>
                <strong>
                  {c.trips.length} trips · {totalPhotos} photos
                </strong>
                <br />
                Click to zoom in.
              </Popup>
            </Marker>
          );
        }
        const t = c.trips[0];
        if (!t) return null;
        const isFocused = focusedTripId === t.id;
        return (
          <CircleMarker
            key={t.id}
            center={[t.center_lat, t.center_lng]}
            radius={radiusForCount(t.photo_count)}
            pathOptions={{
              color: isFocused ? '#5ae3b6' : '#2f7d64',
              fillColor: isFocused ? '#5ae3b6' : '#4fb8a0',
              fillOpacity: isFocused ? 0.9 : 0.7,
              weight: isFocused ? 2 : 1,
            }}
            eventHandlers={{ click: () => setFocusedTripId(t.id) }}
          >
            <Popup>
              <strong>{t.name ?? fmtCoords(t.center_lat, t.center_lng)}</strong>
              <br />
              {fmtDate(t.start_at)}
              {t.start_at !== t.end_at ? ` → ${fmtDate(t.end_at)}` : ''}
              <br />
              {t.photo_count} photo{t.photo_count === 1 ? '' : 's'}
            </Popup>
          </CircleMarker>
        );
      })}
    </>
  );
}

/** Fits the map to the current trip bounds whenever they change. */
function FitBoundsOnTrips({ trips }: { trips: TripRow[] }) {
  const map = useMap();
  useEffect(() => {
    const bounds = tripsBounds(trips);
    if (!bounds) return;
    map.fitBounds(bounds, { padding: [40, 40], maxZoom: 10 });
  }, [map, trips]);
  return null;
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
  const [timeRange, setTimeRange] = useState<'7d' | '30d' | '1y' | 'all'>('all');

  const filteredTrips = useMemo(() => {
    if (timeRange === 'all') return trips;
    const daysByRange = { '7d': 7, '30d': 30, '1y': 365 } as const;
    const days = daysByRange[timeRange];
    const cutoff = Date.now() - days * 86_400_000;
    return trips.filter((t) => {
      const ts = new Date(t.end_at).getTime();
      return Number.isFinite(ts) && ts >= cutoff;
    });
  }, [trips, timeRange]);

  const focusedTrip = useMemo(
    () => filteredTrips.find((t) => t.id === focusedTripId) ?? null,
    [filteredTrips, focusedTripId],
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
            {filteredTrips.length} trip{filteredTrips.length === 1 ? '' : 's'} · GPS-tagged photos clustered
            {timeRange !== 'all' && trips.length !== filteredTrips.length && (
              <span className="mono" style={{ marginLeft: 8, color: 'var(--fg-mute)', fontSize: 11 }}>
                (of {trips.length})
              </span>
            )}
          </div>
        </div>
        <div style={{ flex: 1 }} />
        <div
          className="mono"
          role="tablist"
          aria-label="Time range"
          style={{
            display: 'flex',
            gap: 4,
            fontSize: 11,
            marginRight: 8,
            padding: 3,
            background: 'var(--bg-elev)',
            border: '1px solid var(--stroke)',
            borderRadius: 'var(--radius-sm)',
          }}
        >
          {(['7d', '30d', '1y', 'all'] as const).map((r) => (
            <button
              key={r}
              type="button"
              role="tab"
              aria-selected={timeRange === r}
              onClick={() => setTimeRange(r)}
              style={{
                padding: '3px 10px',
                borderRadius: 4,
                border: 'none',
                cursor: 'pointer',
                background: timeRange === r ? 'var(--accent)' : 'transparent',
                color: timeRange === r ? 'var(--bg)' : 'var(--fg-mute)',
                fontFamily: 'var(--mono-font)',
                fontSize: 11,
              }}
            >
              {r === 'all' ? 'All' : r}
            </button>
          ))}
        </div>
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

      {filteredTrips.length === 0 ? (
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
            NO TRIPS YET
          </div>
          <h1 className="page-title">
            No GPS-tagged photos
            <em>.</em>
          </h1>
          <p style={{ maxWidth: 540, color: 'var(--fg-dim)', fontSize: 13, lineHeight: 1.5 }}>
            Photos with EXIF GPS (from iPhone, iCloud sync, Sony A7 IV geotagged via phone pairing) will
            cluster into trips here. Click <strong>Recompute</strong> once you've imported some.
          </p>
        </div>
      ) : (
        <div
          style={{
            flex: 1,
            display: 'grid',
            gridTemplateRows: '360px 1fr',
            minHeight: 0,
          }}
        >
          <div className="map-canvas" style={{ borderBottom: '1px solid var(--stroke)' }}>
            <MapContainer
              center={[filteredTrips[0]?.center_lat ?? 0, filteredTrips[0]?.center_lng ?? 0]}
              zoom={4}
              scrollWheelZoom={true}
              style={{ width: '100%', height: '100%', background: 'var(--bg-elev)' }}
            >
              <CachedTileLayer attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors' />
              <FitBoundsOnTrips trips={filteredTrips} />
              <ClusteredMarkers
                trips={filteredTrips}
                focusedTripId={focusedTripId}
                setFocusedTripId={setFocusedTripId}
              />
            </MapContainer>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '340px 1fr', minHeight: 0 }}>
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
              {filteredTrips.map((t: TripRow) => (
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
                      style={{
                        fontSize: 11,
                        color: 'var(--accent)',
                        letterSpacing: '0.06em',
                      }}
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
                    {fmtDate(focusedTrip.start_at)} → {fmtDate(focusedTrip.end_at)} ·{' '}
                    {focusedTrip.photo_count} photos · radius {focusedTrip.radius_km.toFixed(1)} km
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
                          photo={{
                            hue: (pid * 31) % 360,
                            filename: `photo-${pid}`,
                            id: String(pid),
                          }}
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
                  Pick a trip from the list or click a pin on the map.
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
