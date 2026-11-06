// Black-box helpers: these tests only talk to a running site over HTTP.
export const BASE = process.env.BASE_URL ?? 'http://127.0.0.1:8811';
export const TELEGRAM_SECRET = process.env.TELEGRAM_SECRET ?? 'webhook-secret';
export const BOT_NAME = 'MagiOrbBot';

let counter = 0;
const runId = Date.now().toString(36);

/** Unique per test run, so reruns against a kept database don't collide. */
export const uniqueAlias = (name: string): string => {
  counter += 1;
  return `${name}-${runId}-${counter}`;
};

/** A visitor of their own, so tests don't share rate limits. */
export const newIp = (): string => {
  counter += 1;
  return `198.51.${(counter >> 8) & 255}.${counter & 255}`;
};

export const go = (path: string, init: RequestInit = {}): Promise<Response> =>
  fetch(`${BASE}${path}`, { redirect: 'manual', ...init });

export const post = (
  path: string,
  fields: Record<string, string | Blob>,
  headers: Record<string, string> = { 'X-Orb': 'fragment' },
): Promise<Response> => {
  const body = new FormData();
  Object.entries(fields).forEach(([key, value]) => body.append(key, value));
  return go(path, { method: 'POST', body, headers: { 'CF-Connecting-IP': newIp(), ...headers } });
};

export type Created = { alias: string; key: string; html: string; status: number };

export const create = async (
  fields: Record<string, string | Blob> = {},
  headers?: Record<string, string>,
): Promise<Created> => {
  const alias = String(fields.alias ?? uniqueAlias('link'));
  const response = await post('/orb', { url: 'https://example.com/target', alias, ...fields }, headers);
  const html = await response.text();
  const key = html.match(/data-key="([^"]+)"/)?.[1] ?? '';
  return { alias, key, html, status: response.status };
};

export const createPaste = async (
  fields: Record<string, string | Blob> = {},
  headers?: Record<string, string>,
): Promise<Created> => {
  const alias = String(fields.alias ?? uniqueAlias('paste'));
  const response = await post('/paste', { text: 'hello from the orb', alias, ...fields }, headers);
  const html = await response.text();
  const key = html.match(/data-key="([^"]+)"/)?.[1] ?? '';
  return { alias, key, html, status: response.status };
};

export const telegram = (text: unknown, secret = TELEGRAM_SECRET): Promise<Response> =>
  go('/telegram', {
    method: 'POST',
    headers: { 'X-Telegram-Bot-Api-Secret-Token': secret, 'Content-Type': 'application/json' },
    body: JSON.stringify({ message: { text, chat: { id: 99 } } }),
  });

export const WS_BASE = BASE.replace(/^http/, 'ws');
