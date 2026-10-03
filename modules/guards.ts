import type { KVNamespace } from '@cloudflare/workers-types';
import { sha256Hex } from './secrets';

type Limit = {
  /** Most attempts allowed inside the window. */
  max: number;
  /** Window length in seconds. KV needs at least 60. */
  window: number;
};

/** Wrong passwords allowed per link and visitor before a lockout. */
const UNLOCK_LIMIT: Limit = { max: 5, window: 900 };

/** Links one visitor can create per hour. */
const CREATE_LIMIT: Limit = { max: 30, window: 3600 };

const visitorId = (request: Request): Promise<string> =>
  // Hashed so no raw IPs end up in storage.
  sha256Hex(request.headers.get('CF-Connecting-IP') ?? 'unknown');

const limitKey = async (scope: string, request: Request): Promise<string> =>
  `limit:${scope}:${await visitorId(request)}`;

const isLimited = async (links: KVNamespace, key: string, limit: Limit): Promise<boolean> =>
  Number(await links.get(key)) >= limit.max;

/**
 * Counts one attempt. Like the click counter, concurrent attempts can be
 * undercounted, which is fine for slowing down guessers and spammers.
 */
const countAttempt = async (links: KVNamespace, key: string, limit: Limit): Promise<void> => {
  const attempts = Number(await links.get(key)) + 1;
  await links.put(key, String(attempts), { expirationTtl: limit.window });
};

const TURNSTILE_VERIFY_URL = 'https://challenges.cloudflare.com/turnstile/v0/siteverify';

/**
 * Checks the Turnstile token from the form. Without a secret the check is off.
 */
const passesTurnstile = async (secret: string | undefined, token: unknown, request: Request): Promise<boolean> => {
  if (!secret) {
    return true;
  }
  if (typeof token !== 'string' || token === '') {
    return false;
  }
  const body = new FormData();
  body.append('secret', secret);
  body.append('response', token);
  body.append('remoteip', request.headers.get('CF-Connecting-IP') ?? '');
  try {
    const response = await fetch(TURNSTILE_VERIFY_URL, { method: 'POST', body });
    const result = await response.json<{ success?: boolean }>();
    return result.success === true;
  } catch {
    return false;
  }
};

const SAFE_BROWSING_URL = 'https://safebrowsing.googleapis.com/v4/threatMatches:find';

/**
 * Asks Google Safe Browsing about a destination. Without a key the check is off.
 * Fails open: if Google is down, links still get created.
 */
const isDangerousUrl = async (apiKey: string | undefined, url: string): Promise<boolean> => {
  if (!apiKey) {
    return false;
  }
  try {
    const response = await fetch(`${SAFE_BROWSING_URL}?key=${encodeURIComponent(apiKey)}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        client: { clientId: 'magi', clientVersion: '1' },
        threatInfo: {
          threatTypes: ['MALWARE', 'SOCIAL_ENGINEERING', 'UNWANTED_SOFTWARE', 'POTENTIALLY_HARMFUL_APPLICATION'],
          platformTypes: ['ANY_PLATFORM'],
          threatEntryTypes: ['URL'],
          threatEntries: [{ url }],
        },
      }),
    });
    if (!response.ok) {
      return false;
    }
    const result = await response.json<{ matches?: unknown[] }>();
    return Array.isArray(result.matches) && result.matches.length > 0;
  } catch {
    return false;
  }
};

export {
  countAttempt,
  CREATE_LIMIT,
  isDangerousUrl,
  isLimited,
  limitKey,
  passesTurnstile,
  SAFE_BROWSING_URL,
  TURNSTILE_VERIFY_URL,
  UNLOCK_LIMIT,
};
export type { Limit };
