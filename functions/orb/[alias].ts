export async function onRequestGet({ params }) {
    const destinationURL = await params.links.get(params.alias);
    const statusCode = 301;
    return Response.redirect(destinationURL, statusCode);
}