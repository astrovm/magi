import { afterEach, describe, expect, mock, spyOn, test } from 'bun:test';
import { onRequestGet, onRequestHead, onRequestPost as postAlias } from '../functions/[alias]';
import { onRequestPost as postManage } from '../functions/manage';
import { onRequestGet as getOrbHandler, onRequestPost as postOrbHandler } from '../functions/orb';
import {
  countAttempt,
  CREATE_LIMIT,
  isDangerousUrl,
  isLimited,
  limitKey,
  passesTurnstile,
  SAFE_BROWSING_URL,
  TURNSTILE_VERIFY_URL,
  UNLOCK_LIMIT,
} from '../modules/guards';
import { deleteLink, getLink, putLink } from '../modules/kvHelpers';
import { MAX_FORTUNES, parseFortunes } from '../modules/linkInput';
import { COUNTDOWN_SECONDS } from '../modules/pages';
import { hashPassword } from '../modules/secrets';
import {
  fetchSpy,
  formRequest,
  HOST,
  makeEnv,
  md5,
  MemoryKV,
  sha256,
  storeLink,
  visitContext,
} from './helpers';
import type { TestEnv } from './helpers';

type Handler = (context: unknown) => Promise<Response>;
const getAlias = onRequestGet as unknown as Handler;
const headAlias = onRequestHead as unknown as Handler;
const unlockAlias = postAlias as unknown as Handler;
const getOrb = getOrbHandler as unknown as Handler;
const postOrb = postOrbHandler as unknown as Handler;
const manage = postManage as unknown as Handler;

const asKv = (kv: MemoryKV) => kv as unknown as Parameters<typeof getLink>[0];

const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

const fetchReplies = (...bodies: Array<unknown | Error>) => {
  const spy = spyOn(globalThis, 'fetch');
  for (const body of bodies) {
    if (body instanceof Error) {
      spy.mockImplementationOnce(async () => {
        throw body;
      });
    } else {
      spy.mockImplementationOnce(async () => jsonResponse(body));
    }
  }
  return spy;
};

const visitorRequest = (path: string, ip: string, init: RequestInit = {}) =>
  new Request(`${HOST}${path}`, { ...init, headers: { 'CF-Connecting-IP': ip, ...init.headers } });

const create = (env: TestEnv, fields: Record<string, string>, ip = '198.51.100.1') => {
  const body = new FormData();
  Object.entries(fields).forEach(([key, value]) => body.append(key, value));
  const request = new Request(`${HOST}/orb`, {
    method: 'POST',
    body,
    headers: { 'X-Orb': 'fragment', 'CF-Connecting-IP': ip },
  });
  return postOrb({ env, request });
};

afterEach(() => {
  mock.restore();
});

describe('GET /orb', () => {
  test('hands out the Turnstile site key', async () => {
    expect(await (await getOrb({ env: makeEnv(), request: new Request(`${HOST}/orb`) })).json()).toEqual({
      turnstileSiteKey: null,
    });
    const env = makeEnv({ TURNSTILE_SITE_KEY: 'site-key' });
    const response = await getOrb({ env, request: new Request(`${HOST}/orb`) });
    expect(response.headers.get('Cache-Control')).toBe('no-store');
    expect(await response.json()).toEqual({ turnstileSiteKey: 'site-key' });
  });

  test('checks aliases while you type', async () => {
    const env = makeEnv();
    storeLink(env, 'taken', 'https://example.com');
    const status = async (alias: string) =>
      (await getOrb({ env, request: new Request(`${HOST}/orb?alias=${encodeURIComponent(alias)}`) })).json();
    expect(await status('Fresh Spell')).toEqual({ status: 'free', alias: 'Fresh-Spell' });
    expect(await status('TAKEN')).toEqual({ status: 'taken' });
    expect(await status('no.dots')).toEqual({ status: 'invalid' });
    expect(await status('  ')).toEqual({ status: 'invalid' });
  });
});

describe('POST /orb guards', () => {
  test('slows down link spam per visitor', async () => {
    const env = makeEnv();
    for (let index = 0; index < CREATE_LIMIT.max; index += 1) {
      expect((await create(env, { url: 'https://example.com', alias: `spam-${index}` })).status).toBe(201);
    }
    const blocked = await create(env, { url: 'https://example.com', alias: 'one-more' });
    expect(blocked.status).toBe(429);
    expect(await blocked.text()).toContain('the worm is exhausted');
    expect((await create(env, { url: 'https://example.com', alias: 'someone-else' }, '203.0.113.7')).status).toBe(201);
    const limitPut = env.links.putCalls.find((call) => call.key.startsWith('limit:create:'));
    expect(limitPut?.options).toEqual({ expirationTtl: CREATE_LIMIT.window });
    expect(limitPut?.key).not.toContain('198.51.100.1');
  });

  test('asks Turnstile when it is configured', async () => {
    const env = makeEnv({ TURNSTILE_SECRET_KEY: 'secret' });
    const missing = await create(env, { url: 'https://example.com', alias: 'bot' });
    expect(missing.status).toBe(403);
    expect(await missing.text()).toContain("couldn't tell if you're human");

    const fetch = fetchReplies({ success: false }, new Error('down'), { success: true });
    const token = { 'cf-turnstile-response': 'token' };
    expect((await create(env, { url: 'https://example.com', alias: 'bot', ...token })).status).toBe(403);
    expect((await create(env, { url: 'https://example.com', alias: 'bot', ...token })).status).toBe(403);
    expect((await create(env, { url: 'https://example.com', alias: 'human', ...token })).status).toBe(201);

    const [url, init] = fetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(TURNSTILE_VERIFY_URL);
    const sent = init.body as FormData;
    expect(sent.get('secret')).toBe('secret');
    expect(sent.get('response')).toBe('token');
    expect(sent.get('remoteip')).toBe('198.51.100.1');
  });

  test('refuses links Safe Browsing flags', async () => {
    const env = makeEnv({ SAFE_BROWSING_API_KEY: 'key' });
    const fetch = fetchReplies({ matches: [{ threatType: 'SOCIAL_ENGINEERING' }] });
    const response = await create(env, { url: 'https://phish.example', alias: 'phish' });
    expect(response.status).toBe(400);
    expect(await response.text()).toContain('the orb smells something fishy');
    expect(env.links.store.has(md5('phish'))).toBe(false);
    const [url, init] = fetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${SAFE_BROWSING_URL}?key=key`);
    expect(JSON.parse(String(init.body)).threatInfo.threatEntries).toEqual([{ url: 'https://phish.example' }]);
  });

  test('checks fortune cookie links too', async () => {
    const env = makeEnv({ SAFE_BROWSING_API_KEY: 'key' });
    fetchReplies({}, { matches: [{}] });
    const response = await create(env, {
      url: 'https://fine.example',
      fortune: 'https://evil.example',
      alias: 'mixed',
    });
    expect(response.status).toBe(400);
  });

  test('fails open when Safe Browsing is down', async () => {
    const env = makeEnv({ SAFE_BROWSING_API_KEY: 'key' });
    const spy = spyOn(globalThis, 'fetch');
    spy.mockImplementationOnce(async () => new Response('nope', { status: 500 }));
    spy.mockImplementationOnce(async () => {
      throw new Error('offline');
    });
    expect((await create(env, { url: 'https://a.example', alias: 'a' })).status).toBe(201);
    expect((await create(env, { url: 'https://b.example', alias: 'b' })).status).toBe(201);
  });
});

describe('guards', () => {
  test('counts attempts per visitor with an expiry', async () => {
    const kv = new MemoryKV();
    const key = await limitKey('test', new Request(HOST));
    expect(key).toBe(`limit:test:${sha256('unknown')}`);
    expect(await isLimited(asKv(kv), key, UNLOCK_LIMIT)).toBe(false);
    for (let index = 0; index < UNLOCK_LIMIT.max; index += 1) {
      await countAttempt(asKv(kv), key, UNLOCK_LIMIT);
    }
    expect(await isLimited(asKv(kv), key, UNLOCK_LIMIT)).toBe(true);
    expect(kv.putCalls[0].options).toEqual({ expirationTtl: UNLOCK_LIMIT.window });
  });

  test('skips checks that are not configured', async () => {
    const fetch = fetchSpy();
    expect(await passesTurnstile(undefined, undefined, new Request(HOST))).toBe(true);
    expect(await isDangerousUrl(undefined, 'https://example.com')).toBe(false);
    expect(fetch).not.toHaveBeenCalled();
  });

  test('sends an empty remote IP when there is none', async () => {
    const fetch = fetchReplies({ success: true });
    expect(await passesTurnstile('secret', 'token', new Request(HOST))).toBe(true);
    expect(((fetch.mock.calls[0] as [string, RequestInit])[1].body as FormData).get('remoteip')).toBe('');
  });
});

describe('fortune cookie links', () => {
  test('parses one link per line', () => {
    expect(parseFortunes(' example.org \n\nhttps://example.net\r\n', 's.4st.li')).toEqual([
      'https://example.org',
      'https://example.net',
    ]);
    expect(parseFortunes('', 's.4st.li')).toEqual([]);
    expect(parseFortunes('ftp://nope', 's.4st.li')).toBe('invalidUrl');
    expect(parseFortunes('s.4st.li/loop', 's.4st.li')).toBe('ownTail');
    expect(parseFortunes(Array(MAX_FORTUNES + 1).fill('example.com').join('\n'), 's.4st.li')).toBe('badSpell');
  });

  test('stores every destination and picks one per visit', async () => {
    const env = makeEnv();
    const response = await create(env, {
      url: 'https://a.example',
      fortune: 'b.example\nc.example',
      alias: 'cookie',
    });
    expect(response.status).toBe(201);
    expect(env.links.store.get(md5('cookie'))).toBe('https://a.example\nhttps://b.example\nhttps://c.example');

    const random = spyOn(Math, 'random').mockReturnValue(0.99);
    const visit = await getAlias(visitContext(env, 'cookie').context);
    random.mockRestore();
    expect(visit.status).toBe(302);
    expect(visit.headers.get('Location')).toBe('https://c.example');
  });

  test('rejects a bad fortune list', async () => {
    const env = makeEnv();
    expect((await create(env, { url: 'https://a.example', fortune: 'nope nope', alias: 'x' })).status).toBe(400);
    const blob = new FormData();
    blob.append('url', 'https://a.example');
    blob.append('fortune', new Blob(['x']));
    const request = new Request(`${HOST}/orb`, { method: 'POST', body: blob });
    expect((await postOrb({ env, request })).status).toBe(400);
  });

  test('previews every destination', async () => {
    const env = makeEnv();
    env.links.store.set(md5('cookie'), 'https://a.example\nhttps://b.example');
    const html = await (await getAlias(visitContext(env, 'cookie+').context)).text();
    expect(html).toContain('🥠 goes to one of');
    expect(html).toContain('https://a.example</a><br /><a');
    expect(html).toContain('https://b.example');
  });

  test('keeps the fortunes when transmuting', async () => {
    const env = makeEnv();
    env.links.store.set(md5('cookie'), 'https://a.example\nhttps://b.example');
    env.links.metadata.set(md5('cookie'), { key: sha256('k') });
    const response = await manage({
      env,
      request: formRequest('/manage', { alias: 'cookie', key: 'k', action: 'transmute', url: 'z.example' }),
    });
    expect(response.status).toBe(200);
    expect(env.links.store.get(md5('cookie'))).toBe('https://z.example\nhttps://b.example');
  });

  test('refuses dangerous transmutations', async () => {
    const env = makeEnv({ SAFE_BROWSING_API_KEY: 'key' });
    storeLink(env, 'mine', 'https://example.com', { key: sha256('k') });
    fetchReplies({ matches: [{}] });
    const response = await manage({
      env,
      request: formRequest('/manage', { alias: 'mine', key: 'k', action: 'transmute', url: 'evil.example' }),
    });
    expect(response.status).toBe(400);
    expect(env.links.store.get(md5('mine'))).toBe('https://example.com');
  });
});

describe('countdown links', () => {
  test('make visitors wait before redirecting', async () => {
    const env = makeEnv();
    const response = await create(env, { url: 'https://example.com/slow', alias: 'slow', wait: 'on' });
    expect(response.status).toBe(201);
    expect((env.links.metadata.get(md5('slow')) as { wait?: boolean }).wait).toBe(true);

    const page = await getAlias(visitContext(env, 'slow').context);
    const html = await page.text();
    expect(page.status).toBe(200);
    expect(page.headers.get('Refresh')).toBe(`${COUNTDOWN_SECONDS}; url=https://example.com/slow`);
    expect(page.headers.get('Cache-Control')).toBe('no-store');
    expect(html).toContain('data-target="https://example.com/slow"');
    expect(html).toContain('charging the orb');
  });

  test('answer HEAD with the redirect', async () => {
    const env = makeEnv();
    storeLink(env, 'slow', 'https://example.com/slow', { wait: true });
    const request = new Request(`${HOST}/slow`, { method: 'HEAD' });
    const response = await headAlias(visitContext(env, 'slow', request).context);
    expect(response.status).toBe(302);
    expect(response.headers.get('Location')).toBe('https://example.com/slow');
  });
});

describe('countdown preview', () => {
  test('previews the countdown spell', async () => {
    const env = makeEnv();
    storeLink(env, 'slowpoke', 'https://example.com', { wait: true });
    const html = await (await getAlias(visitContext(env, 'slowpoke+').context)).text();
    expect(html).toContain('seconds of suspense');
  });

  test('drops the visit counter on banish', async () => {
    const kv = new MemoryKV();
    kv.store.set('abc', 'https://example.com');
    kv.store.set('clicks:abc', '3');
    await deleteLink(asKv(kv), 'abc');
    expect(kv.store.size).toBe(0);
  });
});

describe('precise self-destruct', () => {
  test('treats a used-up link as gone even if the lookup is cached', async () => {
    const env = makeEnv();
    storeLink(env, 'boom', 'https://example.com', { max: 2 });
    env.links.store.set(`clicks:${md5('boom')}`, '2');
    const { context, pending } = visitContext(env, 'boom');
    const response = await getAlias(context);
    expect(response.status).toBe(404);
    await Promise.all(pending);
    expect(env.links.store.has(md5('boom'))).toBe(false);
  });
});

describe('password lockout', () => {
  test('locks a visitor out after too many wrong guesses', async () => {
    const env = makeEnv();
    storeLink(env, 'vault', 'https://example.com/secret', { pw: await hashPassword('hunter2') });
    const guess = (password: string, ip = '198.51.100.1') => {
      const body = new FormData();
      body.append('password', password);
      const request = visitorRequest('/vault', ip, { method: 'POST', body });
      return unlockAlias(visitContext(env, 'vault', request).context);
    };

    for (let index = 0; index < UNLOCK_LIMIT.max; index += 1) {
      expect((await guess('nope')).status).toBe(403);
    }
    const locked = await guess('hunter2');
    expect(locked.status).toBe(429);
    expect(await locked.text()).toContain('too many wrong guesses');
    expect((await guess('hunter2', '203.0.113.7')).status).toBe(302);
  });
});

describe('link storage', () => {
  test('round-trips fortune cookie links', async () => {
    const kv = new MemoryKV();
    await putLink(asKv(kv), 'f', { url: 'https://a.example', more: ['https://b.example'], meta: {} });
    expect(await getLink(asKv(kv), 'f')).toEqual({ url: 'https://a.example', more: ['https://b.example'], meta: {} });
  });
});

describe('QR codes', () => {
  test('come with every new link', async () => {
    const html = await (await create(makeEnv(), { url: 'https://example.com', alias: 'qr' })).text();
    expect(html).toContain('<details class="qr">');
    expect(html).toContain('<svg xmlns="http://www.w3.org/2000/svg"');
    expect(html).toContain('data-alias="qr" data-short="https://s.4st.li/qr" data-key="');
  });
});
