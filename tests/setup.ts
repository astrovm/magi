import { createHash } from 'node:crypto';

// Cloudflare Workers support the non-standard MD5 digest; Bun's Web Crypto does not.
const originalDigest = crypto.subtle.digest.bind(crypto.subtle);

crypto.subtle.digest = (async (algorithm: AlgorithmIdentifier, data: BufferSource) => {
  const name = typeof algorithm === 'string' ? algorithm : algorithm.name;
  if (name.toUpperCase() !== 'MD5') {
    return originalDigest(algorithm, data);
  }
  const bytes = ArrayBuffer.isView(data)
    ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
    : new Uint8Array(data);
  const digest = createHash('md5').update(bytes).digest();
  return digest.buffer.slice(digest.byteOffset, digest.byteOffset + digest.byteLength);
}) as typeof crypto.subtle.digest;
