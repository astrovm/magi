export const isAValidUrl = (input: string): boolean => {
  try {
    const url = new URL(input);
    if (url.protocol === 'http:' || url.protocol === 'https:') {
      return true;
    }
    return false;
  } catch (TypeError) {
    return false;
  }
};

export const hasSpecialChars = (input: string): boolean => {
  if (input === encodeURIComponent(input)) {
    return false;
  }
  return true;
};

export const hashText = async (text: string): Promise<string> => {
  const msgUint8: Uint8Array = new TextEncoder().encode(text); // encode as (utf-8) Uint8Array
  const hashBuffer: ArrayBuffer = await crypto.subtle.digest('MD5', msgUint8); // hash the message
  const hashArray: number[] = Array.from(new Uint8Array(hashBuffer)); // convert buffer to bytearray
  const hashHex: string = hashArray.map((b) => b.toString(16).padStart(2, '0')).join(''); // convert bytes to hex string
  return hashHex;
};
