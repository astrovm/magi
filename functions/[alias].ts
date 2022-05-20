export async function onRequestGet({ env, params, request }): Promise<Response> {
    const alias: string = params.alias;
    if (alias.includes('.')) {
        return env.ASSETS.fetch(request);
    }
    if (alias.length > 128) {
        return new Response(`the orb rejected your link.\n`);
    }
    const destinationURL: string = await env.links.get(params.alias, { cacheTtl: 86400 });
    const statusCode: number = 301;
    return Response.redirect(destinationURL, statusCode);
}