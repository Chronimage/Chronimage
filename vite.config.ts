import { resolve } from 'node:path';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri's dev server uses a fixed port; Vite must match. See tauri.conf.json.
// TanStack Router plugin is deferred until Phase 1 when we switch from the
// Zustand-driven screen state to file-based routes. When added back, create
// src/routes/__root.tsx + per-screen routes first.
const host = process.env.TAURI_DEV_HOST ?? '127.0.0.1';

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  // Don't rewrite `process.env.TAURI_*` — they're passed through by Tauri.
  clearScreen: false,

  server: {
    host,
    port: 1420,
    strictPort: true,
    watch: {
      // Don't crawl these
      ignored: ['**/src-tauri/target/**', '**/design-handoff/**', '**/models/**', '**/node_modules/**'],
    },
    hmr: host === '127.0.0.1' ? undefined : { protocol: 'ws', host, port: 1430 },
  },

  envPrefix: ['VITE_', 'TAURI_ENV_'],

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
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react-dom'],
          router: ['@tanstack/react-router'],
          radix: [
            '@radix-ui/react-dialog',
            '@radix-ui/react-dropdown-menu',
            '@radix-ui/react-popover',
            '@radix-ui/react-scroll-area',
            '@radix-ui/react-slider',
            '@radix-ui/react-tabs',
            '@radix-ui/react-tooltip',
          ],
        },
      },
    },
  },
}));
