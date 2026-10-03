/**
 * PBKDF2 iterations for link passwords.
 * Workers cap PBKDF2 at 100000 iterations; half keeps unlocks well inside
 * the free plan CPU budget. These guard silly short links, not bank vaults.
 */
const PASSWORD_ITERATIONS = 50000;

const toBase64Url = (bytes: Uint8Array): string =>
  btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

const fromBase64Url = (text: string): Uint8Array => {
  const base64 = text.replace(/-/g, '+').replace(/_/g, '/');
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
};

const randomToken = (byteLength = 16): string =>
  toBase64Url(crypto.getRandomValues(new Uint8Array(byteLength)));

const sha256Hex = async (text: string): Promise<string> => {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
};

const derivePasswordBits = async (password: string, salt: Uint8Array): Promise<Uint8Array> => {
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(password),
    'PBKDF2',
    false,
    ['deriveBits'],
  );
  const bits = await crypto.subtle.deriveBits(
    { name: 'PBKDF2', hash: 'SHA-256', salt, iterations: PASSWORD_ITERATIONS },
    keyMaterial,
    256,
  );
  return new Uint8Array(bits);
};

const hashPassword = async (password: string): Promise<string> => {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const hash = await derivePasswordBits(password, salt);
  return `${toBase64Url(salt)}.${toBase64Url(hash)}`;
};

const timingSafeEqual = (a: string, b: string): boolean => {
  if (a.length !== b.length) {
    return false;
  }
  let difference = 0;
  for (let index = 0; index < a.length; index += 1) {
    difference |= a.charCodeAt(index) ^ b.charCodeAt(index);
  }
  return difference === 0;
};

const verifyPassword = async (password: string, stored: string): Promise<boolean> => {
  const [salt, expected] = stored.split('.');
  if (!salt || !expected) {
    return false;
  }
  const hash = await derivePasswordBits(password, fromBase64Url(salt));
  return timingSafeEqual(toBase64Url(hash), expected);
};

export {
  PASSWORD_ITERATIONS,
  hashPassword,
  randomToken,
  sha256Hex,
  timingSafeEqual,
  verifyPassword,
};
