import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import { getCachedLink } from '../modules/kvHelpers';
import { getResponse } from '../modules/responses';
import { readStringField } from '../modules/requestParsers';

const getAliasParam = (params: Record<'alias', string | string[] | undefined>): string | null => {
  const aliasParam = params.alias;
  return readStringField(aliasParam);
};

export const onRequestGet: PagesFunction<Env, 'alias'> = async ({ env, params, request }) => {
  const aliasParam = getAliasParam(params);

  if (!aliasParam) {
    return getResponse('invalidRequestText');
  }

  const alias = new Alias(aliasParam);
  const { hasDot } = alias.prepareForLookup('-');
  if (hasDot) {
    return env.ASSETS.fetch(request);
  }

  const aliasHash = await alias.getHash();
  const destinationURL = await getCachedLink(env.links, aliasHash);
  if (destinationURL === null) {
    return getResponse('linkNotFound');
  }

  const statusCode = 301;
  return Response.redirect(destinationURL, statusCode);
};
