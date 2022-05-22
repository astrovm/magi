import Alias from '../modules/aliasClass';

export const onRequestGet = async (
  { env, params, request }: { env: any, params: Params, request: Request },
): Promise<Response> => {
  if (typeof params.alias !== 'string') {
    return new Response('the orb rejected your request.\n');
  }

  const alias: Alias = new Alias(params.alias);
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
