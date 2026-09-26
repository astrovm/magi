import { describe, expect, spyOn, test } from 'bun:test';
import Alias from '../modules/aliasClass';
import {
  MAX_ALIAS_LENGTH,
  MAX_URL_LENGTH,
  hasSpecialChars,
  hashText,
  isAValidUrl,
} from '../modules/commonFunctions';
import { getCachedLink, LINK_CACHE_TTL } from '../modules/kvHelpers';
import { readStringField } from '../modules/requestParsers';
import {
  createResponse,
  getResponse,
  HTML_HEADERS,
  ORB_LINK_NOT_FOUND_MESSAGE,
  TEXT_HEADERS,
} from '../modules/responses';
import { isString } from '../modules/typeGuards';
import Url from '../modules/urlClass';
import { MemoryKV, md5 } from './helpers';

describe('commonFunctions', () => {
  test('exposes the input limits', () => {
    expect(MAX_ALIAS_LENGTH).toBe(13312);
    expect(MAX_URL_LENGTH).toBe(2048);
  });

  test('accepts only http and https URLs', () => {
    expect(isAValidUrl('https://example.com/path')).toBe(true);
    expect(isAValidUrl('http://example.com')).toBe(true);
    expect(isAValidUrl('ftp://example.com')).toBe(false);
    expect(isAValidUrl('javascript:alert(1)')).toBe(false);
    expect(isAValidUrl('not a url')).toBe(false);
  });

  test('rethrows unexpected URL parsing errors', () => {
    const OriginalURL = globalThis.URL;
    const failure = new RangeError('boom');
    globalThis.URL = class {
      constructor() {
        throw failure;
      }
    } as unknown as typeof URL;
    try {
      expect(() => isAValidUrl('https://example.com')).toThrow(failure);
    } finally {
      globalThis.URL = OriginalURL;
    }
  });

  test('detects characters that need URL encoding', () => {
    expect(hasSpecialChars('plain-alias_1')).toBe(false);
    expect(hasSpecialChars('with space')).toBe(true);
    expect(hasSpecialChars('ñandú')).toBe(true);
    expect(hasSpecialChars('a/b')).toBe(true);
  });

  test('hashes text with MD5 as lowercase hex', async () => {
    expect(await hashText('')).toBe('d41d8cd98f00b204e9800998ecf8427e');
    expect(await hashText('magi')).toBe(md5('magi'));
  });

  test('leaves other digest algorithms to Web Crypto', async () => {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode('magi'));
    expect(Buffer.from(digest).toString('hex')).toBe(
      new Bun.CryptoHasher('sha256').update('magi').digest('hex'),
    );
  });

  test('propagates digest failures', async () => {
    const failure = new Error('unsupported');
    const digest = spyOn(crypto.subtle, 'digest').mockRejectedValueOnce(failure);
    await expect(hashText('x')).rejects.toBe(failure);
    digest.mockRestore();
  });
});

describe('Alias', () => {
  test('normalizes whitespace and percent-encoding', () => {
    const alias = new Alias('  my%20%20cool \t link ');
    expect(alias.normalize().get()).toBe('my cool link');
    expect(alias.toString()).toBe('my cool link');
  });

  test('keeps malformed percent-encoding as-is', () => {
    expect(new Alias('bad%E0%A4%A').decode().get()).toBe('bad%E0%A4%A');
  });

  test('rethrows unexpected decoding errors', () => {
    const failure = new TypeError('boom');
    const decode = spyOn(globalThis, 'decodeURIComponent').mockImplementationOnce(() => {
      throw failure;
    });
    expect(() => new Alias('x').decode()).toThrow(failure);
    decode.mockRestore();
  });

  test('prepares aliases for lookup', () => {
    const dotted = new Alias('style.css');
    expect(dotted.prepareForLookup('-')).toEqual({ hasDot: true });

    const spaced = new Alias('hello%20big world');
    expect(spaced.prepareForLookup('-')).toEqual({ hasDot: false });
    expect(spaced.get()).toBe('hello-big-world');
  });

  test('replaces spaces and reports special characters', () => {
    const alias = new Alias('a b');
    expect(alias.hasSpecialChars()).toBe(true);
    alias.replaceSpacesWith('_');
    expect(alias.get()).toBe('a_b');
    expect(alias.hasSpecialChars()).toBe(false);
    expect(alias.hasDot()).toBe(false);
  });

  test('compares lengths', () => {
    const alias = new Alias('abc');
    expect(alias.lengthIsGreaterThan(2)).toBe(true);
    expect(alias.lengthIsGreaterThan(3)).toBe(false);
  });

  test('hashes case-insensitively', async () => {
    expect(await new Alias('HeLLo').getHash()).toBe(md5('hello'));
    expect(await new Alias('HeLLo').getHash()).toBe(await new Alias('hello').getHash());
  });
});

describe('Url', () => {
  test('wraps valid URLs', () => {
    const url = new Url('https://example.com/a');
    expect(url.get()).toBe('https://example.com/a');
    expect(url.lengthIsGreaterThan(5)).toBe(true);
  });

  test('rejects invalid URLs', () => {
    expect(() => new Url('mailto:someone@example.com')).toThrow('Invalid URL provided');
  });
});

describe('request parsing', () => {
  test('reads only string fields', () => {
    expect(isString('x')).toBe(true);
    expect(isString(1)).toBe(false);
    expect(readStringField('value')).toBe('value');
    expect(readStringField(['value'])).toBeNull();
    expect(readStringField(null)).toBeNull();
    expect(readStringField(undefined)).toBeNull();
  });
});

describe('kvHelpers', () => {
  test('reads links with the cache TTL', async () => {
    const kv = new MemoryKV();
    kv.store.set('hash', 'https://example.com');
    const links = kv as unknown as Parameters<typeof getCachedLink>[0];
    expect(await getCachedLink(links, 'hash')).toBe('https://example.com');
    expect(await getCachedLink(links, 'missing')).toBeNull();
    expect(kv.getCalls[0]).toEqual({ key: 'hash', options: { cacheTtl: LINK_CACHE_TTL } });
    expect(LINK_CACHE_TTL).toBe(86400);
  });
});

describe('responses', () => {
  test.each([
    ['invalidRequestHtml', '<p>the orb rejected your request.</p>', HTML_HEADERS],
    ['invalidRequestText', 'the orb rejected your request.\n', TEXT_HEADERS],
    ['invalidAlias', '<p>the orb rejected your alias.</p>', HTML_HEADERS],
    ['invalidUrl', '<p>the orb rejected your invalid url.</p>', HTML_HEADERS],
    ['aliasLocked', '<p>the orb rejected your access to rewrite this link.</p>', HTML_HEADERS],
    ['linkNotFound', `${ORB_LINK_NOT_FOUND_MESSAGE}\n`, TEXT_HEADERS],
  ] as const)('builds %s', async (key, body, headers) => {
    const response = getResponse(key);
    expect(await response.text()).toBe(body);
    expect(response.headers.get('Content-Type')).toBe(headers['Content-Type']);
  });

  test('builds the link-created response with the alias', async () => {
    const response = getResponse('linkCreated', { alias: 'orb' });
    expect(await response.text()).toBe(
      '<p>the worm summoned your link <a href="https://s.4st.li/orb">s.4st.li/orb</a></p>',
    );
  });

  test('requires an alias for the link-created response', () => {
    expect(() => getResponse('linkCreated')).toThrow('requires an alias');
    expect(() => getResponse('linkCreated', { alias: '' })).toThrow('requires an alias');
  });

  test('creates responses with a formatter and headers', async () => {
    const response = createResponse('hi', (message) => `[${message}]`, TEXT_HEADERS);
    expect(await response.text()).toBe('[hi]');
    expect(response.headers.get('Content-Type')).toBe('text/plain; charset=utf-8');
  });
});
