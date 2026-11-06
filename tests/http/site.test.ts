import { describe, expect, test } from 'bun:test';
import { BASE, BOT_NAME, create, go, newIp, post, TARGET, telegram, uniqueAlias } from './helpers';

const RICKROLL = 'https://www.youtube.com/watch?v=dQw4w9WgXcQ';
const rustOnly = test.skipIf(TARGET !== 'rust');

const peekText = async (alias: string): Promise<string> => (await go(`/${alias}+`)).text();
const destinations = (html: string): string[] =>
  [...new Set(html.match(/https:\/\/[a-z]+\.example(?=")/g) ?? [])].sort();

describe('static site', () => {
  test('serves the homepage for GET and HEAD', async () => {
    const page = await go('/');
    expect(page.status).toBe(200);
    expect(page.headers.get('Content-Type')).toContain('text/html');
    expect(await page.text()).toContain('a wizard worm that powers an orb');
    expect((await go('/', { method: 'HEAD' })).status).toBe(200);
  });

  test.each([
    ['/style.css', 'text/css'],
    ['/app.js', 'javascript'],
    ['/logo3d.webp', 'image/webp'],
    ['/favicon.ico', 'image'],
    ['/fonts/fredoka-latin.woff2', 'font/woff2'],
  ])('serves %s', async (path, type) => {
    const response = await go(path);
    expect(response.status).toBe(200);
    expect(response.headers.get('Content-Type')).toContain(type);
  });

  // The old site answered these with the homepage and a 200.
  rustOnly('answers 404 for a dotted path that is no file', async () => {
    expect((await go('/nope.css')).status).toBe(404);
  });
});

describe('GET /orb', () => {
  test('hands out public settings', async () => {
    const response = await go('/orb');
    expect(response.headers.get('Cache-Control')).toBe('no-store');
    expect(response.headers.get('Content-Type')).toContain('application/json');
    expect(await response.json()).toEqual({ turnstileSiteKey: null });
  });

  test('checks aliases while you type', async () => {
    const { alias } = await create({ alias: uniqueAlias('Taken') });
    const status = async (value: string) =>
      (await go(`/orb?alias=${encodeURIComponent(value)}`)).json();
    const fresh = uniqueAlias('fresh');
    expect(await status(`Big ${fresh}`)).toEqual({ status: 'free', alias: `Big-${fresh}` });
    expect(await status(alias.toUpperCase())).toEqual({ status: 'taken' });
    expect(await status('no.dots')).toEqual({ status: 'invalid' });
    expect(await status('  ')).toEqual({ status: 'invalid' });
    expect(await status('orb')).toEqual({ status: 'invalid' });
    expect(await status('ñandú')).toEqual({ status: 'invalid' });
  });
});

describe('POST /orb', () => {
  test('creates a link and answers with a fragment', async () => {
    const stored = uniqueAlias('Cool');
    const { html, key, status } = await create({ alias: ` ${stored.replace('-', '%20')} ` });
    expect(status).toBe(201);
    expect(html).not.toContain('<!DOCTYPE html>');
    expect(html).toContain('the worm summoned your link');
    expect(html).toContain(`href="${BASE}/${stored}"`);
    expect(html).toContain(`href="${BASE}/${stored}+"`);
    expect(html).toContain(`data-alias="${stored}"`);
    expect(key).toMatch(/^[\w-]{22}$/);
    expect(html).toContain('<details class="qr">');
    expect(html).toContain('<svg xmlns="http://www.w3.org/2000/svg"');
    expect(html).toContain(`<code>/alert ${stored} ${key}</code>`);
    expect(html).toContain(`href="https://t.me/${BOT_NAME}"`);
  });

  test('answers plain form posts with a full page', async () => {
    const alias = uniqueAlias('plain');
    const response = await go('/orb', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'CF-Connecting-IP': newIp() },
      body: new URLSearchParams({ url: 'example.com', alias }),
    });
    const html = await response.text();
    expect(response.status).toBe(201);
    expect(html).toStartWith('<!DOCTYPE html>');
    expect(html).toContain('the worm summoned your link');
    const visit = await go(`/${alias}`);
    expect(visit.status).toBe(301);
    expect(visit.headers.get('Location')).toBe('https://example.com');
  });

  test('lets the worm pick an alias', async () => {
    const response = await post('/orb', { url: 'https://example.com' });
    expect(response.status).toBe(201);
    expect(await response.text()).toMatch(/data-alias="[a-z]+-[a-z]+-\d{1,2}"/);
  });

  test('refuses to overwrite an alias, whatever the case', async () => {
    const { alias } = await create({ alias: uniqueAlias('mine') });
    const again = await create({ alias: alias.toUpperCase(), url: 'https://new.example' });
    expect(again.status).toBe(409);
    expect(again.html).toContain('the orb rejected your access to rewrite this link.');
    expect((await go(`/${alias}`)).headers.get('Location')).toBe('https://example.com/target');
  });

  test.each([
    ['a dotted alias', 'file.txt'],
    ['special characters', 'ñandú'],
    ['a reserved alias', 'Manage'],
    ['another reserved alias', 'telegram'],
    ['a long alias', 'a'.repeat(13313)],
  ])('rejects %s', async (_label, alias) => {
    const { status, html } = await create({ alias });
    expect(status).toBe(400);
    expect(html).toContain('the orb rejected your alias.');
  });

  test.each([
    ['an unsupported scheme', 'ftp://example.com'],
    ['garbage', 'not a url'],
    ['a javascript link', 'javascript:alert(1)'],
    ['a very long url', `https://example.com/${'a'.repeat(2048)}`],
  ])('rejects %s', async (_label, url) => {
    const { status, html } = await create({ url });
    expect(status).toBe(400);
    expect(html).toContain('the orb rejected your invalid url.');
  });

  test('refuses to shorten itself', async () => {
    const { status, html } = await create({ url: new URL(BASE).host });
    expect(status).toBe(400);
    expect(html).toContain('the orb refuses to eat its own tail.');
  });

  test.each([
    ['a zero self-destruct', { burn: '0' }],
    ['a fractional self-destruct', { burn: '1.5' }],
    ['a huge self-destruct', { burn: '1000001' }],
    ['a word as self-destruct', { burn: 'soon' }],
    ['an unknown lifetime', { expires: '42' }],
    ['a novel as password', { password: 'x'.repeat(257) }],
  ])('rejects %s', async (_label, spells) => {
    const { status, html } = await create(spells);
    expect(status).toBe(400);
    expect(html).toContain('the orb rejected your weird spell settings.');
  });

  test('rejects a request with no url', async () => {
    const response = await post('/orb', { alias: 'x' });
    expect(response.status).toBe(400);
    expect(await response.text()).toContain('the orb rejected your request.');
  });

  test.skipIf(TARGET !== 'rust').each([
    ['a file as url', { url: new File(['x'], 'x.txt') }],
    ['a file as alias', { url: 'https://example.com', alias: new File(['x'], 'x.txt') }],
    ['a file as fortune', { url: 'https://example.com', fortune: new File(['x'], 'x.txt') }],
  ])('rejects %s', async (_label, fields) => {
    const response = await post('/orb', fields);
    expect(response.status).toBe(400);
    expect(await response.text()).toContain('the orb rejected your request.');
  });

  rustOnly('rejects an alias that decodes to nothing', async () => {
    const { status, html } = await create({ alias: '%20' });
    expect(status).toBe(400);
    expect(html).toContain('the orb rejected your alias.');
  });

  rustOnly('answers a body that is not a form with a clean error', async () => {
    const response = await go('/orb', {
      method: 'POST',
      body: '{"nope":1}',
      headers: { 'Content-Type': 'application/json' },
    });
    expect(response.status).toBe(400);
    expect(await response.text()).toContain('the orb rejected your request.');
  });

  test('slows down spam per visitor', async () => {
    const spammer = newIp();
    const headers = { 'X-Orb': 'fragment', 'CF-Connecting-IP': spammer };
    for (let index = 0; index < 30; index += 1) {
      expect((await create({}, headers)).status).toBe(201);
    }
    const blocked = await create({}, headers);
    expect(blocked.status).toBe(429);
    expect(blocked.html).toContain('the worm is exhausted');
    expect((await create()).status).toBe(201);
  }, 30000);
});

describe('visiting a link', () => {
  test('redirects permanently with a short cache', async () => {
    const { alias } = await create();
    const response = await go(`/${alias}`);
    expect(response.status).toBe(301);
    expect(response.headers.get('Location')).toBe('https://example.com/target');
    expect(response.headers.get('Cache-Control')).toBe('public, max-age=300');
  });

  test('finds aliases whatever the case or spacing', async () => {
    const alias = uniqueAlias('Big-Spell');
    await create({ alias });
    expect((await go(`/${alias.toUpperCase()}`)).status).toBe(301);
    expect((await go(`/${alias.replace('-', '%20')}`)).status).toBe(301);
    expect((await go(`/${alias}%2B`)).status).toBe(200);
  });

  test('keeps the destination exactly as it was typed', async () => {
    const { alias } = await create({ url: 'https://Example.com:8080/x?y=1#z' });
    expect((await go(`/${alias}`)).headers.get('Location')).toBe('https://Example.com:8080/x?y=1#z');
  });

  rustOnly('redirects to destinations with non-ASCII characters', async () => {
    const { alias, status } = await create({ url: 'https://example.com/日本語?q=ñ' });
    expect(status).toBe(201);
    const response = await go(`/${alias}`);
    expect(response.status).toBe(301);
    expect(response.headers.get('Location')).toBe('https://example.com/%E6%97%A5%E6%9C%AC%E8%AA%9E?q=%C3%B1');
  });

  test('shows the lost page for a missing link', async () => {
    const response = await go(`/${uniqueAlias('ghost')}`);
    expect(response.status).toBe(404);
    expect(response.headers.get('Cache-Control')).toBe('no-store');
    expect(await response.text()).toContain("the orb didn't found this link.");
  });

  test('answers HEAD like GET without counting a visit', async () => {
    const plain = await create();
    const counted = await create({ count: 'on' });
    expect((await go(`/${plain.alias}`, { method: 'HEAD' })).status).toBe(301);
    const head = await go(`/${counted.alias}`, { method: 'HEAD' });
    expect(head.status).toBe(302);
    expect(head.headers.get('Location')).toBe('https://example.com/target');
    expect(await peekText(counted.alias)).toContain('<dt>visits</dt><dd>0</dd>');
    expect((await go(`/${uniqueAlias('ghost')}`, { method: 'HEAD' })).status).toBe(404);
  });

  test('counts visits and stops browsers caching the redirect', async () => {
    const { alias } = await create({ count: 'on' });
    for (let index = 0; index < 2; index += 1) {
      const response = await go(`/${alias}`);
      expect(response.status).toBe(302);
      expect(response.headers.get('Cache-Control')).toBe('no-store');
      await Bun.sleep(300);
    }
    expect(await peekText(alias)).toContain('<dt>visits</dt><dd>2</dd>');
  });

  test('self-destructs after the last visit', async () => {
    const { alias } = await create({ burn: '2' });
    expect((await go(`/${alias}`)).status).toBe(302);
    await Bun.sleep(300);
    expect((await go(`/${alias}`)).status).toBe(302);
    await Bun.sleep(300);
    expect((await go(`/${alias}`)).status).toBe(404);
  });

  test('curses sometimes rickroll', async () => {
    const { alias } = await create({ cursed: 'on' });
    const seen = new Set<string | null>();
    for (let index = 0; index < 200; index += 1) {
      seen.add((await go(`/${alias}`)).headers.get('Location'));
    }
    expect([...seen].sort()).toEqual(['https://example.com/target', RICKROLL].sort());
  }, 30000);

  test('fortune cookies pick from every destination', async () => {
    const { alias } = await create({ url: 'https://a.example', fortune: ' b.example \n\nhttps://c.example\r\n' });
    const seen = new Set<string | null>();
    for (let index = 0; index < 80; index += 1) {
      const response = await go(`/${alias}`);
      expect(response.status).toBe(302);
      seen.add(response.headers.get('Location'));
    }
    expect([...seen].sort()).toEqual(['https://a.example', 'https://b.example', 'https://c.example']);
  }, 30000);

  test.each([
    ['too many fortunes', Array(10).fill('example.com').join('\n'), 'weird spell settings'],
    ['a bad fortune', 'nope nope', 'invalid url'],
    ['a fortune pointing home', new URL(BASE).host, 'its own tail'],
  ])('rejects %s', async (_label, fortune, message) => {
    const { status, html } = await create({ fortune });
    expect(status).toBe(400);
    expect(html).toContain(message);
  });

  test('countdowns make visitors wait, HEAD gets the redirect', async () => {
    const { alias } = await create({ url: 'https://example.com/slow', wait: 'on' });
    const page = await go(`/${alias}`);
    const html = await page.text();
    expect(page.status).toBe(200);
    expect(page.headers.get('Refresh')).toBe('5; url=https://example.com/slow');
    expect(page.headers.get('Cache-Control')).toBe('no-store');
    expect(html).toContain('data-target="https://example.com/slow"');
    expect(html).toContain('charging the orb');
    const head = await go(`/${alias}`, { method: 'HEAD' });
    expect(head.status).toBe(302);
    expect(head.headers.get('Location')).toBe('https://example.com/slow');
  });
});

describe('peek', () => {
  test('shows every spell', async () => {
    const { alias } = await create({
      url: 'https://example.com/<b>',
      burn: '5',
      expires: '3600',
      cursed: 'on',
      wait: 'on',
    });
    const response = await go(`/${alias}+`);
    const html = await response.text();
    expect(response.headers.get('Cache-Control')).toBe('no-store');
    expect(html).toContain(`${new URL(BASE).host}/${alias}`);
    expect(html).toContain('https://example.com/&lt;b&gt;');
    expect(html).toMatch(/<dt>born<\/dt><dd>\d{4}-\d{2}-\d{2}<\/dd>/);
    expect(html).toContain('<dt>visits</dt><dd>0</dd>');
    expect(html).toContain('after 5 more visits');
    expect(html).toMatch(/<dt>expires<\/dt><dd>\d{4}-\d{2}-\d{2} \d{2}:\d{2} UTC<\/dd>/);
    expect(html).toContain('might rickroll you');
    expect(html).toContain('seconds of suspense');
  });

  test('previews fortune cookies', async () => {
    const { alias } = await create({ url: 'https://a.example', fortune: 'https://b.example' });
    const html = await peekText(alias);
    expect(html).toContain('🥠 goes to one of');
    expect(destinations(html)).toEqual(['https://a.example', 'https://b.example']);
  });

  test('keeps passwords secret', async () => {
    const { alias } = await create({ url: 'https://hidden.example', password: 'hunter2', count: 'on' });
    const html = await peekText(alias);
    expect(html).not.toContain('hidden.example');
    expect(html).toContain('sworn to silence');
    expect(html).toContain('<dt>visits</dt><dd>0</dd>');
  });
});

describe('passwords', () => {
  const unlock = (alias: string, password: string, ip = newIp()) =>
    post(`/${alias}`, { password }, { 'CF-Connecting-IP': ip });

  test('asks first, then redirects with the right word', async () => {
    const { alias } = await create({ url: 'https://example.com/secret', password: 'hunter2', count: 'on' });
    const ask = await go(`/${alias}`);
    const html = await ask.text();
    expect(ask.status).toBe(200);
    expect(html).toContain(`<form method="post" action="/${alias}"`);
    expect(html).not.toContain('example.com/secret');

    const open = await unlock(alias, 'hunter2');
    expect(open.status).toBe(302);
    expect(open.headers.get('Location')).toBe('https://example.com/secret');
    await Bun.sleep(300);
    expect(await peekText(alias)).toContain('<dt>visits</dt><dd>1</dd>');
  });

  test('scolds wrong words and locks out after five', async () => {
    const { alias } = await create({ password: 'hunter2' });
    const guesser = newIp();
    for (let index = 0; index < 5; index += 1) {
      const wrong = await unlock(alias, 'nope', guesser);
      expect(wrong.status).toBe(403);
      expect(await wrong.text()).toContain('the worm is disappointed in you');
    }
    const locked = await unlock(alias, 'hunter2', guesser);
    expect(locked.status).toBe(429);
    expect(await locked.text()).toContain('too many wrong guesses');
    expect((await unlock(alias, 'hunter2')).status).toBe(302);
  });

  test('accepts plain form posts', async () => {
    const { alias } = await create({ password: 'hunter2' });
    const response = await go(`/${alias}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded', 'CF-Connecting-IP': newIp() },
      body: new URLSearchParams({ password: 'hunter2' }),
    });
    expect(response.status).toBe(302);
  });

  test('just redirects links without a password', async () => {
    const { alias } = await create();
    expect((await unlock(alias, 'whatever')).status).toBe(301);
  });

  test('rejects posts with no password', async () => {
    const { alias } = await create({ password: 'hunter2' });
    expect((await post(`/${alias}`, { nope: 'x' })).status).toBe(400);
  });

  test('treats ghosts, peeks and files as lost', async () => {
    const { alias } = await create({ password: 'hunter2' });
    for (const path of [uniqueAlias('ghost'), `${alias}+`, 'file.txt']) {
      expect((await unlock(path, 'hunter2')).status).toBe(404);
    }
  });
});

describe('POST /manage', () => {
  const manage = (fields: Record<string, string>) => post('/manage', fields);

  test('transmutes a link and keeps its spells', async () => {
    const { alias, key } = await create({ count: 'on', fortune: 'b.example' });
    const response = await manage({ alias: alias.toUpperCase(), key: ` ${key} `, action: 'transmute', url: 'z.example' });
    expect(response.status).toBe(200);
    expect(await response.text()).toContain('the worm transmuted your link.');
    const html = await peekText(alias);
    expect(html).toContain('https://z.example');
    expect(html).toContain('https://b.example');
    expect(html).toContain('<dt>visits</dt>');
  });

  test('validates the new link', async () => {
    const { alias, key } = await create();
    const attempt = (url?: string) =>
      manage({ alias, key, action: 'transmute', ...(url ? { url } : {}) });
    expect(await (await attempt()).text()).toContain('invalid url');
    expect(await (await attempt('ftp://example.com')).text()).toContain('invalid url');
    expect(await (await attempt(new URL(BASE).host)).text()).toContain('its own tail');
    expect((await go(`/${alias}`)).headers.get('Location')).toBe('https://example.com/target');
  });

  test('banishes a link', async () => {
    const { alias, key } = await create();
    const response = await manage({ alias, key, action: 'banish' });
    expect(await response.text()).toContain('the worm banished your link to the shadow realm.');
    expect((await go(`/${alias}`)).status).toBe(404);
  });

  test('rejects wrong keys, ghosts and incomplete requests', async () => {
    const { alias, key } = await create();
    const wrong = await manage({ alias, key: 'nope', action: 'banish' });
    expect(wrong.status).toBe(403);
    expect(await wrong.text()).toContain("the orb doesn't recognize that manage key.");
    expect((await manage({ alias: uniqueAlias('ghost'), key, action: 'banish' })).status).toBe(404);
    for (const fields of [{ key, action: 'banish' }, { alias, action: 'banish' }, { alias, key, action: 'explode' }]) {
      expect((await manage(fields as Record<string, string>)).status).toBe(400);
    }
    expect((await go(`/${alias}`)).status).toBe(301);
  });
});

describe('POST /telegram', () => {
  test('hides behind the webhook secret', async () => {
    expect((await telegram('/start', 'wrong')).status).toBe(404);
    expect((await go('/telegram', { method: 'POST', body: '{}' })).status).toBe(404);
  });

  rustOnly('toggles alerts and keeps every fortune cookie destination', async () => {
    const { alias, key } = await create({ url: 'https://a.example', fortune: 'b.example\nc.example' });
    await telegram(`/alert ${alias} ${key}`);
    const watched = await peekText(alias);
    expect(watched).toContain('the owner gets visit alerts');
    expect(destinations(watched)).toEqual(['https://a.example', 'https://b.example', 'https://c.example']);
    await telegram(`/alert ${alias} ${key}`);
    const quiet = await peekText(alias);
    expect(quiet).not.toContain('the owner gets visit alerts');
    expect(destinations(quiet)).toEqual(['https://a.example', 'https://b.example', 'https://c.example']);
  });

  rustOnly('answers 200 even when replying to Telegram fails, so it does not retry', async () => {
    expect((await telegram('/start')).status).toBe(200);
  });

  test('ignores a wrong manage key', async () => {
    const { alias } = await create();
    await telegram(`/alert ${alias} nope`);
    expect(await peekText(alias)).not.toContain('the owner gets visit alerts');
  });
});

describe('routing', () => {
  test('treats function paths as ghosts for other methods', async () => {
    expect((await go('/manage')).status).toBe(404);
    expect((await go('/telegram')).status).toBe(404);
  });

  test('accepts a trailing slash', async () => {
    const { alias } = await create();
    expect((await go(`/${alias}/`)).status).toBe(301);
    expect((await go('/orb/')).status).toBe(200);
  });

  test('treats /index as an alias, not as the homepage', async () => {
    expect((await go('/index')).status).toBe(404);
  });

  test.each(['PUT', 'DELETE', 'OPTIONS', 'PATCH'])('refuses %s', async (method) => {
    for (const path of ['/', '/orb', '/some-alias']) {
      expect((await go(path, { method })).status).toBe(405);
    }
  });

  // The old site answered this with the homepage and a 200.
  rustOnly('answers multi-segment paths with 404', async () => {
    expect((await go('/a/b')).status).toBe(404);
    expect((await go('/a/b', { method: 'POST' })).status).toBeGreaterThanOrEqual(400);
  });
});
