import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ConfirmDialog } from './ConfirmDialog';

describe('<ConfirmDialog />', () => {
  it('renders no dialog content when closed', () => {
    render(
      <ConfirmDialog open={false} title="Test" confirmLabel="Go" onCancel={() => {}} onConfirm={() => {}} />,
    );
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('renders title + description when open', () => {
    render(
      <ConfirmDialog
        open
        title="Remove 3 photos?"
        description="metadata gone"
        confirmLabel="Remove 3 photos"
        onCancel={() => {}}
        onConfirm={() => {}}
      />,
    );
    expect(screen.getByText('Remove 3 photos?')).toBeInTheDocument();
    expect(screen.getByText('metadata gone')).toBeInTheDocument();
    expect(screen.getByText('Remove 3 photos')).toBeInTheDocument();
  });

  it('invokes onCancel when cancel clicked', () => {
    const onCancel = vi.fn();
    render(<ConfirmDialog open title="T" confirmLabel="OK" onCancel={onCancel} onConfirm={() => {}} />);
    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('passes selected option ids to onConfirm', () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open
        title="T"
        confirmLabel="OK"
        options={[
          { id: 'a', label: 'opt a', defaultChecked: true },
          { id: 'b', label: 'opt b', defaultChecked: false },
        ]}
        onCancel={() => {}}
        onConfirm={onConfirm}
      />,
    );

    fireEvent.click(screen.getByText('OK'));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    const firstCall = onConfirm.mock.calls[0]?.[0] as Set<string>;
    expect(firstCall.has('a')).toBe(true);
    expect(firstCall.has('b')).toBe(false);
  });

  it('ESC key triggers onCancel via Radix dialog dismissal', () => {
    const onCancel = vi.fn();
    render(<ConfirmDialog open title="T" confirmLabel="OK" onCancel={onCancel} onConfirm={() => {}} />);
    // Radix listens for Escape on the dialog content; firing via the
    // dialog element ensures the close path runs.
    fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('disables buttons + suppresses ESC when busy', () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(<ConfirmDialog open title="T" confirmLabel="OK" busy onCancel={onCancel} onConfirm={onConfirm} />);
    expect((screen.getByText('Cancel') as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByText('Working…') as HTMLButtonElement).disabled).toBe(true);
    fireEvent.keyDown(screen.getByRole('alertdialog'), { key: 'Escape' });
    expect(onCancel).not.toHaveBeenCalled();
  });
});
