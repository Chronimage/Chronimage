import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import React from 'react';
import { describe, expect, it } from 'vitest';
import { OnboardScreen } from './OnboardScreen';

function wrapper({ children }: { children: React.ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return React.createElement(QueryClientProvider, { client }, children);
}

describe('OnboardScreen', () => {
  it('renders the welcome heading', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByText(/Your photos/i)).toBeInTheDocument();
  });

  it('renders the add local folder button', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByRole('button', { name: /add local folder/i })).toBeInTheDocument();
  });

  it('renders the Google Photos connector button', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByRole('button', { name: /google photos/i })).toBeInTheDocument();
  });

  it('renders the iCloud connector button', () => {
    render(<OnboardScreen />, { wrapper });
    // Either "iCloud Photos" or "iCloud (detected)"
    expect(screen.getByRole('button', { name: /icloud/i })).toBeInTheDocument();
  });

  it('renders the iPhone USB button (disabled when no device)', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByRole('button', { name: /iphone usb/i })).toBeDisabled();
  });

  it('renders the WELCOME label', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByText('WELCOME')).toBeInTheDocument();
  });
});
