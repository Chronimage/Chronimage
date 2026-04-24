import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { CullScreen } from './CullScreen';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

function photoFixture(id: number, overrides: Record<string, unknown> = {}) {
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
    ...overrides,
  };
}

describe('CullScreen', () => {
  it('renders the empty state when no cull pairs can be built', async () => {
    render(<CullScreen />, { wrapper });
    expect(await screen.findByText(/nothing to cull/i)).toBeInTheDocument();
  });

  it('renders the compare view with a pair built from live photos', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'list_photos') {
        return [photoFixture(1), photoFixture(2)];
      }
      return undefined;
    });

    render(<CullScreen />, { wrapper });

    expect(await screen.findByText(/accept ai verdict/i)).toBeInTheDocument();
    expect(await screen.findByText(/pair 1 of/i)).toBeInTheDocument();
  });
});
