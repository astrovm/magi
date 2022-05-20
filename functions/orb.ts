function isAValidUrl(value: string): boolean {
    try {
        new URL(value);
        return true;
    } catch (TypeError) {
        return false;
    }
}

export async function onRequestPost({ env, request }): Promise<Response> {
    const formData: object = await request.formData();
    const alias: string = await formData.get(`alias`);
    const url: string = await formData.get(`url`);
    if (!alias || alias.length < 4 || alias.length > 128 || alias.includes(`.`)) {
        return new Response(`the orb rejected your alias.\n`);
    }
    if (!url || url.length < 4 || url.length > 2048 || !isAValidUrl(url)) {
        return new Response(`the orb rejected your invalid url.\n`);
    }
    const checkIfExists = await env.links.get(alias, { cacheTtl: 86400 });
    if (checkIfExists) {
        return new Response(`the orb rejected your access to rewrite this link.\n`);
    }
    await env.links.put(alias, url);
    return new Response(
        `the worm summoned your link https://magi.lol/${alias}.\n`
    );
}
