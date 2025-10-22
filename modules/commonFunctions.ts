/**
 * Time-to-live in seconds for cached alias lookups in KV storage.
 * Update this value to adjust how long link resolutions stay cached.
 */
export const LINK_CACHE_TTL = 86400;

export const isAValidUrl = (input: string): boolean => {
  try {
    const url = new URL(input);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch (error) {
    if (error instanceof TypeError) {
      return false;
    }
    throw error;
  }
};

export const hasSpecialChars = (input: string): boolean => input !== encodeURIComponent(input);

export const hashText = async (text: string): Promise<string> => {
  try {
    const utf8Array: Uint8Array = new TextEncoder().encode(text);
    const hashBuffer: ArrayBuffer = await crypto.subtle.digest('MD5', utf8Array);
    const hashArray: number[] = Array.from(new Uint8Array(hashBuffer));
    const hashHex: string = hashArray.map((byte) => byte.toString(16).padStart(2, '0')).join('');
    return hashHex;
  } catch (error) {
    if (error instanceof Error) {
      throw error;
    }
    throw new Error('Error hashing text');
  }
};
