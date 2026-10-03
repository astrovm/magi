import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import { MAX_ALIAS_LENGTH } from '../modules/commonFunctions';
import { getLink, putLink } from '../modules/kvHelpers';
import { parseSpells, parseTarget } from '../modules/linkInput';
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

export const onRequestPost: PagesFunction<Env> = async ({ env, request }) => {
  const formFields = await request.formData();
  const urlField = readStringField(formFields.get('url'));
  const aliasField = formFields.get('alias') ?? '';

  if (urlField === null || typeof aliasField !== 'string') {
    return getResponse(request, 'invalidRequest');
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

  const claimed = aliasField.trim() === ''
    ? await claimRandomAlias(env)
    : await claimCustomAlias(env, aliasField);
  if (typeof claimed === 'string') {
    return getResponse(request, claimed);
  }

  const { password, ...meta } = spells;
  const manageKey = randomToken();
  await putLink(env.links, claimed.aliasHash, {
    url: url.get(),
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
