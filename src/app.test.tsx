import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { App } from './app';

function renderApp() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <App />
    </QueryClientProvider>,
  );
}

describe('App shell', () => {
  it('renders the titlebar brand split into head + italic tail', () => {
    const { container } = renderApp();
    const brand = container.querySelector('.titlebar .brand');
    expect(brand).not.toBeNull();
    // Design splits app name into two spans: head + italic tail (default 'Chronimage').
    expect(brand?.textContent).toBe('Chronimage');
    expect(brand?.querySelector('em')?.textContent).toBe('ge');
  });

  it('renders the rail with the primary screen buttons plus Settings', () => {
    const { container } = renderApp();
    const buttons = container.querySelectorAll('.rail button');
    // Catalog, People, Cull, Develop + Settings
    expect(buttons.length).toBe(5);
  });

  it('renders the status bar', () => {
    const { container } = renderApp();
    expect(container.querySelector('.statusbar')).not.toBeNull();
  });
});
