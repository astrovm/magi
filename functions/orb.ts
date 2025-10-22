import type { KVNamespace, PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import Url from '../modules/urlClass';
import { LINK_CACHE_TTL } from '../modules/commonFunctions';
import { getResponse } from '../modules/responses';
import { isString } from '../modules/typeGuards';

type Env = {
  links: KVNamespace;
};

type FormInputs = {
  alias: string;
  url: string;
};

const parseFormInputs = (formFields: FormData): FormInputs | null => {
  const aliasField = formFields.get('alias');
  const urlField = formFields.get('url');

  if (isString(aliasField) && isString(urlField)) {
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
  alias.normalize();
  alias.replaceSpacesWith('-');
  if (alias.lengthIsGreaterThan(13312) || alias.includes('.') || alias.hasSpecialChars()) {
    return getResponse('invalidAlias');
  }

  let url: Url;
  try {
    url = new Url(urlField);
  } catch (error) {
    return getResponse('invalidUrl');
  }

  if (url.lengthIsGreaterThan(2048) || !url.isValid()) {
    return getResponse('invalidUrl');
  }

  const aliasHash = await alias.getHash();

  const existingValue = await env.links.get(aliasHash, { cacheTtl: LINK_CACHE_TTL });
  if (existingValue !== null) {
    return getResponse('aliasLocked');
  }

  const storedAlias = alias.get();
  await env.links.put(aliasHash, url.get());
  return getResponse('linkCreated', { alias: storedAlias });
};
