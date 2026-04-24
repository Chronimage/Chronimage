import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { App } from './app';

describe('App shell', () => {
  it('renders the titlebar brand split into head + italic tail', () => {
    const { container } = render(<App />);
    const brand = container.querySelector('.titlebar .brand');
    expect(brand).not.toBeNull();
    // Design splits app name into two spans: head + italic tail (default 'Chronimage').
    expect(brand?.textContent).toBe('Chronimage');
    expect(brand?.querySelector('em')?.textContent).toBe('ge');
  });

  it('renders the rail with all 6 primary screen buttons plus Settings', () => {
    const { container } = render(<App />);
    const buttons = container.querySelectorAll('.rail button');
    // 6 primary items (Catalog, People, Cull, Cull Bin, Map, Develop) + Settings
    expect(buttons.length).toBe(7);
  });

  it('renders the status bar', () => {
    const { container } = render(<App />);
    expect(container.querySelector('.statusbar')).not.toBeNull();
  });
});
