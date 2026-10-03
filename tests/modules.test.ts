import { describe, expect, spyOn, test } from 'bun:test';
import Alias from '../modules/aliasClass';
import {
  MAX_ALIAS_LENGTH,
  MAX_URL_LENGTH,
  RESERVED_ALIASES,
  hasSpecialChars,
  hashText,
  isAValidUrl,
  withScheme,
} from '../modules/commonFunctions';
import { addClick, clicksKey, deleteLink, getClicks, getLink, LINK_CACHE_TTL, putLink } from '../modules/kvHelpers';
import { escapeHtml, renderPage } from '../modules/html';
import { readStringField } from '../modules/requestParsers';
import { getResponse, linkCreated, wantsFragment } from '../modules/responses';
import { hashPassword, PASSWORD_ITERATIONS, randomToken, sha256Hex, timingSafeEqual, verifyPassword } from '../modules/secrets';
import { describeDevice, describePlace, parseTelegramCommand } from '../modules/telegram';
import { isString } from '../modules/typeGuards';
import Url from '../modules/urlClass';
import { CURSE_CHANCE, needsEveryVisit, pickDestination, redirectTo, RICKROLL_URL } from '../modules/visits';
import { ADJECTIVES, NOUNS, summonWizardWords } from '../modules/wizardWords';
import { MemoryKV, md5, sha256 } from './helpers';

describe('commonFunctions', () => {
  test('exposes the input limits', () => {
    expect(MAX_ALIAS_LENGTH).toBe(13312);
    expect(MAX_URL_LENGTH).toBe(2048);
    expect(RESERVED_ALIASES).toEqual(['orb', 'manage', 'telegram']);
  });

  test('adds https to scheme-less input', () => {
    expect(withScheme('  example.com/a ')).toBe('https://example.com/a');
    expect(withScheme('http://example.com')).toBe('http://example.com');
    expect(withScheme('ftp://example.com')).toBe('ftp://example.com');
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

  test('knows reserved aliases and peek suffixes', () => {
    expect(new Alias('ORB').isReserved()).toBe(true);
    expect(new Alias('orbs').isReserved()).toBe(false);
    const peek = new Alias('cool+');
    expect(peek.stripSuffix('+')).toBe(true);
    expect(peek.get()).toBe('cool');
    expect(peek.stripSuffix('+')).toBe(false);
    expect(peek.get()).toBe('cool');
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

  test('builds from loose input and reports its host', () => {
    const url = Url.fromInput('Example.com:8080/x');
    expect(url.get()).toBe('https://Example.com:8080/x');
    expect(url.getHost()).toBe('example.com:8080');
    expect(String(url)).toBe('https://Example.com:8080/x');
    expect(() => Url.fromInput('javascript:alert(1)')).toThrow('Invalid URL provided');
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
  const asKv = (kv: MemoryKV) => kv as unknown as Parameters<typeof getLink>[0];

  test('reads links with metadata and the cache TTL', async () => {
    const kv = new MemoryKV();
    kv.store.set('legacy', 'https://example.com');
    kv.store.set('new', 'https://example.org');
    kv.metadata.set('new', { count: true });
    expect(await getLink(asKv(kv), 'legacy')).toEqual({ url: 'https://example.com', meta: {} });
    expect(await getLink(asKv(kv), 'new')).toEqual({ url: 'https://example.org', meta: { count: true } });
    expect(await getLink(asKv(kv), 'missing')).toBeNull();
    expect(kv.getCalls[0]).toEqual({ key: 'legacy', options: { cacheTtl: LINK_CACHE_TTL } });
    expect(LINK_CACHE_TTL).toBe(300);
  });

  test('writes links with expiration only when they expire', async () => {
    const kv = new MemoryKV();
    await putLink(asKv(kv), 'a', { url: 'https://a.example', meta: {} });
    await putLink(asKv(kv), 'b', { url: 'https://b.example', meta: { exp: 123 } });
    expect(kv.putCalls.map((call) => call.options)).toEqual([
      { metadata: {} },
      { expiration: 123, metadata: { exp: 123 } },
    ]);
  });

  test('counts clicks and deletes everything', async () => {
    const kv = new MemoryKV();
    expect(clicksKey('abc')).toBe('clicks:abc');
    expect(await getClicks(asKv(kv), 'abc')).toBe(0);
    expect(await addClick(asKv(kv), 'abc', {})).toBe(1);
    expect(await addClick(asKv(kv), 'abc', { exp: 99 })).toBe(2);
    expect(kv.putCalls.map((call) => call.options)).toEqual([{}, { expiration: 99 }]);
    kv.store.set('abc', 'https://example.com');
    await deleteLink(asKv(kv), 'abc');
    expect(kv.store.size).toBe(0);
  });
});

describe('secrets', () => {
  test('makes url-safe random tokens', () => {
    const token = randomToken();
    expect(token).toMatch(/^[\w-]{22}$/);
    expect(randomToken()).not.toBe(token);
    expect(randomToken(3)).toHaveLength(4);
  });

  test('hashes with SHA-256', async () => {
    expect(await sha256Hex('magi')).toBe(sha256('magi'));
  });

  test('hashes and verifies passwords', async () => {
    expect(PASSWORD_ITERATIONS).toBe(50000);
    const stored = await hashPassword('hunter2');
    expect(stored).not.toBe(await hashPassword('hunter2'));
    expect(await verifyPassword('hunter2', stored)).toBe(true);
    expect(await verifyPassword('hunter3', stored)).toBe(false);
    expect(await verifyPassword('hunter2', 'garbage')).toBe(false);
    expect(await verifyPassword('hunter2', '.')).toBe(false);
  });

  test('compares strings in constant time', () => {
    expect(timingSafeEqual('abc', 'abc')).toBe(true);
    expect(timingSafeEqual('abc', 'abd')).toBe(false);
    expect(timingSafeEqual('abc', 'ab')).toBe(false);
  });
});

describe('wizard words', () => {
  test('summons adjective-noun-number aliases', () => {
    expect(summonWizardWords(() => 0)).toBe(`${ADJECTIVES[0]}-${NOUNS[0]}-0`);
    expect(summonWizardWords(() => 0.999)).toBe(`${ADJECTIVES.at(-1)}-${NOUNS.at(-1)}-99`);
    expect(summonWizardWords()).toMatch(/^[a-z]+-[a-z]+-\d{1,2}$/);
  });

  test('only uses url-safe words', () => {
    for (const word of [...ADJECTIVES, ...NOUNS]) {
      expect(word).toMatch(/^[a-z]+$/);
    }
  });
});

describe('visits', () => {
  test('picks the destination, sometimes Rick', () => {
    const record = { url: 'https://example.com', meta: { cursed: true } };
    expect(pickDestination(record, () => CURSE_CHANCE - 0.01)).toBe(RICKROLL_URL);
    expect(pickDestination(record, () => CURSE_CHANCE)).toBe('https://example.com');
    expect(pickDestination({ url: 'https://example.com', meta: {} }, () => 0)).toBe('https://example.com');
    const random = spyOn(Math, 'random').mockReturnValue(0.99);
    expect(pickDestination(record)).toBe('https://example.com');
    random.mockRestore();
  });

  test('knows which links need every visit', () => {
    expect(needsEveryVisit({})).toBe(false);
    expect(needsEveryVisit({ created: 1, key: 'k', exp: 2 })).toBe(false);
    for (const meta of [{ count: true }, { max: 1 }, { cursed: true }, { tg: 1 }, { pw: 'x' }]) {
      expect(needsEveryVisit(meta)).toBe(true);
    }
  });

  test('builds cacheable and private redirects', () => {
    const cached = redirectTo('https://example.com', true);
    expect(cached.status).toBe(301);
    expect(cached.headers.get('Cache-Control')).toBe('public, max-age=300');
    const fresh = redirectTo('https://example.com', false);
    expect(fresh.status).toBe(302);
    expect(fresh.headers.get('Location')).toBe('https://example.com');
  });
});

describe('telegram helpers', () => {
  test.each([
    ['Mozilla/5.0 (Windows NT 10.0) Chrome/130.0 Safari/537.36', 'Chrome on Windows'],
    ['Mozilla/5.0 (Windows NT 10.0) Chrome/130.0 Safari/537.36 Edg/130.0', 'Edge on Windows'],
    ['Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko Firefox/130.0', 'Firefox on Linux'],
    ['Mozilla/5.0 (Linux; Android 15) Chrome/130.0 Mobile Safari/537.36 OPR/85', 'Opera on Android'],
    ['Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15', 'Safari on macOS'],
    ['TelegramBot (like TwitterBot)', '🤖 a bot on a mystery device'],
    ['', 'a mystery browser on a mystery device'],
  ])('describes %s', (userAgent, expected) => {
    expect(describeDevice(userAgent)).toBe(expected);
  });

  test('describes places', () => {
    expect(describePlace({ city: 'Rosario', country: 'AR' })).toBe('Rosario, AR');
    expect(describePlace({ country: 'AR', city: '' })).toBe('AR');
    expect(describePlace({ city: 7 })).toBe('somewhere in the void');
    expect(describePlace(undefined)).toBe('somewhere in the void');
  });

  test('parses commands', () => {
    expect(parseTelegramCommand({ message: { text: ' /Alert@MagiOrbBot  cool  key ', chat: { id: 5 } } })).toEqual({
      chatId: 5,
      command: 'alert',
      args: ['cool', 'key'],
    });
    expect(parseTelegramCommand({ message: { text: 'hello', chat: { id: 5 } } })?.command).toBe('hello');
    expect(parseTelegramCommand({})).toBeNull();
    expect(parseTelegramCommand({ message: { text: 'hi' } })).toBeNull();
  });
});

describe('html', () => {
  test('escapes html', () => {
    expect(escapeHtml(`<a href="x">'&'</a>`)).toBe('&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;');
  });

  test('renders full pages', async () => {
    const page = renderPage('<p>hi</p>', { title: '<lost>', status: 418, headers: { 'X-Test': '1' } });
    const html = await page.text();
    expect(page.status).toBe(418);
    expect(page.headers.get('X-Test')).toBe('1');
    expect(page.headers.get('Content-Type')).toBe('text/html; charset=utf-8');
    expect(html).toContain('<title>&lt;lost&gt; | Magi</title>');
    expect(html).toContain('<section class="card"><p>hi</p></section>');
    const plain = renderPage('x');
    expect(plain.status).toBe(200);
    expect(await plain.text()).toContain('<title>Magi</title>');
  });
});

describe('responses', () => {
  const fragmentRequest = new Request('https://s.4st.li/orb', { headers: { 'X-Orb': 'fragment' } });
  const pageRequest = new Request('https://s.4st.li/orb');

  test('knows when the page script is asking', () => {
    expect(wantsFragment(fragmentRequest)).toBe(true);
    expect(wantsFragment(pageRequest)).toBe(false);
  });

  test.each([
    ['invalidRequest', 'the orb rejected your request.', 400],
    ['invalidAlias', 'the orb rejected your alias.', 400],
    ['invalidUrl', 'the orb rejected your invalid url.', 400],
    ['aliasLocked', 'the orb rejected your access to rewrite this link.', 409],
    ['ownTail', 'the orb refuses to eat its own tail.', 400],
    ['wrongKey', "the orb doesn't recognize that manage key.", 403],
    ['badSpell', 'the orb rejected your weird spell settings.', 400],
    ['noAliasLeft', 'the worm ran out of ideas. try a custom alias.', 503],
    ['linkNotFound', "the orb didn't found this link.", 404],
    ['linkTransmuted', 'the worm transmuted your link.', 200],
    ['linkBanished', 'the worm banished your link to the shadow realm.', 200],
  ] as const)('builds %s', async (key, message, status) => {
    const response = getResponse(fragmentRequest, key);
    const tone = status < 300 ? 'success' : 'error';
    expect(response.status).toBe(status);
    expect(await response.text()).toBe(`<p class="orb-msg orb-msg--${tone}">${message}</p>`);
    expect(response.headers.get('Content-Type')).toBe('text/html; charset=utf-8');
  });

  test('wraps responses in a page for plain requests', async () => {
    const response = getResponse(pageRequest, 'invalidUrl');
    expect(await response.text()).toStartWith('<!DOCTYPE html>');
  });

  test('escapes the created link', async () => {
    const html = await linkCreated(fragmentRequest, {
      shortUrl: 'https://s.4st.li/a"b',
      manageKey: '<k>',
      alias: 'a"b',
      telegramBot: 'Bot"',
    }).text();
    expect(html).not.toContain('a"b');
    expect(html).not.toContain('<k>');
    expect(html).toContain('/alert a&quot;b &lt;k&gt;');
    expect(html).toContain('@Bot&quot;');
  });
});
