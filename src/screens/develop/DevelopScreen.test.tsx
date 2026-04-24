import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { DevelopScreen } from './DevelopScreen';
import { DevelopSidePanel } from './DevelopSidePanel';

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
    filename: `IMG_${id}.ARW`,
    width: 7008,
    height: 4672,
    captured_at: '2026-04-01T12:00:00Z',
    imported_at: '2026-04-22T10:00:00Z',
    is_raw: true,
    size_bytes: 42_000_000,
    camera_make: 'Sony',
    camera_model: 'ILCE-7M4',
    aperture: 2.8,
    shutter: '1/500',
    iso: 400,
    focal_mm: 50,
    aesthetic_score: 7.5,
    paired_photo_id: null,
    raw_format: 'ARW',
    orientation: 1,
    sharpness_score: 620,
    ...overrides,
  };
}

describe('DevelopScreen', () => {
  it('renders the pick-a-photo empty state when no catalog photos exist', async () => {
    render(<DevelopScreen />, { wrapper });
    expect(await screen.findByText(/pick a photo to develop/i)).toBeInTheDocument();
  });

  it('renders the develop stage + inspector when a photo is available', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'list_photos') return [photoFixture(1)];
      return undefined;
    });

    render(<DevelopScreen />, { wrapper });

    expect(await screen.findByText(/exposure/i)).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /auto light/i })).toBeInTheDocument();
  });

  it('auto-light sets default values on click', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'list_photos') return [photoFixture(1)];
      return undefined;
    });

    render(<DevelopScreen />, { wrapper });

    const autoLight = await screen.findByRole('button', { name: /auto light/i });
    fireEvent.click(autoLight);
    // Exposure slider should now read +12 after auto-light preset.
    expect(await screen.findByText('+12 EV')).toBeInTheDocument();
  });
});

describe('DevelopSidePanel', () => {
  it('switches preset category on click', () => {
    render(<DevelopSidePanel />, { wrapper });
    const sceneBtn = screen.getByRole('button', { name: 'Scene' });
    fireEvent.click(sceneBtn);
    expect(sceneBtn).toHaveAttribute('aria-pressed', 'true');
  });

  it('shows the empty-state message in the custom-presets tab when none exist', () => {
    render(<DevelopSidePanel />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: /my presets/i }));
    expect(screen.getByText(/no custom presets yet/i)).toBeInTheDocument();
  });
});
