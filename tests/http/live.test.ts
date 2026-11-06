// Live pastes, with the same Yjs client code a browser runs.
import { afterEach, describe, expect, test } from 'bun:test';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';
import { go, post, uniqueAlias, WS_BASE } from './helpers';

const FIELD = 'codemirror';
const NOTICE_MESSAGE = 4;

type Tab = { doc: Y.Doc; text: Y.Text; provider: WebsocketProvider };

const open: Tab[] = [];

afterEach(() => {
  open.splice(0).forEach(({ provider, doc }) => {
    provider.destroy();
    doc.destroy();
  });
});

const until = async (check: () => boolean, what: string, ms = 8000): Promise<void> => {
  const deadline = Date.now() + ms;
  while (!check()) {
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for ${what}`);
    }
    await Bun.sleep(25);
  }
};

const connect = async (alias: string): Promise<Tab> => {
  const doc = new Y.Doc();
  const provider = new WebsocketProvider(`${WS_BASE}/paste/ws`, alias, doc, {
    WebSocketPolyfill: WebSocket as never,
    disableBc: true,
    connect: true,
  });
  // The page handles the room's notices. Here they just need somewhere to go.
  provider.messageHandlers[NOTICE_MESSAGE] = () => {};
  const tab = { doc, text: doc.getText(FIELD), provider };
  open.push(tab);
  await until(() => provider.synced, `${alias} to sync`);
  return tab;
};

const close = (tab: Tab): void => {
  tab.provider.destroy();
  tab.doc.destroy();
  open.splice(open.indexOf(tab), 1);
};

type Live = { alias: string; key: string; viewAlias: string; status: number };

const createLive = async (fields: Record<string, string> = {}): Promise<Live> => {
  const alias = fields.alias ?? uniqueAlias('live');
  const response = await post('/paste', { live: 'on', alias, text: '', ...fields });
  const html = await response.text();
  const key = html.match(/data-key="([^"]+)"/)?.[1] ?? '';
  const viewUrl = html.match(/view-only link<\/p><div class="copy-row"><a class="short-link" href="([^"]+)"/)?.[1] ?? '';
  return { alias, key, viewAlias: viewUrl.split('/').pop() ?? '', status: response.status };
};

const raw = async (alias: string): Promise<string> => (await go(`/${alias}?raw`)).text();

const manage = (live: Live, action: string, extra: Record<string, string> = {}): Promise<Response> =>
  post('/manage', { alias: live.alias, key: live.key, action, ...extra });

describe('creating a live paste', () => {
  test('starts with the given text and gives a separate view-only link', async () => {
    const live = await createLive({ text: 'hello\nthere' });
    expect(live.status).toBe(201);
    expect(live.key).not.toBe('');
    expect(live.viewAlias).not.toBe('');
    expect(live.viewAlias).not.toContain(live.alias);
    expect(await raw(live.alias)).toBe('hello\nthere');
    expect(await raw(live.viewAlias)).toBe('hello\nthere');
  });

  test('can start empty, unlike a normal paste', async () => {
    const live = await createLive();
    expect(live.status).toBe(201);
    expect(await raw(live.alias)).toBe('');
  });

  test('shows the editor to editors and viewers', async () => {
    const live = await createLive();
    const edit = await go(`/${live.alias}`);
    const editHtml = await edit.text();
    expect(edit.status).toBe(200);
    expect(editHtml).toContain('data-role="edit"');
    expect(editHtml).toContain('<script src="/live.js"');
    expect(editHtml).toContain(live.viewAlias);
    expect(edit.headers.get('content-security-policy')).toContain('connect-src');

    const viewHtml = await (await go(`/${live.viewAlias}`)).text();
    expect(viewHtml).toContain('data-role="view"');
    // A viewer must never find the way to the edit link.
    expect(viewHtml).not.toContain(live.alias);
    expect(viewHtml).not.toContain('live-owner');
  });

  test('peek says what it is without leaking the other link', async () => {
    const live = await createLive();
    expect(await (await go(`/${live.alias}%2B`)).text()).toContain('a live paste anyone can edit');
    const view = await (await go(`/${live.viewAlias}%2B`)).text();
    expect(view).toContain('view only');
    expect(view).not.toContain(live.alias);
  });

  test('takes up the alias', async () => {
    const live = await createLive();
    expect((await createLive({ alias: live.alias })).status).toBe(409);
  });

  test('the browser bundle is served', async () => {
    const response = await go('/live.js');
    expect(response.status).toBe(200);
    expect((await response.text()).length).toBeGreaterThan(1000);
  });
});

describe('editing together', () => {
  test('two tabs see each other type', async () => {
    const live = await createLive({ text: 'start' });
    const [ana, bo] = [await connect(live.alias), await connect(live.alias)];
    await until(() => ana.text.toString() === 'start' && bo.text.toString() === 'start', 'the first text');

    ana.text.insert(5, ' from ana');
    await until(() => bo.text.toString() === 'start from ana', 'ana to reach bo');
    bo.text.insert(0, 'bo: ');
    await until(() => ana.text.toString() === 'bo: start from ana', 'bo to reach ana');
  });

  test('five tabs typing at once all end up with the same text', async () => {
    const live = await createLive();
    const tabs = await Promise.all(Array.from({ length: 5 }, () => connect(live.alias)));
    const rounds = 40;
    await Promise.all(tabs.map(async (tab, who) => {
      for (let round = 0; round < rounds; round += 1) {
        tab.text.insert(Math.floor(Math.random() * (tab.text.length + 1)), `${who}`);
        await Bun.sleep(5);
      }
    }));
    const total = tabs.length * rounds;
    await until(() => tabs.every((tab) => tab.text.length === total), 'everyone to get every edit', 15000);
    const final = tabs[0].text.toString();
    tabs.forEach((tab) => expect(tab.text.toString()).toBe(final));
    expect(await raw(live.alias)).toBe(final);
  });

  test('deleting, emoji and non-ASCII survive the trip', async () => {
    const live = await createLive({ text: 'ñandú 🪱 worm' });
    const [ana, bo] = [await connect(live.alias), await connect(live.alias)];
    await until(() => ana.text.length > 0 && bo.text.length > 0, 'the text');
    ana.text.delete(0, 6);
    ana.text.insert(ana.text.length, ' 🔮');
    await until(() => bo.text.toString() === '🪱 worm 🔮', 'the edit');
    expect(await raw(live.alias)).toBe('🪱 worm 🔮');
  });

  test('edits are saved and come back after everyone leaves', async () => {
    const live = await createLive();
    const ana = await connect(live.alias);
    ana.text.insert(0, 'kept');
    await Bun.sleep(2600);
    close(ana);

    const bo = await connect(live.alias);
    await until(() => bo.text.toString() === 'kept', 'the saved text');
  });

  test('an offline tab syncs what it typed when it comes back', async () => {
    const live = await createLive();
    const [ana, bo] = [await connect(live.alias), await connect(live.alias)];
    ana.provider.disconnect();
    ana.text.insert(0, 'typed offline');
    await Bun.sleep(100);
    expect(bo.text.toString()).toBe('');
    ana.provider.connect();
    await until(() => bo.text.toString() === 'typed offline', 'the offline edit');
  });

  test('keeps working after the room hibernates and forgets everything', async () => {
    const live = await createLive({ text: 'before' });
    const [ana, bo] = [await connect(live.alias), await connect(live.alias)];
    await until(() => bo.text.length > 0, 'the text');
    await Bun.sleep(25000);
    ana.text.insert(ana.text.length, ' after');
    await until(() => bo.text.toString() === 'before after', 'the edit after hibernating', 10000);
  }, 45000);
});

describe('view-only link', () => {
  test('sees edits live but its own edits go nowhere', async () => {
    const live = await createLive({ text: 'read me' });
    const [editor, viewer] = [await connect(live.alias), await connect(live.viewAlias)];
    await until(() => viewer.text.toString() === 'read me', 'the text');

    editor.text.insert(0, 'do ');
    await until(() => viewer.text.toString() === 'do read me', 'the edit');

    viewer.text.insert(0, 'hacked ');
    await Bun.sleep(500);
    expect(editor.text.toString()).toBe('do read me');
    expect(await raw(live.alias)).toBe('do read me');
  });
});

describe('owner tools', () => {
  test('lock stops all edits until unlock, then they come through', async () => {
    const live = await createLive({ text: 'x' });
    const [ana, bo] = [await connect(live.alias), await connect(live.alias)];
    await until(() => bo.text.length === 1 && ana.text.length === 1, 'the text');

    expect((await manage(live, 'lock')).status).toBe(200);
    ana.text.insert(1, 'y');
    await Bun.sleep(400);
    expect(bo.text.toString()).toBe('x');
    expect(await raw(live.alias)).toBe('x');

    expect((await manage(live, 'unlock')).status).toBe(200);
    await until(() => bo.text.toString() === 'xy', 'the refused edit after unlocking');
  });

  test('a lock outlives a reconnect', async () => {
    const live = await createLive({ text: 'x' });
    await manage(live, 'lock');
    const ana = await connect(live.alias);
    await until(() => ana.text.length === 1, 'the text');
    ana.text.insert(1, 'y');
    await Bun.sleep(300);
    expect(await raw(live.alias)).toBe('x');
  });

  test('restore brings back an earlier version for open tabs too', async () => {
    const live = await createLive();
    const ana = await connect(live.alias);
    ana.text.insert(0, 'first version');
    await Bun.sleep(2600);
    ana.text.delete(0, ana.text.length);
    ana.text.insert(0, 'second version');
    await Bun.sleep(300);

    const list = await manage(live, 'snapshots');
    expect(list.status).toBe(200);
    const snapshots = await list.json() as { id: number; at: number; preview: string }[];
    const first = snapshots.find((snapshot) => snapshot.preview === 'first version');
    expect(first).toBeDefined();

    const restored = await manage(live, 'restore', { snapshot: String(first?.id) });
    expect(restored.status).toBe(200);
    await until(() => ana.text.toString() === 'first version', 'the restored text');
    expect(await raw(live.alias)).toBe('first version');

    // What was there before the restore is one more version to go back to.
    const after = await (await manage(live, 'snapshots')).json() as { preview: string }[];
    expect(after.some((snapshot) => snapshot.preview === 'second version')).toBe(true);
  });

  test('a version that does not exist is refused', async () => {
    const live = await createLive();
    expect((await manage(live, 'restore', { snapshot: '999' })).status).toBe(400);
    expect((await manage(live, 'restore', { snapshot: 'nope' })).status).toBe(400);
  });

  test('banish wipes the room, closes sockets and frees both links', async () => {
    const live = await createLive({ text: 'secret' });
    const ana = await connect(live.alias);
    const closed = new Promise<number>((resolve) => {
      ana.provider.on('connection-close', (event: CloseEvent | null) => resolve(event?.code ?? 0));
    });
    expect((await manage(live, 'banish')).status).toBe(200);
    expect(await closed).toBe(4404);
    ana.provider.disconnect();

    await Bun.sleep(100);
    expect((await go(`/${live.alias}`)).status).toBe(404);
    expect((await go(`/${live.viewAlias}`)).status).toBe(404);

    // The alias can be used again and nothing old is left in the new room.
    const again = await createLive({ alias: live.alias });
    expect(again.status).toBe(201);
    expect(await raw(live.alias)).toBe('');
  });

  test('the manage key is required for all of it', async () => {
    const live = await createLive({ text: 'x' });
    for (const action of ['lock', 'unlock', 'snapshots', 'restore', 'banish']) {
      const denied = await post('/manage', { alias: live.alias, key: 'nope', action });
      expect(denied.status).toBe(403);
    }
    expect(await raw(live.alias)).toBe('x');
  });

  test('transmute points at the editor, and normal links refuse live actions', async () => {
    const live = await createLive();
    expect((await manage(live, 'transmute', { text: 'x' })).status).toBe(400);
    const link = await post('/orb', { url: 'https://example.com', alias: uniqueAlias('plain') });
    const html = await link.text();
    const key = html.match(/data-key="([^"]+)"/)?.[1] ?? '';
    const alias = html.match(/data-alias="([^"]+)"/)?.[1] ?? '';
    expect((await post('/manage', { alias, key, action: 'lock' })).status).toBe(400);
  });
});

describe('abuse', () => {
  const rawSocket = async (alias: string): Promise<WebSocket> => {
    const socket = new WebSocket(`${WS_BASE}/paste/ws/${alias}`);
    socket.binaryType = 'arraybuffer';
    await new Promise((resolve, reject) => {
      socket.onopen = resolve;
      socket.onerror = reject;
    });
    return socket;
  };

  const closeCode = (socket: WebSocket): Promise<number> =>
    new Promise((resolve) => {
      socket.onclose = (event) => resolve(event.code);
    });

  test('a broken frame closes that socket and hurts nobody else', async () => {
    const live = await createLive({ text: 'safe' });
    const ana = await connect(live.alias);
    const attacker = await rawSocket(live.alias);
    const closed = closeCode(attacker);
    attacker.send(new Uint8Array([0, 2, 5, 255, 255, 255, 255, 255]));
    expect(await closed).toBe(4400);

    await until(() => ana.text.length === 4, 'the text');
    ana.text.insert(0, 'still ');
    await Bun.sleep(200);
    expect(await raw(live.alias)).toBe('still safe');
  });

  test('a flood closes the socket', async () => {
    const live = await createLive();
    const attacker = await rawSocket(live.alias);
    const closed = closeCode(attacker);
    for (let index = 0; index < 1000; index += 1) {
      attacker.send(new Uint8Array([3]));
    }
    expect(await closed).toBe(4429);
  });

  test('no more than 50 sockets per room', async () => {
    const live = await createLive();
    const sockets = await Promise.all(Array.from({ length: 50 }, () => rawSocket(live.alias)));
    await expect(rawSocket(live.alias)).rejects.toBeDefined();
    sockets.forEach((socket) => socket.close());
  });

  test('other sites cannot open sockets from their pages', async () => {
    const live = await createLive();
    const crossSite = await go(`/paste/ws/${live.alias}`, {
      headers: {
        Upgrade: 'websocket',
        Origin: 'https://evil.example',
        Connection: 'Upgrade',
        'Sec-WebSocket-Version': '13',
        'Sec-WebSocket-Key': 'dGhlIHNhbXBsZSBub25jZQ==',
      },
    });
    expect(crossSite.status).toBe(403);
  });

  test('sockets only open for live pastes', async () => {
    const link = await post('/orb', { url: 'https://example.com', alias: uniqueAlias('plain') });
    const alias = (await link.text()).match(/data-alias="([^"]+)"/)?.[1] ?? '';
    await expect(rawSocket(alias)).rejects.toBeDefined();
    await expect(rawSocket('nothing-here')).rejects.toBeDefined();
    expect((await go(`/paste/ws/${alias}`)).status).toBe(426);
  });
});
