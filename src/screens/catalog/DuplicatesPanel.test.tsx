import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { describe, expect, it, vi } from 'vitest';
import { DuplicatesPanel } from './DuplicatesPanel';

function renderWithQuery(ui: React.ReactElement) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(React.createElement(QueryClientProvider, { client }, ui));
}

describe('<DuplicatesPanel />', () => {
  it('renders the empty-state copy when no duplicate groups are returned', async () => {
    const onClose = vi.fn();
    renderWithQuery(<DuplicatesPanel onClose={onClose} />);
    await waitFor(() => {
      expect(screen.getByText(/No near-duplicates detected/i)).toBeInTheDocument();
    });
  });

  it('renders the Duplicates heading and 0-group counter', async () => {
    renderWithQuery(<DuplicatesPanel onClose={() => undefined} />);
    await waitFor(() => {
      expect(screen.getByText(/Duplicates/)).toBeInTheDocument();
    });
    expect(screen.getByText(/0 GROUPS/i)).toBeInTheDocument();
  });

  it('invokes onClose when the Back button is clicked', async () => {
    const onClose = vi.fn();
    renderWithQuery(<DuplicatesPanel onClose={onClose} />);
    await waitFor(() => expect(screen.getByText(/Back/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/Back/));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('invokes onClose when the user presses Escape', async () => {
    const onClose = vi.fn();
    renderWithQuery(<DuplicatesPanel onClose={onClose} />);
    await waitFor(() => expect(screen.getByText(/Duplicates/)).toBeInTheDocument());
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
