/**
 * Typed logging wrapper. Use this instead of `console.log` — lefthook's
 * forbidden-patterns check bans raw console usage in app code.
 *
 * Logs are also forwarded to the Rust backend so frontend/backend events land
 * in the same local rolling log files under the app data directory.
 */

import { frontendLog } from '../tauri/invoke';

const IS_DEV = import.meta.env.DEV;

type Level = 'debug' | 'info' | 'warn' | 'error';

function fileLog(level: Level, message: string): void {
  frontendLog(level, message).catch(() => {
    // The app may be running in a browser test without Tauri IPC.
  });
}

function fmt(level: Level, args: unknown[]): string {
  return `[chronimage:${level}] ${args.map(stringifyLogValue).join(' ')}`;
}

function stringifyLogValue(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof value !== 'object' || value === null) return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

export function debug(...args: unknown[]): void {
  if (!IS_DEV) return;
  const msg = fmt('debug', args);
  // biome-ignore lint/suspicious/noConsole: app-wide debug wrapper
  console.debug(msg);
  fileLog('debug', msg);
}

export function info(...args: unknown[]): void {
  const msg = fmt('info', args);
  // biome-ignore lint/suspicious/noConsole: app-wide info wrapper
  console.info(msg);
  fileLog('info', msg);
}

export function warn(...args: unknown[]): void {
  const msg = fmt('warn', args);
  console.warn(msg);
  fileLog('warn', msg);
}

export function error(...args: unknown[]): void {
  const msg = fmt('error', args);
  console.error(msg);
  fileLog('error', msg);
}

/**
 * Extract a human-readable string from any value thrown/rejected. Handles
 * three cases in order:
 *   1. `Error` instance → `err.message`
 *   2. Tauri command rejection → `{ code, message }` plain object (our
 *      AppError serialises this way; see `src-tauri/src/error.rs`)
 *   3. Anything else → `String(value)`
 *
 * Use this everywhere we surface errors to the UI; `String(err)` on a
 * plain object returns `"[object Object]"`, which is useless.
 */
export function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === 'object' && err !== null) {
    const e = err as { message?: unknown; code?: unknown };
    if (typeof e.message === 'string' && e.message.length > 0) return e.message;
    if (typeof e.code === 'string') return `AppError(${e.code})`;
  }
  return String(err);
}
