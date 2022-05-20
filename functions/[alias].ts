export async function onRequestGet({ env, params, request }): Promise<Response> {
    if (params.alias.includes('.')) {
        return env.ASSETS.fetch(request);
    }
    const destinationURL = await env.links.get(params.alias, { cacheTtl: 86400 });
    const statusCode = 301;
    return Response.redirect(destinationURL, statusCode);
}