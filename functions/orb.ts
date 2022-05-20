function isAValidUrl(value: string): boolean {
    try {
        new URL(value);
        return true;
    } catch (TypeError) {
        return false;
    }
}

export async function onRequestPost({ env, request }): Promise<Response> {
    const formData: FormData = await request.formData();
    const alias: File | string = formData.get(`alias`);
    const url: File | string = formData.get(`url`);
    if (typeof alias !== 'string' || typeof url !== 'string') {
        return new Response(`the orb rejected your request.\n`);
    }
    if (!alias || alias.length < 4 || alias.length > 128 || alias.includes(`.`)) {
        return new Response(`the orb rejected your alias.\n`);
    }
    if (!url || url.length < 4 || url.length > 2048 || !isAValidUrl(url)) {
        return new Response(`the orb rejected your invalid url.\n`);
    }
    const checkIfExists: KVNamespace["get"] = await env.links.get(alias, { cacheTtl: 86400 });
    if (checkIfExists) {
        return new Response(`the orb rejected your access to rewrite this link.\n`);
    }
    await env.links.put(alias, url);
    return new Response(
        `the worm summoned your link https://magi.lol/${alias}.\n`
    );
}
