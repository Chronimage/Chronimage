import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { describe, expect, it } from 'vitest';
import { Thumbnail } from './Thumbnail';

function renderWithQuery(ui: React.ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(React.createElement(QueryClientProvider, { client }, ui));
}

describe('<Thumbnail />', () => {
  it('falls back to <Placeholder> while the blob URL is unavailable', async () => {
    // The global invoke mock throws for `get_thumbnail`, so `useThumbnailUrl`
    // resolves to undefined and <Thumbnail> renders the <Placeholder> path.
    renderWithQuery(<Thumbnail photoId={1} photo={{ hue: 120, filename: 'IMG_0001.JPG', id: '1' }} />);
    await waitFor(() => expect(document.querySelector('.ph')).toBeInTheDocument());
    // No <img> rendered when the thumbnail isn't ready.
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('applies the selected class when `selected` is true', async () => {
    renderWithQuery(<Thumbnail photoId={2} photo={{ hue: 0, filename: 'IMG_0002.JPG', id: '2' }} selected />);
    // The `selected` class lands on either the <Placeholder> or the real image wrapper.
    await waitFor(() => {
      const el = document.querySelector('.ph');
      expect(el?.className).toContain('selected');
    });
  });
});
