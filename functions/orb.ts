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

const digestMessage = async (message: string) => {
    const msgUint8: Uint8Array = new TextEncoder().encode(message);                           // encode as (utf-8) Uint8Array
    const hashBuffer: ArrayBuffer = await crypto.subtle.digest('MD5', msgUint8);           // hash the message
    const hashArray: number[] = Array.from(new Uint8Array(hashBuffer));                     // convert buffer to byte array
    const hashHex: string = hashArray.map(b => b.toString(16).padStart(2, '0')).join(''); // convert bytes to hex string
    return hashHex;
}

export const onRequestPost = async ({ env, request }): Promise<Response> => {
    const formData: FormData = await request.formData();
    const alias: File | string = formData.get(`alias`);
    const url: File | string = formData.get(`url`);

    if (!alias || typeof alias !== 'string' || !url || typeof url !== 'string') {
        return new Response(`the orb rejected your request.\n`);
    }

    const aliasReplaceSpaces: string = alias.replace(/\s/g, '-');
    if (alias.length < 4 || alias.length > 2048 || alias.includes(`.`) || hasSpecialChars(aliasReplaceSpaces)) {
        return new Response(`the orb rejected your alias.\n`);
    }
    if (url.length > 2048 || !isAValidUrl(url)) {
        return new Response(`the orb rejected your invalid url.\n`);
    }

    const aliasLowerCase: string = aliasReplaceSpaces.toLowerCase();
    const aliasHash: string = await digestMessage(aliasLowerCase);

    const checkIfExists: KVNamespace["get"] = await env.links.get(aliasHash, { cacheTtl: 86400 });
    if (checkIfExists) {
        return new Response(`the orb rejected your access to rewrite this link.\n`);
    }
    await env.links.put(aliasHash, url);
    return new Response(
        `the worm summoned your link https://magi.lol/${aliasReplaceSpaces}\n`
    );
}
