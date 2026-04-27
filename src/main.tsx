import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app';
import { revokeRemovedThumbnailQueryBlobs } from './state/queryInvalidation';
import './styles/tokens.css';
import './styles/global.css';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Don't retry on error — Tauri IPC errors are usually deterministic.
      retry: false,
      staleTime: 30_000,
    },
  },
});
revokeRemovedThumbnailQueryBlobs(queryClient);

const rootEl = document.getElementById('root');
if (!rootEl) {
  throw new Error('Missing #root element in index.html');
}

ReactDOM.createRoot(rootEl).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
