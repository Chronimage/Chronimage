/**
 * Typed logging wrapper. Use this instead of `console.log` — lefthook's
 * forbidden-patterns check bans raw console usage in app code.
 *
 * In dev, logs are also pushed to Loki (http://localhost:3100) when available.
 * Loki shipping is fire-and-forget — failures are silently swallowed.
 */

const IS_DEV = import.meta.env.DEV;
const LOKI_URL = 'http://localhost:3100/loki/api/v1/push';

type Level = 'debug' | 'info' | 'warn' | 'error';

function lokiPush(level: Level, message: string): void {
  if (!IS_DEV) return;
  const nowNs = (BigInt(Date.now()) * 1_000_000n).toString();
  const body = JSON.stringify({
    streams: [
      {
        stream: { app: 'chronimage', env: 'dev', layer: 'frontend', level },
        values: [[nowNs, message]],
      },
    ],
  });
  fetch(LOKI_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
  }).catch(() => {
    // Loki not running — ignore silently
  });
}

function fmt(level: Level, args: unknown[]): string {
  return `[chronimage:${level}] ${args.map((a) => (typeof a === 'object' ? JSON.stringify(a) : String(a))).join(' ')}`;
}

export function debug(...args: unknown[]): void {
  if (!IS_DEV) return;
  const msg = fmt('debug', args);
  // biome-ignore lint/suspicious/noConsole: app-wide debug wrapper
  console.debug(msg);
  lokiPush('debug', msg);
}

export function info(...args: unknown[]): void {
  const msg = fmt('info', args);
  // biome-ignore lint/suspicious/noConsole: app-wide info wrapper
  console.info(msg);
  lokiPush('info', msg);
}

export function warn(...args: unknown[]): void {
  const msg = fmt('warn', args);
  console.warn(msg);
  lokiPush('warn', msg);
}

export function error(...args: unknown[]): void {
  const msg = fmt('error', args);
  console.error(msg);
  lokiPush('error', msg);
}
