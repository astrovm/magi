// Normal pastes, as a browser or `curl` sees them.
import { describe, expect, test } from 'bun:test';
import { createPaste, go, post, uniqueAlias } from './helpers';

const TEXT = `line one\nline two\n<script>alert('orb')</script> ñandú 🪱`;

describe('POST /paste', () => {
  test('creates a paste that reads back byte for byte', async () => {
    const { alias, html, key, status } = await createPaste({ text: TEXT });
    expect(status).toBe(201);
    expect(html).toContain('the worm summoned your paste');
    expect(key).not.toBe('');

    const raw = await go(`/${alias}?raw`);
    expect(raw.status).toBe(200);
    expect(raw.headers.get('content-type')).toBe('text/plain; charset=utf-8');
    expect(raw.headers.get('x-content-type-options')).toBe('nosniff');
    expect(raw.headers.get('content-security-policy')).toContain('sandbox');
    expect(await raw.text()).toBe(TEXT);
  });

  test('shows the page with the text escaped', async () => {
    const { alias } = await createPaste({ text: TEXT });
    const page = await go(`/${alias}`);
    const html = await page.text();
    expect(page.status).toBe(200);
    expect(page.headers.get('content-security-policy')).toContain("default-src 'none'");
    expect(html).toContain('&lt;script&gt;alert');
    expect(html).not.toContain("<script>alert");
  });

  test('accepts the biggest paste and refuses one byte more', async () => {
    const limit = 512 * 1024;
    const fits = await createPaste({ text: 'a'.repeat(limit) });
    expect(fits.status).toBe(201);
    expect(await (await go(`/${fits.alias}?raw`)).text()).toHaveLength(limit);
    expect((await createPaste({ text: 'a'.repeat(limit + 1) })).status).toBe(413);
  });

  test('refuses empty pastes and the paste path as an alias', async () => {
    expect((await createPaste({ text: '  \n ' })).status).toBe(400);
    expect((await createPaste({ alias: 'paste' })).status).toBe(400);
    const link = await post('/orb', { url: 'https://example.com', alias: 'PASTE' });
    expect(link.status).toBe(400);
  });

  test('shares aliases with links', async () => {
    const alias = uniqueAlias('shared');
    expect((await post('/orb', { url: 'https://example.com', alias })).status).toBe(201);
    expect((await createPaste({ alias })).status).toBe(409);
  });
});

describe('spells on a paste', () => {
  test('a password guards the page and the raw text', async () => {
    const { alias } = await createPaste({ text: TEXT, password: 'hunter2 ñ' });
    expect(await (await go(`/${alias}`)).text()).not.toContain('line one');
    expect(await (await go(`/${alias}?raw`)).text()).not.toContain('line one');
    expect((await post(`/${alias}`, { password: 'nope' })).status).toBe(403);
    const unlocked = await post(`/${alias}?raw`, { password: 'hunter2 ñ' });
    expect(await unlocked.text()).toBe(TEXT);
  });

  test('self-destructs after its last visit', async () => {
    const { alias } = await createPaste({ burn: '1' });
    expect((await go(`/${alias}`)).status).toBe(200);
    await Bun.sleep(300);
    expect((await go(`/${alias}`)).status).toBe(404);
  });

  test('peek tells the size and hides the text', async () => {
    const { alias } = await createPaste({ text: TEXT, count: 'on' });
    const html = await (await go(`/${alias}%2B`)).text();
    expect(html).toContain('a paste, 1 KB');
    expect(html).not.toContain('line one');
  });

  test('expires in 30 days unless told otherwise', async () => {
    const { alias } = await createPaste();
    expect(await (await go(`/${alias}%2B`)).text()).toContain('<dt>expires</dt>');
    const forever = await createPaste({ expires: '' });
    expect(await (await go(`/${forever.alias}%2B`)).text()).not.toContain('<dt>expires</dt>');
  });
});

describe('managing a paste', () => {
  test('transmute edits the text and banish removes it', async () => {
    const { alias, key } = await createPaste({ text: TEXT });
    const edited = await post('/manage', { alias, key, action: 'transmute', text: 'new text' });
    expect(edited.status).toBe(200);
    await Bun.sleep(100);
    const banished = await post('/manage', { alias, key, action: 'banish' });
    expect(banished.status).toBe(200);
    expect(await banished.text()).toContain('banished your paste');
  });

  test('a wrong key changes nothing', async () => {
    const { alias } = await createPaste({ text: TEXT });
    const denied = await post('/manage', { alias, key: 'nope', action: 'transmute', text: 'x' });
    expect(denied.status).toBe(403);
    expect(await (await go(`/${alias}?raw`)).text()).toBe(TEXT);
  });
});
