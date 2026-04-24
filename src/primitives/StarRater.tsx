/**
 * StarRater — 5-star row bound to `photos.star_rating` (0..=5).
 * Click a star to set the rating; click the already-selected star to clear.
 * Keyboard `0`–`5` is wired at the DetailView level so it works without focus
 * on a specific star button.
 */

import { Icon } from './Icon';

export interface StarRaterProps {
  rating: number;
  onChange: (rating: number) => void;
  size?: number;
}

export function StarRater({ rating, onChange, size = 14 }: StarRaterProps) {
  return (
    <div style={{ display: 'inline-flex', gap: 2, alignItems: 'center' }}>
      {[1, 2, 3, 4, 5].map((n) => {
        const on = n <= rating;
        return (
          <button
            key={n}
            type="button"
            aria-pressed={on}
            onClick={() => onChange(rating === n ? 0 : n)}
            title={`Rate ${n} star${n === 1 ? '' : 's'} (${n})`}
            style={{
              padding: 3,
              lineHeight: 0,
              color: on ? 'var(--accent)' : 'var(--fg-mute)',
            }}
          >
            <Icon name="star" size={size} />
          </button>
        );
      })}
    </div>
  );
}
