import type { KVNamespace, PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import Url from '../modules/urlClass';

type Env = {
  links: KVNamespace;
};

type FormInputs = {
  alias: string;
  url: string;
};

const isString = (value: unknown): value is string => typeof value === 'string';

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
    return new Response('<p>the orb rejected your request.</p>');
  }

  const { alias: aliasField, url: urlField } = inputs;
  const alias = new Alias(aliasField);
  alias.replaceSpacesWith('-');
  if (alias.lengthIsGreaterThan(13312) || alias.includes('.') || alias.hasSpecialChars()) {
    return new Response('<p>the orb rejected your alias.</p>');
  }

  let url: Url;
  try {
    url = new Url(urlField);
  } catch (error) {
    return new Response('<p>the orb rejected your invalid url.</p>');
  }

  if (url.lengthIsGreaterThan(2048) || !url.isValid()) {
    return new Response('<p>the orb rejected your invalid url.</p>');
  }

  const aliasHash = await alias.getHash();

  const existingValue = await env.links.get(aliasHash, { cacheTtl: 86400 });
  if (existingValue !== null) {
    return new Response('<p>the orb rejected your access to rewrite this link.</p>');
  }

  const storedAlias = alias.get();
  await env.links.put(aliasHash, url.get());
  return new Response(
    `<p>the worm summoned your link <a href="https://s.4st.li/${storedAlias}">s.4st.li/${storedAlias}</a></p>`,
  );
};
