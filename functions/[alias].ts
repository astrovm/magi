import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import { getClicks, getLink } from '../modules/kvHelpers';
import type { LinkRecord } from '../modules/kvHelpers';
import { notFoundPage, previewPage, unlockPage } from '../modules/pages';
import { getResponse } from '../modules/responses';
import { readStringField } from '../modules/requestParsers';
import { verifyPassword } from '../modules/secrets';
import { needsEveryVisit, pickDestination, recordVisit, redirectTo } from '../modules/visits';

type Lookup = {
  alias: string;
  aliasHash: string;
  peek: boolean;
  record: LinkRecord | null;
};

const getAliasParam = (params: Record<'alias', string | string[] | undefined>): string | null => {
  const aliasParam = params.alias;
  return readStringField(aliasParam);
};

const lookUp = async (env: Env, aliasParam: string): Promise<Lookup | 'asset'> => {
  const alias = new Alias(aliasParam);
  const { hasDot } = alias.prepareForLookup('-');
  if (hasDot) {
    return 'asset';
  }

  const peek = alias.stripSuffix('+');
  const aliasHash = await alias.getHash();
  const record = await getLink(env.links, aliasHash);
  return { alias: alias.get(), aliasHash, peek, record };
};

type Context = Parameters<PagesFunction<Env, 'alias'>>[0];

const visit = (context: Context, lookup: Lookup, record: LinkRecord): Response => {
  const { env, request } = context;
  const target = pickDestination(record);
  const shortLink = `${new URL(request.url).host}/${lookup.alias}`;
  const tracked = needsEveryVisit(record.meta);
  if (tracked) {
    context.waitUntil(recordVisit({ env, aliasHash: lookup.aliasHash, record, shortLink, target, request }));
  }
  return redirectTo(target, !tracked);
};

export const onRequestGet: PagesFunction<Env, 'alias'> = async (context) => {
  const { env, params, request } = context;
  const aliasParam = getAliasParam(params);

  if (!aliasParam) {
    return getResponse(request, 'invalidRequest');
  }

  const lookup = await lookUp(env, aliasParam);
  if (lookup === 'asset') {
    return env.ASSETS.fetch(request);
  }

  const { record } = lookup;
  if (record === null) {
    return notFoundPage();
  }

  if (lookup.peek) {
    const clicks = await getClicks(env.links, lookup.aliasHash);
    return previewPage({ alias: lookup.alias, host: new URL(request.url).host, record, clicks });
  }

  if (record.meta.pw) {
    return unlockPage(lookup.alias);
  }

  return visit(context, lookup, record);
};

export const onRequestPost: PagesFunction<Env, 'alias'> = async (context) => {
  const { env, params, request } = context;
  const aliasParam = getAliasParam(params);
  const formFields = await request.formData();
  const password = readStringField(formFields.get('password'));

  if (!aliasParam || password === null) {
    return getResponse(request, 'invalidRequest');
  }

  const lookup = await lookUp(env, aliasParam);
  if (lookup === 'asset' || lookup.peek || lookup.record === null) {
    return notFoundPage();
  }

  const { record } = lookup;
  if (!record.meta.pw) {
    return visit(context, lookup, record);
  }

  if (!(await verifyPassword(password, record.meta.pw))) {
    return unlockPage(lookup.alias, true);
  }

  return visit(context, lookup, record);
};
