import type { KVNamespace } from '@cloudflare/workers-types';

/**
 * Time-to-live in seconds for cached alias lookups in KV storage.
 * Update this value to adjust how long link resolutions stay cached.
 */
const LINK_CACHE_TTL = 86400;

const getCachedLink = (
  links: KVNamespace,
  aliasHash: string,
): Promise<string | null> => links.get(aliasHash, { cacheTtl: LINK_CACHE_TTL });

export { getCachedLink, LINK_CACHE_TTL };
