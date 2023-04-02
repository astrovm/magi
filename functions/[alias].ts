import Alias from '../modules/aliasClass';

type RequestData = { env: any, params: Params, request: Request };

function isString(value: any): value is string {
  return typeof value === 'string';
}

async function validateAliasParam(params: Params): Promise<string | null> {
  if (isString(params.alias)) {
    return params.alias;
  }

  return null;
}

export const onRequestGet = async ({ env, params, request }: RequestData): Promise<Response> => {
  const aliasParam = await validateAliasParam(params);

  if (!aliasParam) {
    return new Response('the orb rejected your request.\n');
  }

  const alias: Alias = new Alias(aliasParam);
  if (alias.includes('.')) {
    return env.ASSETS.fetch(request);
  }

  alias.decode();
  alias.replaceSpacesWith('-');
  const aliasHash: string = await alias.getHash();

  const destinationURL: string = await env.links.get(aliasHash, { cacheTtl: 86400 });
  if (!destinationURL) {
    return new Response('the orb didn\'t found this link.\n');
  }

  const statusCode = 301;
  return Response.redirect(destinationURL, statusCode);
};
