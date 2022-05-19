export async function onRequestGet({ env, params, request }) {
    if (params.alias.includes('.')) {
        return env.ASSETS.fetch(request);
    }
    const destinationURL = await env.links.get(params.alias);
    const statusCode = 301;
    return Response.redirect(destinationURL, statusCode);
}