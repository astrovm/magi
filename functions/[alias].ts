export const onRequestGet = async ({ env, params, request }): Promise<Response> => {
    const alias: string = params.alias;
    if (alias.includes('.')) {
        return env.ASSETS.fetch(request);
    }
    if (alias.length > 256) {
        return new Response(`the orb rejected your link.\n`);
    }
    const aliasEncoded: string = decodeURIComponent(alias).toLowerCase().replace(/\s/g, '-');
    const destinationURL: string = await env.links.get(aliasEncoded, { cacheTtl: 86400 });
    const statusCode: number = 301;
    return Response.redirect(destinationURL, statusCode);
}