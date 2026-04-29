import { resolve } from 'node:path';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri's dev server uses a fixed port; Vite must match. See tauri.conf.json.
// Routing is Zustand-driven via `useUi` in src/state/ui.ts — no file-based
// router yet. If we ever wire TanStack Router, add `TanStackRouterVite()` here
// and create src/routes/__root.tsx + per-screen routes.
const host = process.env.TAURI_DEV_HOST ?? '127.0.0.1';

export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  // Don't rewrite `process.env.TAURI_*` — they're passed through by Tauri.
  clearScreen: false,

  server: {
    host,
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/target/**', '**/models/**', '**/node_modules/**'],
    },
    hmr: host === '127.0.0.1' ? undefined : { protocol: 'ws', host, port: 1430 },
  },

  envPrefix: ['VITE_', 'TAURI_ENV_'],

  // `react-resizable-panels` ships a dual CJS+ESM build via the modern
  // `exports` map (cjs.mjs re-export wrapper). Vite's dep optimiser
  // sometimes resolves it to the CJS variant on the first cold cache,
  // which doesn't expose named ESM exports — `import { PanelGroup }`
  // then resolves to `undefined` at runtime. Forcing inclusion in
  // optimizeDeps makes Vite always pre-bundle it as ESM.
  optimizeDeps: {
    include: ['react-resizable-panels'],
  },

  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
      '@chrome': resolve(__dirname, 'src/chrome'),
      '@primitives': resolve(__dirname, 'src/primitives'),
      '@screens': resolve(__dirname, 'src/screens'),
      '@state': resolve(__dirname, 'src/state'),
      '@tauri-cli': resolve(__dirname, 'src/tauri'),
      '@styles': resolve(__dirname, 'src/styles'),
    },
  },

  build: {
    // Windows Tauri ships WebView2 (chrome105-compatible); the non-Windows
    // fallback exists for local dev builds on mac/linux and can be modern —
    // vite 8 + rolldown-vite dropped transform support for pre-safari16.
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari16',
    minify: process.env.TAURI_ENV_DEBUG ? false : 'esbuild',
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    rollupOptions: {
      output: {
        // Id-based vendor split. React + its scheduler dep must share a chunk
        // (separating them creates a circular `vendor -> react -> vendor`).
        // @tanstack packages get their own chunk because they're sizeable and
        // stable across releases — keeps long-term browser caching effective.
        // Everything else (including small Radix primitives) stays in vendor.
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined;
          if (
            id.includes('node_modules/react/') ||
            id.includes('node_modules/react-dom/') ||
            id.includes('node_modules/scheduler/')
          ) {
            return 'react';
          }
          if (id.includes('node_modules/@tanstack/')) return 'tanstack';
          return 'vendor';
        },
      },
    },
  },
}));
