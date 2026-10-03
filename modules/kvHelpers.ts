import type { KVNamespace } from '@cloudflare/workers-types';

/**
 * Time-to-live in seconds for cached alias lookups in KV storage.
 * Kept short so edits, banishments and self-destructs show up quickly.
 */
const LINK_CACHE_TTL = 300;

/**
 * Extra spells stored as KV metadata next to the destination URL.
 * Links created before these existed have no metadata at all.
 * Keys are short because KV metadata is capped at 1024 bytes.
 */
type LinkMeta = {
  /** Creation time in epoch milliseconds. */
  created?: number;
  /** SHA-256 of the manage key. */
  key?: string;
  /** PBKDF2 password hash as `salt.hash`. */
  pw?: string;
  /** Self-destruct after this many visits. */
  max?: number;
  /** Count visits. */
  count?: boolean;
  /** Sometimes rickroll visitors. */
  cursed?: boolean;
  /** Expiration in epoch seconds. */
  exp?: number;
  /** Telegram chat that gets visit alerts. */
  tg?: number;
};

type LinkRecord = {
  url: string;
  meta: LinkMeta;
};

const clicksKey = (aliasHash: string): string => `clicks:${aliasHash}`;

const getLink = async (links: KVNamespace, aliasHash: string): Promise<LinkRecord | null> => {
  const { value, metadata } = await links.getWithMetadata<LinkMeta>(aliasHash, {
    cacheTtl: LINK_CACHE_TTL,
  });
  if (value === null) {
    return null;
  }
  return { url: value, meta: metadata ?? {} };
};

const putLink = (links: KVNamespace, aliasHash: string, record: LinkRecord): Promise<void> => {
  const { meta } = record;
  const options = meta.exp ? { expiration: meta.exp, metadata: meta } : { metadata: meta };
  return links.put(aliasHash, record.url, options);
};

const deleteLink = async (links: KVNamespace, aliasHash: string): Promise<void> => {
  await Promise.all([links.delete(aliasHash), links.delete(clicksKey(aliasHash))]);
};

const getClicks = async (links: KVNamespace, aliasHash: string): Promise<number> => {
  const value = await links.get(clicksKey(aliasHash));
  return Number(value) || 0;
};

/**
 * Bumps the visit counter. KV has no atomic increment, so concurrent visits
 * can be undercounted. The orb is bad at math and has made peace with it.
 */
const addClick = async (links: KVNamespace, aliasHash: string, meta: LinkMeta): Promise<number> => {
  const clicks = (await getClicks(links, aliasHash)) + 1;
  const options = meta.exp ? { expiration: meta.exp } : {};
  await links.put(clicksKey(aliasHash), String(clicks), options);
  return clicks;
};

export { addClick, clicksKey, deleteLink, getClicks, getLink, LINK_CACHE_TTL, putLink };
export type { LinkMeta, LinkRecord };
