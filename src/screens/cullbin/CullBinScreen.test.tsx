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

function binRow(id: number, reason = 'user') {
  return {
    photo_id: id,
    filename: `IMG_${id}.jpg`,
    rejected_at: '2026-04-26T10:00:00Z',
    reason,
    retention_days: 30,
    permanent_delete_after: '2026-05-26T10:00:00Z',
    size_bytes: 8_000_000,
    sha256: String(id).padStart(64, '0'),
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
      if (cmd === 'cull_bin_list') return [binRow(1), binRow(2, 'blur')];
      if (cmd === 'cull_bin_summary') return { total_count: 2, total_bytes: 16_000_000, by_reason: [] };
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
