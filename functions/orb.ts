export async function onRequestPost({ env, request }) {
    const formData = await request.formData();
    const params = {};
    for (const entry of formData.entries()) {
        params[entry[0]] = entry[1];
    }
    const newLink = await env.links.put(params.alias, params.url);
    return new Response(newLink);
}