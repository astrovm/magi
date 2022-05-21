import { hashText } from '../modules/common';

export const onRequestGet = async ({ env, params, request }): Promise<Response> => {
  const { alias } = params;
  if (alias.includes('.')) {
    return env.ASSETS.fetch(request);
  }

  const aliasEncoded: string = decodeURIComponent(alias).toLowerCase().replace(/\s/g, '-');
  const aliasHash: string = await hashText(aliasEncoded);

  const destinationURL: string = await env.links.get(aliasHash, { cacheTtl: 86400 });
  if (!destinationURL) {
    return new Response('the orb didn\'t found this link.\n');
  }

  const statusCode = 301;
  return Response.redirect(destinationURL, statusCode);
};
