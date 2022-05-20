const digestMessage = async (message: string) => {
    const msgUint8: Uint8Array = new TextEncoder().encode(message);                           // encode as (utf-8) Uint8Array
    const hashBuffer: ArrayBuffer = await crypto.subtle.digest('MD5', msgUint8);           // hash the message
    const hashArray: number[] = Array.from(new Uint8Array(hashBuffer));                     // convert buffer to byte array
    const hashHex: string = hashArray.map(b => b.toString(16).padStart(2, '0')).join(''); // convert bytes to hex string
    return hashHex;
}

export const onRequestGet = async ({ env, params, request }): Promise<Response> => {
    const alias: string = params.alias;
    if (alias.includes('.')) {
        return env.ASSETS.fetch(request);
    }
    if (alias.length > 4096) {
        return new Response(`the orb rejected your link.\n`);
    }
    const aliasEncoded: string = decodeURIComponent(alias).toLowerCase().replace(/\s/g, '-');
    const aliasHash: string = await digestMessage(aliasEncoded);

    const destinationURL: string = await env.links.get(aliasHash, { cacheTtl: 86400 });
    const statusCode: number = 301;
    return Response.redirect(destinationURL, statusCode);
}