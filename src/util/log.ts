/**
 * Typed logging wrapper. Use this instead of `console.log` — lefthook's
 * forbidden-patterns check bans raw console usage in app code.
 */

const enabled = import.meta.env.DEV;

export function debug(...args: unknown[]): void {
  if (enabled) {
    // biome-ignore lint/suspicious/noConsole: app-wide debug wrapper
    console.debug('[chronimage]', ...args);
  }
}

export function info(...args: unknown[]): void {
  // biome-ignore lint/suspicious/noConsole: app-wide info wrapper
  console.info('[chronimage]', ...args);
}

export function warn(...args: unknown[]): void {
  console.warn('[chronimage]', ...args);
}

export function error(...args: unknown[]): void {
  console.error('[chronimage]', ...args);
}
