import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import { deleteLink, getLink, putLink } from '../modules/kvHelpers';
import { parseTarget } from '../modules/linkInput';
import { getResponse } from '../modules/responses';
import { readStringField } from '../modules/requestParsers';
import { sha256Hex, timingSafeEqual } from '../modules/secrets';

export const onRequestPost: PagesFunction<Env> = async ({ env, request }) => {
  const formFields = await request.formData();
  const aliasField = readStringField(formFields.get('alias'));
  const keyField = readStringField(formFields.get('key'));
  const action = formFields.get('action');

  if (!aliasField || !keyField || (action !== 'transmute' && action !== 'banish')) {
    return getResponse(request, 'invalidRequest');
  }

  const alias = new Alias(aliasField);
  alias.prepareForLookup('-');
  const aliasHash = await alias.getHash();
  const record = await getLink(env.links, aliasHash);
  if (record === null) {
    return getResponse(request, 'linkNotFound');
  }

  if (!record.meta.key || !timingSafeEqual(await sha256Hex(keyField.trim()), record.meta.key)) {
    return getResponse(request, 'wrongKey');
  }

  if (action === 'banish') {
    await deleteLink(env.links, aliasHash);
    return getResponse(request, 'linkBanished');
  }

  const url = parseTarget(readStringField(formFields.get('url')) ?? '', new URL(request.url).host);
  if (typeof url === 'string') {
    return getResponse(request, url);
  }

  await putLink(env.links, aliasHash, { url: url.get(), meta: record.meta });
  return getResponse(request, 'linkTransmuted');
};
