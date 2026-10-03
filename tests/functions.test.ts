import { afterEach, describe, expect, mock, spyOn, test } from 'bun:test';
import { onRequestGet, onRequestPost as postAlias } from '../functions/[alias]';
import { onRequestPost as postManage } from '../functions/manage';
import { onRequestPost as postOrbHandler } from '../functions/orb';
import { onRequestPost as postTelegram } from '../functions/telegram';
import { MAX_ALIAS_LENGTH, MAX_URL_LENGTH } from '../modules/commonFunctions';
import type { LinkMeta } from '../modules/kvHelpers';
import { hashPassword } from '../modules/secrets';
import { RICKROLL_URL } from '../modules/visits';
import { MemoryKV, md5, sha256 } from './helpers';

type Handler = (context: unknown) => Promise<Response>;
const getAlias = onRequestGet as unknown as Handler;
const unlockAlias = postAlias as unknown as Handler;
const postOrb = postOrbHandler as unknown as Handler;
const manage = postManage as unknown as Handler;
const telegram = postTelegram as unknown as Handler;

const assetResponse = new Response('asset');
const HOST = 'https://s.4st.li';

type TestEnv = {
  links: MemoryKV;
  ASSETS: { fetch: ReturnType<typeof mock> };
  TELEGRAM_BOT_TOKEN?: string;
  TELEGRAM_BOT_USERNAME?: string;
  TELEGRAM_WEBHOOK_SECRET?: string;
};

const makeEnv = (extra: Partial<TestEnv> = {}): TestEnv => {
  const links = new MemoryKV();
  const fetch = mock(async () => assetResponse);
  return { links, ASSETS: { fetch }, ...extra };
};

const storeLink = (env: TestEnv, alias: string, url: string, meta?: LinkMeta): void => {
  env.links.store.set(md5(alias), url);
  if (meta) {
    env.links.metadata.set(md5(alias), meta);
  }
};

const formRequest = (
  path: string,
  fields: Record<string, string | Blob>,
  headers: Record<string, string> = { 'X-Orb': 'fragment' },
): Request => {
  const body = new FormData();
  Object.entries(fields).forEach(([key, value]) => body.append(key, value));
  return new Request(`${HOST}${path}`, { method: 'POST', body, headers });
};

const visitContext = (env: TestEnv, alias: string | string[] | undefined, request?: Request) => {
  const pending: Promise<unknown>[] = [];
  return {
    pending,
    context: {
      env,
      params: { alias },
      request: request ?? new Request(`${HOST}/${typeof alias === 'string' ? alias : ''}`),
      waitUntil: (promise: Promise<unknown>) => pending.push(promise),
    },
  };
};

const fetchSpy = () => spyOn(globalThis, 'fetch').mockImplementation(async () => new Response('{}'));

afterEach(() => {
  mock.restore();
});

describe('GET /:alias', () => {
  test('redirects a plain link permanently with a short cache', async () => {
    const env = makeEnv();
    storeLink(env, 'my-link', 'https://example.com/target');
    const { context, pending } = visitContext(env, 'My%20Link');
    const response = await getAlias(context);
    expect(response.status).toBe(301);
    expect(response.headers.get('Location')).toBe('https://example.com/target');
    expect(response.headers.get('Cache-Control')).toBe('public, max-age=300');
    expect(pending).toHaveLength(0);
  });

  test('shows the lost page for a missing link', async () => {
    const env = makeEnv();
    const response = await getAlias(visitContext(env, 'nothing').context);
    expect(response.status).toBe(404);
    expect(await response.text()).toContain("the orb didn't found this link.");
  });

  test('serves static assets for dotted paths', async () => {
    const env = makeEnv();
    const request = new Request(`${HOST}/style.css`);
    const response = await getAlias(visitContext(env, 'style.css', request).context);
    expect(response).toBe(assetResponse);
    expect(env.ASSETS.fetch).toHaveBeenCalledWith(request);
    expect(env.links.getCalls).toHaveLength(0);
  });

  test('rejects missing or multi-segment aliases', async () => {
    const env = makeEnv();
    for (const alias of [undefined, '', ['a', 'b']]) {
      const response = await getAlias(visitContext(env, alias).context);
      expect(response.status).toBe(400);
      expect(await response.text()).toContain('the orb rejected your request.');
    }
  });

  test('previews a link with its stats when the alias ends in +', async () => {
    const env = makeEnv();
    storeLink(env, 'peek', 'https://example.com/<b>', {
      created: Date.UTC(2026, 9, 3),
      max: 5,
      exp: Date.UTC(2026, 9, 4, 12, 30) / 1000,
      cursed: true,
      tg: 42,
    });
    env.links.store.set(`clicks:${md5('peek')}`, '2');
    const response = await getAlias(visitContext(env, 'peek%2B').context);
    const html = await response.text();
    expect(response.headers.get('Cache-Control')).toBe('no-store');
    expect(html).toContain('s.4st.li/peek');
    expect(html).toContain('https://example.com/&lt;b&gt;');
    expect(html).toContain('<dt>born</dt><dd>2026-10-03</dd>');
    expect(html).toContain('<dt>visits</dt><dd>2</dd>');
    expect(html).toContain('after 3 more visits');
    expect(html).toContain('2026-10-04 12:30 UTC');
    expect(html).toContain('might rickroll you');
    expect(html).toContain('visit alerts');
  });

  test('previews plain links and keeps passwords secret', async () => {
    const env = makeEnv();
    storeLink(env, 'old', 'https://example.com/old');
    storeLink(env, 'locked', 'https://example.com/hidden', { pw: 'x.y', count: true });
    const plain = await (await getAlias(visitContext(env, 'old+').context)).text();
    expect(plain).toContain('https://example.com/old');
    expect(plain).not.toContain('<dt>visits</dt>');
    const locked = await (await getAlias(visitContext(env, 'locked+').context)).text();
    expect(locked).not.toContain('https://example.com/hidden');
    expect(locked).toContain('sworn to silence');
    expect(locked).toContain('<dt>visits</dt><dd>0</dd>');
  });

  test('asks for the password of a guarded link', async () => {
    const env = makeEnv();
    storeLink(env, 'vault', 'https://example.com/secret', { pw: 'x.y' });
    const response = await getAlias(visitContext(env, 'vault').context);
    const html = await response.text();
    expect(response.status).toBe(200);
    expect(html).toContain('<form method="post" action="/vault"');
    expect(html).not.toContain('https://example.com/secret');
  });

  test('counts visits without letting browsers cache the redirect', async () => {
    const env = makeEnv();
    storeLink(env, 'counted', 'https://example.com', { count: true, exp: 2000000000 });
    for (let index = 0; index < 2; index += 1) {
      const { context, pending } = visitContext(env, 'counted');
      const response = await getAlias(context);
      expect(response.status).toBe(302);
      expect(response.headers.get('Cache-Control')).toBe('no-store');
      await Promise.all(pending);
    }
    expect(env.links.store.get(`clicks:${md5('counted')}`)).toBe('2');
    expect(env.links.putCalls.at(-1)?.options).toEqual({ expiration: 2000000000 });
  });

  test('self-destructs after the last allowed visit', async () => {
    const env = makeEnv();
    storeLink(env, 'boom', 'https://example.com', { max: 2 });
    for (const expected of [true, false]) {
      const { context, pending } = visitContext(env, 'boom');
      expect((await getAlias(context)).status).toBe(302);
      await Promise.all(pending);
      expect(env.links.store.has(md5('boom'))).toBe(expected);
    }
    expect(env.links.store.has(`clicks:${md5('boom')}`)).toBe(false);
    expect((await getAlias(visitContext(env, 'boom').context)).status).toBe(404);
  });

  test('cursed links sometimes rickroll', async () => {
    const env = makeEnv();
    storeLink(env, 'cursed', 'https://example.com', { cursed: true });
    const random = spyOn(Math, 'random').mockReturnValueOnce(0.05).mockReturnValueOnce(0.5);
    const first = await getAlias(visitContext(env, 'cursed').context);
    const second = await getAlias(visitContext(env, 'cursed').context);
    expect(first.headers.get('Location')).toBe(RICKROLL_URL);
    expect(second.headers.get('Location')).toBe('https://example.com');
    random.mockRestore();
  });

  test('snitches visits to Telegram without the visitor IP', async () => {
    const env = makeEnv({ TELEGRAM_BOT_TOKEN: 'token' });
    storeLink(env, 'watched', 'https://example.com', { tg: 42 });
    const fetch = fetchSpy();
    const request = new Request(`${HOST}/watched`, {
      headers: {
        'User-Agent': 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1',
        'CF-Connecting-IP': '203.0.113.9',
      },
    });
    Object.defineProperty(request, 'cf', { value: { city: 'Rosario', country: 'AR' } });
    const { context, pending } = visitContext(env, 'watched', request);
    expect((await getAlias(context)).status).toBe(302);
    await Promise.all(pending);

    expect(fetch).toHaveBeenCalledTimes(1);
    const [url, init] = fetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('https://api.telegram.org/bottoken/sendMessage');
    const body = JSON.parse(String(init.body));
    expect(body.chat_id).toBe(42);
    expect(body.text).toBe(
      '🔮 someone opened s.4st.li/watched\n🌍 Rosario, AR\n🧭 Safari on iOS\n➡️ https://example.com',
    );
    expect(body.text).not.toContain('203.0.113.9');
  });

  test('stays quiet when the bot is not configured', async () => {
    const env = makeEnv();
    storeLink(env, 'watched', 'https://example.com', { tg: 42 });
    const fetch = fetchSpy();
    const { context, pending } = visitContext(env, 'watched');
    await getAlias(context);
    await Promise.all(pending);
    expect(fetch).not.toHaveBeenCalled();
  });
});

describe('POST /:alias (unlock)', () => {
  const unlock = (env: TestEnv, alias: string | undefined, fields: Record<string, string>) => {
    const request = formRequest(`/${alias ?? ''}`, fields, {});
    return visitContext(env, alias, request);
  };

  test('redirects with the right password', async () => {
    const env = makeEnv();
    storeLink(env, 'vault', 'https://example.com/secret', { pw: await hashPassword('hunter2'), count: true });
    const { context, pending } = unlock(env, 'vault', { password: 'hunter2' });
    const response = await unlockAlias(context);
    await Promise.all(pending);
    expect(response.status).toBe(302);
    expect(response.headers.get('Location')).toBe('https://example.com/secret');
    expect(env.links.store.get(`clicks:${md5('vault')}`)).toBe('1');
  });

  test('scolds wrong passwords', async () => {
    const env = makeEnv();
    storeLink(env, 'vault', 'https://example.com/secret', { pw: await hashPassword('hunter2') });
    const response = await unlockAlias(unlock(env, 'vault', { password: 'nope' }).context);
    expect(response.status).toBe(403);
    expect(await response.text()).toContain('the worm is disappointed in you');
  });

  test('just redirects links without a password', async () => {
    const env = makeEnv();
    storeLink(env, 'open', 'https://example.com');
    const response = await unlockAlias(unlock(env, 'open', { password: 'whatever' }).context);
    expect(response.status).toBe(301);
  });

  test('rejects requests without a password field', async () => {
    const env = makeEnv();
    for (const [alias, fields] of [['vault', {}], [undefined, { password: 'x' }]] as const) {
      const response = await unlockAlias(unlock(env, alias, fields).context);
      expect(response.status).toBe(400);
    }
  });

  test('treats missing links, previews and assets as lost', async () => {
    const env = makeEnv();
    storeLink(env, 'vault', 'https://example.com', { pw: 'x.y' });
    for (const alias of ['ghost', 'vault+', 'file.txt']) {
      const response = await unlockAlias(unlock(env, alias, { password: 'x' }).context);
      expect(response.status).toBe(404);
    }
  });
});

describe('POST /orb', () => {
  test('stores a new link with a manage key', async () => {
    const env = makeEnv();
    const response = await postOrb({
      env,
      request: formRequest('/orb', { alias: ' Cool%20 Link ', url: 'https://example.com' }),
    });
    const html = await response.text();
    expect(response.status).toBe(201);
    expect(response.headers.get('Content-Type')).toBe('text/html; charset=utf-8');
    expect(html).not.toContain('<!DOCTYPE html>');
    expect(html).toContain('the worm summoned your link');
    expect(html).toContain('<a class="short-link" href="https://s.4st.li/Cool-Link">s.4st.li/Cool-Link</a>');
    expect(html).toContain('href="https://s.4st.li/Cool-Link+"');
    expect(html).not.toContain('t.me');

    const key = html.match(/class="secret">([^<]+)</)?.[1] ?? '';
    expect(key).toMatch(/^[\w-]{22}$/);
    const meta = env.links.metadata.get(md5('cool-link')) as LinkMeta;
    expect(env.links.store.get(md5('cool-link'))).toBe('https://example.com');
    expect(meta.key).toBe(sha256(key));
    expect(meta.created).toBeNumber();
    expect(meta.pw).toBeUndefined();
  });

  test('renders a full page for plain form posts', async () => {
    const env = makeEnv();
    const response = await postOrb({
      env,
      request: formRequest('/orb', { alias: 'page', url: 'example.com' }, {}),
    });
    const html = await response.text();
    expect(html).toStartWith('<!DOCTYPE html>');
    expect(html).toContain('the worm summoned your link');
    expect(env.links.store.get(md5('page'))).toBe('https://example.com');
  });

  test('adds the Telegram hint when a bot is configured', async () => {
    const env = makeEnv({ TELEGRAM_BOT_USERNAME: 'MagiOrbBot' });
    const html = await (await postOrb({
      env,
      request: formRequest('/orb', { alias: 'tg', url: 'https://example.com' }),
    })).text();
    const key = html.match(/class="secret">([^<]+)</)?.[1];
    expect(html).toContain(`<code>/alert tg ${key}</code>`);
    expect(html).toContain('href="https://t.me/MagiOrbBot"');
  });

  test('casts every extra spell', async () => {
    const env = makeEnv();
    const now = spyOn(Date, 'now').mockReturnValue(1_800_000_000_000);
    const response = await postOrb({
      env,
      request: formRequest('/orb', {
        alias: 'spells',
        url: 'https://example.com',
        password: 'hunter2',
        burn: '3',
        expires: '3600',
        cursed: 'on',
        count: 'on',
      }),
    });
    now.mockRestore();
    expect(response.status).toBe(201);
    const meta = env.links.metadata.get(md5('spells')) as LinkMeta;
    expect(meta).toMatchObject({ max: 3, exp: 1_800_003_600, cursed: true, count: true, created: 1_800_000_000_000 });
    expect(meta.pw).toMatch(/^[\w-]+\.[\w-]+$/);
    expect(JSON.stringify(meta)).not.toContain('hunter2');
    expect(env.links.putCalls[0].options?.expiration).toBe(1_800_003_600);
  });

  test.each([
    ['a zero self-destruct', { burn: '0' }],
    ['a fractional self-destruct', { burn: '1.5' }],
    ['a huge self-destruct', { burn: '1000001' }],
    ['an unknown lifetime', { expires: '42' }],
    ['a novel as password', { password: 'x'.repeat(257) }],
  ])('rejects %s', async (_label, spells) => {
    const env = makeEnv();
    const response = await postOrb({
      env,
      request: formRequest('/orb', { alias: 'x', url: 'https://example.com', ...spells }),
    });
    expect(response.status).toBe(400);
    expect(await response.text()).toContain('the orb rejected your weird spell settings.');
    expect(env.links.store.size).toBe(0);
  });

  test('lets the worm pick an alias', async () => {
    const env = makeEnv();
    const random = spyOn(Math, 'random').mockReturnValue(0);
    const response = await postOrb({ env, request: formRequest('/orb', { url: 'https://example.com' }) });
    random.mockRestore();
    expect(await response.text()).toContain('s.4st.li/sneaky-worm-0');
    expect(env.links.store.get(md5('sneaky-worm-0'))).toBe('https://example.com');
  });

  test('gives up when every random alias is taken', async () => {
    const env = makeEnv();
    storeLink(env, 'sneaky-worm-0', 'https://taken.example');
    const random = spyOn(Math, 'random').mockReturnValue(0);
    const response = await postOrb({ env, request: formRequest('/orb', { alias: '  ', url: 'https://example.com' }) });
    random.mockRestore();
    expect(response.status).toBe(503);
    expect(await response.text()).toContain('the worm ran out of ideas');
  });

  test('refuses to overwrite an existing alias', async () => {
    const env = makeEnv();
    storeLink(env, 'taken', 'https://old.example');
    const response = await postOrb({
      env,
      request: formRequest('/orb', { alias: 'TAKEN', url: 'https://new.example' }),
    });
    expect(response.status).toBe(409);
    expect(await response.text()).toContain('the orb rejected your access to rewrite this link.');
    expect(env.links.store.get(md5('taken'))).toBe('https://old.example');
  });

  test('rejects requests without a url or with a file alias', async () => {
    const env = makeEnv();
    for (const fields of [{ alias: 'a' }, { alias: new Blob(['a']), url: 'https://example.com' }]) {
      const response = await postOrb({ env, request: formRequest('/orb', fields) });
      expect(await response.text()).toContain('the orb rejected your request.');
    }
    expect(env.links.store.size).toBe(0);
  });

  test.each([
    ['a dotted alias', 'file.txt'],
    ['an alias with special characters', 'ñandú'],
    ['an alias that is too long', 'a'.repeat(MAX_ALIAS_LENGTH + 1)],
    ['a reserved alias', 'Manage'],
  ])('rejects %s', async (_label, alias) => {
    const env = makeEnv();
    const response = await postOrb({ env, request: formRequest('/orb', { alias, url: 'https://example.com' }) });
    expect(await response.text()).toContain('the orb rejected your alias.');
    expect(env.links.store.size).toBe(0);
  });

  test.each([
    ['an unsupported scheme', 'ftp://example.com'],
    ['garbage', 'not a url'],
    ['a URL that is too long', `https://example.com/${'a'.repeat(MAX_URL_LENGTH)}`],
  ])('rejects %s', async (_label, url) => {
    const env = makeEnv();
    const response = await postOrb({ env, request: formRequest('/orb', { alias: 'ok', url }) });
    expect(await response.text()).toContain('the orb rejected your invalid url.');
    expect(env.links.store.size).toBe(0);
  });

  test('refuses to shorten its own links', async () => {
    const env = makeEnv();
    const response = await postOrb({ env, request: formRequest('/orb', { alias: 'loop', url: 's.4st.li/loop' }) });
    expect(await response.text()).toContain('the orb refuses to eat its own tail.');
  });
});

describe('POST /manage', () => {
  const KEY = 'manage-key';

  const seed = async (env: TestEnv, meta: LinkMeta = {}) => {
    storeLink(env, 'mine', 'https://example.com/old', { key: sha256(KEY), ...meta });
    env.links.store.set(`clicks:${md5('mine')}`, '7');
  };

  test('banishes a link and its counter', async () => {
    const env = makeEnv();
    await seed(env);
    const response = await manage({
      env,
      request: formRequest('/manage', { alias: 'MINE', key: ` ${KEY} `, action: 'banish' }),
    });
    expect(await response.text()).toContain('the worm banished your link to the shadow realm.');
    expect(env.links.store.size).toBe(0);
  });

  test('transmutes a link and keeps its spells', async () => {
    const env = makeEnv();
    await seed(env, { exp: 2000000000, cursed: true });
    const response = await manage({
      env,
      request: formRequest('/manage', { alias: 'mine', key: KEY, action: 'transmute', url: 'example.org/new' }),
    });
    expect(await response.text()).toContain('the worm transmuted your link.');
    expect(env.links.store.get(md5('mine'))).toBe('https://example.org/new');
    expect(env.links.metadata.get(md5('mine'))).toEqual({ key: sha256(KEY), exp: 2000000000, cursed: true });
    expect(env.links.putCalls.at(-1)?.options?.expiration).toBe(2000000000);
  });

  test('validates the new link', async () => {
    const env = makeEnv();
    await seed(env);
    for (const [url, message] of [
      [undefined, 'invalid url'],
      ['ftp://example.com', 'invalid url'],
      ['https://s.4st.li/mine', 'its own tail'],
    ] as const) {
      const fields: Record<string, string> = { alias: 'mine', key: KEY, action: 'transmute' };
      if (url) {
        fields.url = url;
      }
      const response = await manage({ env, request: formRequest('/manage', fields) });
      expect(await response.text()).toContain(message);
    }
    expect(env.links.store.get(md5('mine'))).toBe('https://example.com/old');
  });

  test('rejects wrong keys and links without one', async () => {
    const env = makeEnv();
    await seed(env);
    storeLink(env, 'legacy', 'https://example.com');
    for (const alias of ['mine', 'legacy']) {
      const response = await manage({
        env,
        request: formRequest('/manage', { alias, key: 'nope', action: 'banish' }),
      });
      expect(response.status).toBe(403);
      expect(await response.text()).toContain("the orb doesn't recognize that manage key.");
    }
    expect(env.links.store.has(md5('mine'))).toBe(true);
  });

  test('reports missing links', async () => {
    const env = makeEnv();
    const response = await manage({
      env,
      request: formRequest('/manage', { alias: 'ghost', key: KEY, action: 'banish' }),
    });
    expect(response.status).toBe(404);
  });

  test('rejects incomplete requests', async () => {
    const env = makeEnv();
    for (const fields of [
      { key: KEY, action: 'banish' },
      { alias: 'mine', action: 'banish' },
      { alias: 'mine', key: KEY, action: 'explode' },
    ]) {
      const response = await manage({ env, request: formRequest('/manage', fields) });
      expect(response.status).toBe(400);
    }
  });
});

describe('POST /telegram', () => {
  const SECRET = 'webhook-secret';
  const KEY = 'manage-key';
  const botEnv = () => makeEnv({ TELEGRAM_BOT_TOKEN: 'token', TELEGRAM_WEBHOOK_SECRET: SECRET });

  const update = (text: unknown, secret = SECRET, chatId: unknown = 99) =>
    new Request(`${HOST}/telegram`, {
      method: 'POST',
      headers: { 'X-Telegram-Bot-Api-Secret-Token': secret },
      body: JSON.stringify({ message: { text, chat: { id: chatId } } }),
    });

  const replies = (fetch: ReturnType<typeof fetchSpy>): string[] =>
    fetch.mock.calls.map(([, init]) => JSON.parse(String((init as RequestInit).body)).text);

  test('hides when the bot is not configured or the secret is wrong', async () => {
    for (const [env, secret] of [
      [makeEnv(), SECRET],
      [makeEnv({ TELEGRAM_BOT_TOKEN: 'token' }), SECRET],
      [botEnv(), 'wrong'],
      [botEnv(), ''],
    ] as const) {
      const response = await telegram({ env, request: update('/start', secret) });
      expect(response.status).toBe(404);
    }
    const missingHeader = new Request(`${HOST}/telegram`, { method: 'POST', body: '{}' });
    expect((await telegram({ env: botEnv(), request: missingHeader })).status).toBe(404);
  });

  test('ignores updates without text', async () => {
    const fetch = fetchSpy();
    for (const request of [update(undefined), update('/start', SECRET, 'nope')]) {
      expect((await telegram({ env: botEnv(), request })).status).toBe(200);
    }
    expect(fetch).not.toHaveBeenCalled();
  });

  test('answers help and chat id commands', async () => {
    const fetch = fetchSpy();
    const env = botEnv();
    await telegram({ env, request: update('/start') });
    await telegram({ env, request: update('/GetID@MagiOrbBot') });
    expect(replies(fetch)).toEqual([expect.stringContaining("i'm the orb's snitch"), 'chat id: 99']);
    expect(fetch.mock.calls[0][0]).toBe('https://api.telegram.org/bottoken/sendMessage');
  });

  test('toggles visit alerts with the manage key', async () => {
    const fetch = fetchSpy();
    const env = botEnv();
    storeLink(env, 'mine', 'https://example.com', { key: sha256(KEY), exp: 2000000000 });
    await telegram({ env, request: update(`/alert Mine ${KEY}`) });
    expect(env.links.metadata.get(md5('mine'))).toEqual({ key: sha256(KEY), exp: 2000000000, tg: 99 });
    expect(env.links.putCalls.at(-1)?.options?.expiration).toBe(2000000000);
    await telegram({ env, request: update(`/alert mine ${KEY}`) });
    expect(env.links.metadata.get(md5('mine'))).toEqual({ key: sha256(KEY), exp: 2000000000 });
    expect(replies(fetch)).toEqual(['👀 watching Mine. i\'ll snitch on every visit.', '😴 stopped watching mine.']);
  });

  test('refuses bad alert requests', async () => {
    const fetch = fetchSpy();
    const env = botEnv();
    storeLink(env, 'mine', 'https://example.com', { key: sha256(KEY) });
    storeLink(env, 'legacy', 'https://example.com');
    for (const text of ['/alert', '/alert mine', '/alert ghost key', '/alert mine nope', `/alert legacy ${KEY}`]) {
      await telegram({ env, request: update(text) });
    }
    expect(replies(fetch)).toEqual([
      'usage: /alert <alias> <manage key>',
      'usage: /alert <alias> <manage key>',
      "the orb didn't found this link.",
      "the orb doesn't recognize that manage key.",
      "the orb doesn't recognize that manage key.",
    ]);
    expect(env.links.putCalls).toHaveLength(0);
  });
});
