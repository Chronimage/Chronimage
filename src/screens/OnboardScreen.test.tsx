import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen } from '@testing-library/react';
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
  it('renders the WELCOME TO label and app name on step 1', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByText('WELCOME TO')).toBeInTheDocument();
  });

  it('renders the caption tagline on step 1', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByText(/Your photos/i)).toBeInTheDocument();
  });

  it('renders the STEP 1 heading on the right panel', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByText(/STEP 1 · CATALOG HOME/i)).toBeInTheDocument();
  });

  it('shows all 5 step labels in the left stepper', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByText('Welcome')).toBeInTheDocument();
    expect(screen.getByText('Sources')).toBeInTheDocument();
    expect(screen.getByText('Import')).toBeInTheDocument();
    expect(screen.getByText('Models')).toBeInTheDocument();
    expect(screen.getByText('Name people')).toBeInTheDocument();
  });

  it('advances to step 2 (Sources) when Continue is clicked, showing Add local folder', () => {
    render(<OnboardScreen />, { wrapper });
    const continueBtn = screen.getByRole('button', { name: /continue/i });
    fireEvent.click(continueBtn);
    expect(screen.getByRole('button', { name: /add local folder/i })).toBeInTheDocument();
  });

  it('shows Google Photos button on step 2', () => {
    render(<OnboardScreen />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));
    expect(screen.getByRole('button', { name: /google photos/i })).toBeInTheDocument();
  });

  it('shows iCloud button on step 2', () => {
    render(<OnboardScreen />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));
    expect(screen.getByRole('button', { name: /icloud/i })).toBeInTheDocument();
  });

  it('shows disabled iPhone USB button on step 2 when no device connected', () => {
    render(<OnboardScreen />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));
    expect(screen.getByRole('button', { name: /iphone usb/i })).toBeDisabled();
  });

  it('Back button is disabled on step 1', () => {
    render(<OnboardScreen />, { wrapper });
    expect(screen.getByRole('button', { name: /back/i })).toBeDisabled();
  });

  it('Back button navigates from step 2 back to step 1', () => {
    render(<OnboardScreen />, { wrapper });
    fireEvent.click(screen.getByRole('button', { name: /continue/i }));
    expect(screen.getByText(/STEP 2 · SOURCES/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /back/i }));
    expect(screen.getByText(/STEP 1 · CATALOG HOME/i)).toBeInTheDocument();
  });
});
