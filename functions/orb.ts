import Alias from '../modules/aliasClass';
import Url from '../modules/urlClass';

type RequestData = { env: any, request: Request };

function isString(value: any): value is string {
  return typeof value === 'string';
}

async function validateInputs(formFields: FormData): Promise<[string, string] | null> {
  const aliasField = formFields.get('alias');
  const urlField = formFields.get('url');

  if (isString(aliasField) && isString(urlField)) {
    return [aliasField, urlField];
  }

  return null;
}

export const onRequestPost = async ({ env, request }: RequestData): Promise<Response> => {
  const formFields: FormData = await request.formData();
  const inputs = await validateInputs(formFields);

  if (!inputs) {
    return new Response('the orb rejected your request.\n');
  }

  const [aliasField, urlField] = inputs;
  const alias: Alias = new Alias(aliasField);
  alias.replaceSpacesWith('-');
  if (alias.lengthIsGreaterThan(13312) || alias.includes('.') || alias.hasSpecialChars()) {
    return new Response('the orb rejected your alias.\n');
  }

  const url = new Url(urlField);
  if (url.lengthIsGreaterThan(2048) || !url.isValid()) {
    return new Response('the orb rejected your invalid url.\n');
  }

  const aliasHash: string = await alias.getHash();

  const checkIfExists: KVNamespace['get'] = await env.links.get(aliasHash, { cacheTtl: 86400 });
  if (checkIfExists) {
    return new Response('the orb rejected your access to rewrite this link.\n');
  }

  await env.links.put(aliasHash, url.get());
  return new Response(
    `the worm summoned your link https://magi.pm/${alias.get()}\n`,
  );
};
