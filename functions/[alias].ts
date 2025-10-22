import type { KVNamespace, PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import { LINK_CACHE_TTL } from '../modules/commonFunctions';
import { isString } from '../modules/typeGuards';

type Env = {
  links: KVNamespace;
};

const getAliasParam = (params: Record<'alias', string | string[] | undefined>): string | null => {
  const aliasParam = params.alias;
  if (isString(aliasParam)) {
    return aliasParam;
  }

  return null;
};

export const onRequestGet: PagesFunction<Env, 'alias'> = async ({ env, params, request }) => {
  const aliasParam = getAliasParam(params);

  if (!aliasParam) {
    return new Response('the orb rejected your request.\n');
  }

  const alias = new Alias(aliasParam);
  alias.normalize();
  if (alias.includes('.')) {
    return env.ASSETS.fetch(request);
  }

  alias.replaceSpacesWith('-');
  const aliasHash = await alias.getHash();

  const destinationURL = await env.links.get(aliasHash, { cacheTtl: LINK_CACHE_TTL });
  if (destinationURL === null) {
    return new Response('the orb didn\'t found this link.\n');
  }

  const statusCode = 301;
  return Response.redirect(destinationURL, statusCode);
};
