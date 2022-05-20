const isAValidUrl = (input: string): boolean => {
    try {
        new URL(input);
        return true;
    } catch (TypeError) {
        return false;
    }
}

const hasSpecialChars = (input: string): boolean => {
    if (input === encodeURIComponent(input)) {
        return false;
    } else {
        return true;
    }
}

export const onRequestPost = async ({ env, request }): Promise<Response> => {
    const formData: FormData = await request.formData();
    const alias: File | string = formData.get(`alias`);
    const url: File | string = formData.get(`url`);
    if (!alias || typeof alias !== 'string' || !url || typeof url !== 'string') {
        return new Response(`the orb rejected your request.\n`);
    }
    const aliasReplaceSpaces: string = alias.replace(/\s/g, '-');
    if (alias.length < 4 || alias.length > 128 || alias.includes(`.`) || hasSpecialChars(aliasReplaceSpaces)) {
        return new Response(`the orb rejected your alias.\n`);
    }
    if (url.length < 4 || url.length > 2048 || !isAValidUrl(url)) {
        return new Response(`the orb rejected your invalid url.\n`);
    }
    const aliasLowerCase: string = aliasReplaceSpaces.toLowerCase();
    const checkIfExists: KVNamespace["get"] = await env.links.get(aliasLowerCase, { cacheTtl: 86400 });
    if (checkIfExists) {
        return new Response(`the orb rejected your access to rewrite this link.\n`);
    }
    await env.links.put(aliasLowerCase, url);
    return new Response(
        `the worm summoned your link https://magi.lol/${aliasReplaceSpaces}\n`
    );
}
