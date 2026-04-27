import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { PeopleScreen } from './PeopleScreen';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

describe('PeopleScreen', () => {
  it('shows empty state when no clusters are returned', async () => {
    render(<PeopleScreen />, { wrapper });
    expect(await screen.findByText(/no face clusters yet/i)).toBeInTheDocument();
  });

  it('renders two cards when two clusters are returned', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'face_clusters_list') {
        return [
          { id: 1, name: 'Alice', isNamed: true, faceCount: 120, coverPhotoId: null },
          { id: 2, name: null, isNamed: false, faceCount: 45, coverPhotoId: null },
        ];
      }
      // fall back to defaults for other commands
      if (cmd === 'app_version') return '0.0.0-test';
      if (cmd === 'current_channel') return { channel: 'dev' };
      return undefined;
    });

    render(<PeopleScreen />, { wrapper });

    expect(await screen.findByText('Alice')).toBeInTheDocument();
    expect(await screen.findByText(/unnamed person/i)).toBeInTheDocument();
  });
});
