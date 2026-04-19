import type { Config } from 'tailwindcss';

// Tailwind v4 is CSS-first; most theme lives in tokens.css via @theme directive.
// This file only lists content globs so Tailwind can tree-shake classes.
export default {
  content: ['./index.html', './src/**/*.{ts,tsx,css}'],
} satisfies Config;
