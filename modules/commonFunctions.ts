/**
 * Maximum number of characters accepted for aliases provided by users.
 * Update this value to adjust how large alias strings may be.
 */
export const MAX_ALIAS_LENGTH = 13312;

/**
 * Maximum number of characters accepted for URLs provided by users.
 * Update this value to adjust the supported URL length.
 */
export const MAX_URL_LENGTH = 2048;

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

/**
 * Hashes the provided text using MD5.
 *
 * Any errors thrown by the underlying Web Crypto API (for example, when the
 * runtime does not support the requested algorithm) are intentionally allowed
 * to propagate so callers can present accurate failure information.
 */
export const hashText = async (text: string): Promise<string> => {
  const utf8Array: Uint8Array = new TextEncoder().encode(text);
  const hashBuffer: ArrayBuffer = await crypto.subtle.digest('MD5', utf8Array);
  const hashArray: number[] = Array.from(new Uint8Array(hashBuffer));
  const hashHex: string = hashArray.map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return hashHex;
};
