import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
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

  it('renders the facet chips row', () => {
    render(<CatalogScreen albumId="all" />, { wrapper });
    expect(screen.getByText('All')).toBeInTheDocument();
    expect(screen.getByText('People')).toBeInTheDocument();
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
