import { mock, spyOn } from 'bun:test';
import type { LinkMeta } from '../modules/kvHelpers';

type PutOptions = { expiration?: number; expirationTtl?: number; metadata?: unknown };

export class MemoryKV {
  readonly store: Map<string, string>;

  readonly metadata: Map<string, unknown>;

  readonly getCalls: Array<{ key: string; options: unknown }>;

  readonly putCalls: Array<{ key: string; value: string; options: PutOptions | undefined }>;

  constructor() {
    this.store = new Map();
    this.metadata = new Map();
    this.getCalls = [];
    this.putCalls = [];
  }

  async get(key: string, options?: unknown): Promise<string | null> {
    this.getCalls.push({ key, options });
    return this.store.get(key) ?? null;
  }

  async getWithMetadata(key: string, options?: unknown): Promise<{ value: string | null; metadata: unknown }> {
    this.getCalls.push({ key, options });
    return { value: this.store.get(key) ?? null, metadata: this.metadata.get(key) ?? null };
  }

  async put(key: string, value: string, options?: PutOptions): Promise<void> {
    this.putCalls.push({ key, value, options });
    this.store.set(key, value);
    if (options?.metadata !== undefined) {
      this.metadata.set(key, options.metadata);
    }
  }

  async delete(key: string): Promise<void> {
    this.store.delete(key);
    this.metadata.delete(key);
  }
}

export const md5 = (text: string): string => new Bun.CryptoHasher('md5').update(text).digest('hex');

export const sha256 = (text: string): string => new Bun.CryptoHasher('sha256').update(text).digest('hex');

export const assetResponse = new Response('asset');
export const HOST = 'https://s.4st.li';

export type TestEnv = {
  links: MemoryKV;
  ASSETS: { fetch: ReturnType<typeof mock> };
  TELEGRAM_BOT_TOKEN?: string;
  TELEGRAM_BOT_USERNAME?: string;
  TELEGRAM_WEBHOOK_SECRET?: string;
  TURNSTILE_SITE_KEY?: string;
  TURNSTILE_SECRET_KEY?: string;
  SAFE_BROWSING_API_KEY?: string;
};

export const makeEnv = (extra: Partial<TestEnv> = {}): TestEnv => {
  const links = new MemoryKV();
  const fetch = mock(async () => assetResponse);
  return { links, ASSETS: { fetch }, ...extra };
};

export const storeLink = (env: TestEnv, alias: string, url: string, meta?: LinkMeta): void => {
  env.links.store.set(md5(alias), url);
  if (meta) {
    env.links.metadata.set(md5(alias), meta);
  }
};

export const formRequest = (
  path: string,
  fields: Record<string, string | Blob>,
  headers: Record<string, string> = { 'X-Orb': 'fragment' },
): Request => {
  const body = new FormData();
  Object.entries(fields).forEach(([key, value]) => body.append(key, value));
  return new Request(`${HOST}${path}`, { method: 'POST', body, headers });
};

export const visitContext = (env: TestEnv, alias: string | string[] | undefined, request?: Request) => {
  const pending: Promise<unknown>[] = [];
  return {
    pending,
    context: {
      env,
      params: { alias },
      request: request ?? new Request(`${HOST}/${typeof alias === 'string' ? alias : ''}`),
      waitUntil: (promise: Promise<unknown>) => pending.push(promise),
    },
  };
};

export const fetchSpy = () => spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{}'));

