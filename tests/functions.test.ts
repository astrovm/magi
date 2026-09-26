import { describe, expect, mock, test } from 'bun:test';
import { onRequestGet } from '../functions/[alias]';
import { onRequestPost } from '../functions/orb';
import { MAX_ALIAS_LENGTH, MAX_URL_LENGTH } from '../modules/commonFunctions';
import { MemoryKV, md5 } from './helpers';

type Handler = (context: unknown) => Promise<Response>;
const getAlias = onRequestGet as unknown as Handler;
const postOrb = onRequestPost as unknown as Handler;

const assetResponse = new Response('asset');

const makeEnv = () => {
  const links = new MemoryKV();
  const fetch = mock(async () => assetResponse);
  return { links, ASSETS: { fetch } };
};

const postRequest = (fields: Record<string, string | Blob>): Request => {
  const body = new FormData();
  Object.entries(fields).forEach(([key, value]) => body.append(key, value));
  return new Request('https://s.4st.li/orb', { method: 'POST', body });
};

describe('GET /:alias', () => {
  test('redirects a stored alias permanently', async () => {
    const env = makeEnv();
    env.links.store.set(md5('my-link'), 'https://example.com/target');
    const response = await getAlias({
      env,
      params: { alias: 'My%20Link' },
      request: new Request('https://s.4st.li/My%20Link'),
    });
    expect(response.status).toBe(301);
    expect(response.headers.get('Location')).toBe('https://example.com/target');
  });

  test('reports a missing link', async () => {
    const env = makeEnv();
    const response = await getAlias({
      env,
      params: { alias: 'nothing' },
      request: new Request('https://s.4st.li/nothing'),
    });
    expect(await response.text()).toBe("the orb didn't found this link.\n");
  });

  test('serves static assets for dotted paths', async () => {
    const env = makeEnv();
    const request = new Request('https://s.4st.li/style.css');
    const response = await getAlias({ env, params: { alias: 'style.css' }, request });
    expect(response).toBe(assetResponse);
    expect(env.ASSETS.fetch).toHaveBeenCalledWith(request);
    expect(env.links.getCalls).toHaveLength(0);
  });

  test('rejects missing or multi-segment aliases', async () => {
    const env = makeEnv();
    for (const alias of [undefined, '', ['a', 'b']]) {
      const response = await getAlias({
        env,
        params: { alias },
        request: new Request('https://s.4st.li/'),
      });
      expect(await response.text()).toBe('the orb rejected your request.\n');
    }
  });
});

describe('POST /orb', () => {
  test('stores a new link under the alias hash', async () => {
    const env = makeEnv();
    const response = await postOrb({
      env,
      request: postRequest({ alias: ' Cool%20 Link ', url: 'https://example.com' }),
    });
    expect(await response.text()).toBe(
      '<p>the worm summoned your link <a href="https://s.4st.li/Cool-Link">s.4st.li/Cool-Link</a></p>',
    );
    expect(env.links.store.get(md5('cool-link'))).toBe('https://example.com');
  });

  test('refuses to overwrite an existing alias', async () => {
    const env = makeEnv();
    env.links.store.set(md5('taken'), 'https://old.example');
    const response = await postOrb({
      env,
      request: postRequest({ alias: 'TAKEN', url: 'https://new.example' }),
    });
    expect(await response.text()).toBe(
      '<p>the orb rejected your access to rewrite this link.</p>',
    );
    expect(env.links.store.get(md5('taken'))).toBe('https://old.example');
  });

  test('rejects requests without both fields', async () => {
    const env = makeEnv();
    for (const fields of [{ alias: 'a' }, { url: 'https://example.com' }, {
      alias: new Blob(['a']),
      url: 'https://example.com',
    }]) {
      const response = await postOrb({ env, request: postRequest(fields) });
      expect(await response.text()).toBe('<p>the orb rejected your request.</p>');
    }
    expect(env.links.store.size).toBe(0);
  });

  test.each([
    ['a dotted alias', 'file.txt'],
    ['an alias with special characters', 'ñandú'],
    ['an alias that is too long', 'a'.repeat(MAX_ALIAS_LENGTH + 1)],
  ])('rejects %s', async (_label, alias) => {
    const env = makeEnv();
    const response = await postOrb({
      env,
      request: postRequest({ alias, url: 'https://example.com' }),
    });
    expect(await response.text()).toBe('<p>the orb rejected your alias.</p>');
    expect(env.links.store.size).toBe(0);
  });

  test.each([
    ['an unsupported scheme', 'ftp://example.com'],
    ['garbage', 'not a url'],
    ['a URL that is too long', `https://example.com/${'a'.repeat(MAX_URL_LENGTH)}`],
  ])('rejects %s', async (_label, url) => {
    const env = makeEnv();
    const response = await postOrb({ env, request: postRequest({ alias: 'ok', url }) });
    expect(await response.text()).toBe('<p>the orb rejected your invalid url.</p>');
    expect(env.links.store.size).toBe(0);
  });
});
