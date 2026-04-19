import { resolve } from 'node:path';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
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
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./tests/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}', 'tests/unit/**/*.test.{ts,tsx}'],
    exclude: ['node_modules', 'dist', 'src-tauri', 'design-handoff', 'tests/e2e', 'tests/visual-goldens'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html', 'lcov'],
      include: ['src/**/*.{ts,tsx}'],
      exclude: ['src/**/*.test.{ts,tsx}', 'src/**/*.d.ts', 'src/main.tsx', 'src/routeTree.gen.ts'],
      thresholds: {
        lines: 70,
        functions: 70,
        branches: 70,
        statements: 70,
        // Stricter floors on critical paths:
        'src/state/**': { lines: 80, branches: 75 },
        'src/tauri/**': { lines: 80, branches: 75 },
      },
    },
  },
});
