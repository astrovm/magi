function isAValidUrl(value: string): boolean {
    try {
        const url = new URL(value);
        return true;
    } catch (TypeError) {
        return false;
    }
}

export async function onRequestPost({ env, request }) {
    const formData = await request.formData();
    const alias = await formData.get(`alias`);
    const url = await formData.get(`url`);
    if (!isAValidUrl(url)) {
        return new Response(`the orb rejected your invalid url.\n`);
    }
    if (
        !alias ||
        alias.includes(`.`) ||
        alias.length < 4 ||
        alias.length > 128 ||
        !url ||
        url.length < 4 ||
        url.length > 2048
    ) {
        return new Response(`the orb rejected your alias.\n`);
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
