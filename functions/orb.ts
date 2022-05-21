import Alias from '../modules/aliasClass';
import Url from '../modules/urlClass';

export const onRequestPost = async ({ env, request }): Promise<Response> => {
  const formFields: FormData = await request.formData();
  const aliasField: File | string = formFields.get('alias');
  const urlField: File | string = formFields.get('url');

  if (!aliasField || typeof aliasField !== 'string' || !urlField || typeof urlField !== 'string') {
    return new Response('the orb rejected your request.\n');
  }

  const alias: Alias = new Alias(aliasField);
  alias.replaceSpacesWith('-');
  if (alias.lenghtIsGreaterThan(13312) || alias.includes('.') || alias.hasSpecialChars()) {
    return new Response('the orb rejected your alias.\n');
  }

  const url = new Url(urlField);
  if (url.lenghtIsGreaterThan(2048) || !url.isAValidUrl()) {
    return new Response('the orb rejected your invalid url.\n');
  }

  const aliasHash: string = await alias.getHash();

  const checkIfExists: KVNamespace['get'] = await env.links.get(aliasHash, { cacheTtl: 86400 });
  if (checkIfExists) {
    return new Response('the orb rejected your access to rewrite this link.\n');
  }

  await env.links.put(aliasHash, url.get());
  return new Response(
    `the worm summoned your link https://magi.lol/${alias.get()}\n`,
  );
};
