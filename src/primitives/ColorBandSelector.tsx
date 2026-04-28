/**
 * 8 colored circles for the Color Mixer panel — single-select. The
 * active band drives which `color_mixer[band]` the shared Hue / Sat /
 * Lum sliders edit. Pattern matches Lightroom's Color Mixer header row.
 */

import type { ColorMixerBand } from '../tauri/invoke';

const BANDS: { id: ColorMixerBand; color: string; label: string }[] = [
  { id: 'red', color: '#e54545', label: 'Red' },
  { id: 'orange', color: '#f08a3c', label: 'Orange' },
  { id: 'yellow', color: '#e8c93c', label: 'Yellow' },
  { id: 'green', color: '#5fc15a', label: 'Green' },
  { id: 'aqua', color: '#54c8c8', label: 'Aqua' },
  { id: 'blue', color: '#5b8cd6', label: 'Blue' },
  { id: 'purple', color: '#a05ccd', label: 'Purple' },
  { id: 'magenta', color: '#d05ca8', label: 'Magenta' },
];

export interface ColorBandSelectorProps {
  active: ColorMixerBand;
  onChange: (band: ColorMixerBand) => void;
  /** Optional set of bands that have non-zero adjustments — renders a
   *  dot under the circle so the user can see at a glance which bands
   *  they've already touched. */
  modifiedBands?: ReadonlySet<ColorMixerBand>;
}

export function ColorBandSelector({ active, onChange, modifiedBands }: ColorBandSelectorProps) {
  return (
    <div className="color-band-row">
      {BANDS.map((band) => {
        const isActive = band.id === active;
        const isModified = modifiedBands?.has(band.id) ?? false;
        return (
          <button
            key={band.id}
            type="button"
            aria-pressed={isActive}
            aria-label={band.label}
            className="color-band-pill"
            data-active={isActive}
            data-modified={isModified}
            onClick={() => onChange(band.id)}
            style={{ borderColor: band.color, color: band.color }}
            title={band.label}
          >
            <span className="color-band-fill" style={{ background: isActive ? band.color : 'transparent' }} />
          </button>
        );
      })}
    </div>
  );
}
