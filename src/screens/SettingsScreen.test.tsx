import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it } from 'vitest';
import { SettingsScreen } from './SettingsScreen';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

describe('SettingsScreen', () => {
  it('renders all four section headings', () => {
    render(<SettingsScreen />, { wrapper });
    expect(screen.getByText(/identity/i)).toBeInTheDocument();
    expect(screen.getByText(/ai models/i)).toBeInTheDocument();
    expect(screen.getByText(/culling thresholds/i)).toBeInTheDocument();
    expect(screen.getByText(/storage.*indexing/i)).toBeInTheDocument();
  });

  it('shows the two mocked AI model rows', async () => {
    render(<SettingsScreen />, { wrapper });
    expect(await screen.findByText('siglip2-b16-image')).toBeInTheDocument();
    expect(await screen.findByText('scrfd-10g')).toBeInTheDocument();
  });
});
