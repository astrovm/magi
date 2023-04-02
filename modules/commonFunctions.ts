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

export const hasSpecialChars = (input: string): boolean => {
  return input !== encodeURIComponent(input);
};

export const hashText = async (text: string): Promise<string> => {
  try {
    const utf8Array: Uint8Array = new TextEncoder().encode(text);
    const hashBuffer: ArrayBuffer = await crypto.subtle.digest('MD5', utf8Array);
    const hashArray: number[] = Array.from(new Uint8Array(hashBuffer));
    const hashHex: string = hashArray.map((byte) => byte.toString(16).padStart(2, '0')).join('');
    return hashHex;
  } catch (error) {
    console.error('Error hashing text:', error);
    throw error;
  }
};
