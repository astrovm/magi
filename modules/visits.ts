import type { Env } from './cloudflareEnv';
import { addClick, deleteLink } from './kvHelpers';
import type { LinkRecord } from './kvHelpers';
import { sendTelegramMessage, visitAlertText } from './telegram';

const RICKROLL_URL = 'https://www.youtube.com/watch?v=dQw4w9WgXcQ';

/**
 * Chance that a cursed link sends the visitor to Rick instead.
 */
const CURSE_CHANCE = 0.1;

const pickDestination = (record: LinkRecord, random: () => number = Math.random): string => {
  if (record.meta.cursed && random() < CURSE_CHANCE) {
    return RICKROLL_URL;
  }
  const destinations = [record.url, ...(record.more ?? [])];
  return destinations[Math.floor(random() * destinations.length)];
};

/**
 * Links with these spells must hit the worker on every visit,
 * so browsers can't cache their redirects.
 */
const needsEveryVisit = (record: LinkRecord): boolean => {
  const { meta } = record;
  return Boolean(meta.count || meta.max || meta.cursed || meta.tg || meta.pw || meta.wait || record.more);
};

type Visit = {
  env: Env;
  aliasHash: string;
  record: LinkRecord;
  shortLink: string;
  target: string;
  request: Request;
};

const countVisit = async ({ env, aliasHash, record }: Visit): Promise<void> => {
  const { meta } = record;
  if (!meta.count && !meta.max) {
    return;
  }
  const clicks = await addClick(env.links, aliasHash, meta);
  if (meta.max && clicks >= meta.max) {
    await deleteLink(env.links, aliasHash);
  }
};

const snitchVisit = async ({ env, record, shortLink, target, request }: Visit): Promise<void> => {
  const chatId = record.meta.tg;
  if (!chatId || !env.TELEGRAM_BOT_TOKEN) {
    return;
  }
  await sendTelegramMessage(env.TELEGRAM_BOT_TOKEN, chatId, visitAlertText({ shortLink, target, request }));
};

const recordVisit = async (visit: Visit): Promise<void> => {
  await Promise.all([countVisit(visit), snitchVisit(visit)]);
};

const redirectTo = (url: string, cacheable: boolean): Response =>
  new Response(null, {
    status: cacheable ? 301 : 302,
    headers: {
      Location: url,
      'Cache-Control': cacheable ? 'public, max-age=300' : 'no-store',
    },
  });

export { CURSE_CHANCE, needsEveryVisit, pickDestination, recordVisit, redirectTo, RICKROLL_URL };
export type { Visit };
