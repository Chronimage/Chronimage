import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { CullBinScreen } from './CullBinScreen';
import { CullBinSidePanel } from './CullBinSidePanel';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

function photoFixture(id: number) {
  return {
    id,
    sha256: String(id).padStart(64, '0'),
    filename: `IMG_${id}.jpg`,
    width: 6000,
    height: 4000,
    captured_at: '2026-04-01T12:00:00Z',
    imported_at: '2026-04-22T10:00:00Z',
    is_raw: false,
    size_bytes: 8_000_000,
    camera_make: 'Sony',
    camera_model: 'ILCE-7M4',
    aperture: 2.8,
    shutter: '1/500',
    iso: 400,
    focal_mm: 50,
    aesthetic_score: 7.5,
    paired_photo_id: null,
    raw_format: null,
    orientation: 1,
    sharpness_score: 450,
  };
}

describe('CullBinScreen', () => {
  it('shows the empty state when no rejected photos exist', async () => {
    render(<CullBinScreen />, { wrapper });
    expect(await screen.findByText(/nothing rejected/i)).toBeInTheDocument();
  });

  it('renders a row per rejected photo with reject reason chip', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'list_photos') return [photoFixture(1), photoFixture(2)];
      return undefined;
    });

    render(<CullBinScreen />, { wrapper });

    expect(await screen.findByText(/recoverable/i)).toBeInTheDocument();
    expect(await screen.findByText('IMG_1.jpg')).toBeInTheDocument();
  });
});

describe('CullBinSidePanel', () => {
  it('toggles active filter on click', () => {
    render(<CullBinSidePanel />);
    const nearDupes = screen.getByRole('button', { name: /near-duplicates/i });
    fireEvent.click(nearDupes);
    expect(nearDupes).toHaveAttribute('aria-pressed', 'true');
  });
});
