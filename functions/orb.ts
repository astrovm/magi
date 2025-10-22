import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import Url from '../modules/urlClass';
import { MAX_ALIAS_LENGTH, MAX_URL_LENGTH } from '../modules/commonFunctions';
import { getCachedLink } from '../modules/kvHelpers';
import { getResponse } from '../modules/responses';
import { readStringField } from '../modules/requestParsers';

type FormInputs = {
  alias: string;
  url: string;
};

const parseFormInputs = (formFields: FormData): FormInputs | null => {
  const aliasField = readStringField(formFields.get('alias'));
  const urlField = readStringField(formFields.get('url'));

  if (aliasField !== null && urlField !== null) {
    return {
      alias: aliasField,
      url: urlField,
    };
  }

  return null;
};

export const onRequestPost: PagesFunction<Env> = async ({ env, request }) => {
  const formFields = await request.formData();
  const inputs = parseFormInputs(formFields);

  if (!inputs) {
    return getResponse('invalidRequestHtml');
  }

  const { alias: aliasField, url: urlField } = inputs;
  const alias = new Alias(aliasField);
  const { hasDot } = alias.prepareForLookup('-');
  if (alias.lengthIsGreaterThan(MAX_ALIAS_LENGTH) || hasDot || alias.hasSpecialChars()) {
    return getResponse('invalidAlias');
  }

  let url: Url;
  try {
    url = new Url(urlField);
  } catch (error) {
    return getResponse('invalidUrl');
  }

  if (url.lengthIsGreaterThan(MAX_URL_LENGTH)) {
    return getResponse('invalidUrl');
  }

  const aliasHash = await alias.getHash();

  const existingValue = await getCachedLink(env.links, aliasHash);
  if (existingValue !== null) {
    return getResponse('aliasLocked');
  }

  const storedAlias = alias.get();
  await env.links.put(aliasHash, url.get());
  return getResponse('linkCreated', { alias: storedAlias });
};
