import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it } from 'vitest';
import { CatalogScreen } from './CatalogScreen';
import { CatalogSidePanel } from './CatalogSidePanel';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

describe('CatalogScreen', () => {
  it('renders without crashing with albumId=all', () => {
    render(<CatalogScreen albumId="all" />, { wrapper });
    expect(screen.getByPlaceholderText(/ask your library/i)).toBeInTheDocument();
  });

  it('renders the empty-state source picker when there are no sources + no photos', () => {
    render(<CatalogScreen albumId="all" />, { wrapper });
    // The source picker is the first thing a fresh user sees. The mode
    // chooser was removed when copy-to-catalog became the only mode;
    // now the picker leads with a policy banner + three action buttons.
    expect(screen.getByText(/CHRONIMAGE ALWAYS COPIES/i)).toBeInTheDocument();
    expect(screen.getByText(/Add a folder/i)).toBeInTheDocument();
  });

  it('density toggle exposes three independently selectable buttons', () => {
    render(<CatalogScreen albumId="all" />, { wrapper });
    const compact = screen.getByRole('button', { name: /compact density/i });
    const comfortable = screen.getByRole('button', { name: /comfortable density/i });
    const spacious = screen.getByRole('button', { name: /spacious density/i });

    // Default tweaks ship with `compact`, so only the compact button is on.
    expect(compact).toHaveAttribute('aria-pressed', 'true');
    expect(comfortable).toHaveAttribute('aria-pressed', 'false');
    expect(spacious).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(comfortable);
    expect(compact).toHaveAttribute('aria-pressed', 'false');
    expect(comfortable).toHaveAttribute('aria-pressed', 'true');
    expect(spacious).toHaveAttribute('aria-pressed', 'false');

    fireEvent.click(spacious);
    expect(compact).toHaveAttribute('aria-pressed', 'false');
    expect(comfortable).toHaveAttribute('aria-pressed', 'false');
    expect(spacious).toHaveAttribute('aria-pressed', 'true');
  });
});

describe('CatalogSidePanel', () => {
  it('renders without crashing', () => {
    render(<CatalogSidePanel albumId="all" onAlbumChange={() => undefined} />, { wrapper });
    // "All photos" label is always present in the side panel
    expect(screen.getByText(/all photos/i)).toBeInTheDocument();
  });

  it('renders the Smart Albums section header', () => {
    render(<CatalogSidePanel albumId="all" onAlbumChange={() => undefined} />, { wrapper });
    expect(screen.getByText(/smart albums/i)).toBeInTheDocument();
  });
});
