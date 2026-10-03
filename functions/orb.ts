import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import { MAX_ALIAS_LENGTH } from '../modules/commonFunctions';
import {
  countAttempt,
  CREATE_LIMIT,
  isDangerousUrl,
  isLimited,
  limitKey,
  passesTurnstile,
} from '../modules/guards';
import { getLink, putLink } from '../modules/kvHelpers';
import { parseFortunes, parseSpells, parseTarget } from '../modules/linkInput';
import { getResponse, linkCreated } from '../modules/responses';
import type { ResponseKey } from '../modules/responses';
import { readStringField } from '../modules/requestParsers';
import { hashPassword, randomToken, sha256Hex } from '../modules/secrets';
import { summonWizardWords } from '../modules/wizardWords';

const RANDOM_ALIAS_ATTEMPTS = 5;

type ClaimedAlias = { alias: string; aliasHash: string } | ResponseKey;

const claimCustomAlias = async (env: Env, aliasField: string): Promise<ClaimedAlias> => {
  const alias = new Alias(aliasField);
  const { hasDot } = alias.prepareForLookup('-');
  if (alias.lengthIsGreaterThan(MAX_ALIAS_LENGTH) || hasDot || alias.hasSpecialChars() || alias.isReserved()) {
    return 'invalidAlias';
  }

  const aliasHash = await alias.getHash();
  if ((await getLink(env.links, aliasHash)) !== null) {
    return 'aliasLocked';
  }
  return { alias: alias.get(), aliasHash };
};

const claimRandomAlias = async (env: Env): Promise<ClaimedAlias> => {
  for (let attempt = 0; attempt < RANDOM_ALIAS_ATTEMPTS; attempt += 1) {
    const claimed = await claimCustomAlias(env, summonWizardWords());
    if (typeof claimed !== 'string') {
      return claimed;
    }
  }
  return 'noAliasLeft';
};

const json = (body: unknown): Response =>
  new Response(JSON.stringify(body), {
    headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' },
  });

type AliasStatus = 'free' | 'taken' | 'invalid';

const ALIAS_STATUS: Partial<Record<ResponseKey, AliasStatus>> = { aliasLocked: 'taken' };

/**
 * `GET /orb?alias=x` tells the form if an alias is free while you type.
 * `GET /orb` hands the page its public settings.
 */
export const onRequestGet: PagesFunction<Env> = async ({ env, request }) => {
  const aliasParam = new URL(request.url).searchParams.get('alias');
  if (aliasParam === null) {
    return json({ turnstileSiteKey: env.TURNSTILE_SITE_KEY ?? null });
  }
  if (aliasParam.trim() === '') {
    return json({ status: 'invalid' });
  }
  const claimed = await claimCustomAlias(env, aliasParam);
  if (typeof claimed === 'string') {
    return json({ status: ALIAS_STATUS[claimed] ?? 'invalid' });
  }
  return json({ status: 'free', alias: claimed.alias });
};

export const onRequestPost: PagesFunction<Env> = async ({ env, request }) => {
  const formFields = await request.formData();
  const urlField = readStringField(formFields.get('url'));
  const aliasField = formFields.get('alias') ?? '';
  const fortuneField = formFields.get('fortune') ?? '';

  if (urlField === null || typeof aliasField !== 'string' || typeof fortuneField !== 'string') {
    return getResponse(request, 'invalidRequest');
  }

  const createKey = await limitKey('create', request);
  if (await isLimited(env.links, createKey, CREATE_LIMIT)) {
    return getResponse(request, 'tooMany');
  }
  if (!(await passesTurnstile(env.TURNSTILE_SECRET_KEY, formFields.get('cf-turnstile-response'), request))) {
    return getResponse(request, 'notHuman');
  }

  const now = Date.now();
  const spells = parseSpells(formFields, now);
  if (!spells) {
    return getResponse(request, 'badSpell');
  }

  const requestUrl = new URL(request.url);
  const url = parseTarget(urlField, requestUrl.host);
  if (typeof url === 'string') {
    return getResponse(request, url);
  }
  const fortunes = parseFortunes(fortuneField, requestUrl.host);
  if (typeof fortunes === 'string') {
    return getResponse(request, fortunes);
  }
  const dangers = await Promise.all(
    [url.get(), ...fortunes].map((target) => isDangerousUrl(env.SAFE_BROWSING_API_KEY, target)),
  );
  if (dangers.includes(true)) {
    return getResponse(request, 'dangerUrl');
  }

  const claimed = aliasField.trim() === ''
    ? await claimRandomAlias(env)
    : await claimCustomAlias(env, aliasField);
  if (typeof claimed === 'string') {
    return getResponse(request, claimed);
  }

  const { password, ...meta } = spells;
  const manageKey = randomToken();
  await countAttempt(env.links, createKey, CREATE_LIMIT);
  await putLink(env.links, claimed.aliasHash, {
    url: url.get(),
    ...(fortunes.length > 0 ? { more: fortunes } : {}),
    meta: {
      ...meta,
      created: now,
      key: await sha256Hex(manageKey),
      ...(password ? { pw: await hashPassword(password) } : {}),
    },
  });

  return linkCreated(request, {
    shortUrl: `${requestUrl.origin}/${claimed.alias}`,
    manageKey,
    alias: claimed.alias,
    telegramBot: env.TELEGRAM_BOT_USERNAME,
  });
};
