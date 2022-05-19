export async function onRequestPost({ env, request }) {
    const formData = await request.formData();
    const alias = await formData.get('alias');
    const url = await formData.get('url');
    if (alias && alias.length > 0 && alias.length <= 128 && url && url.length > 0 && url.length <= 2048) {
        await env.links.put(alias, url);
        return new Response(`the worm summoned your link https://magi.lol/${alias}.\n`);
    }
    return new Response("the orb rejected your request.\n");
}