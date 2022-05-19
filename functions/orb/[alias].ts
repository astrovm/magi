export async function onRequestGet({ env, params }) {
    const destinationURL = await env.links.get(params.alias);
    const statusCode = 301;
    return Response.redirect(destinationURL, statusCode);
}