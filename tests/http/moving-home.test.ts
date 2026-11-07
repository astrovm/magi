import { describe, expect, test } from 'bun:test';
import { settleHome } from '../../public/moving-home.js';

const LINKS_KEY = 'magi:links';

type SavedLink = { alias: string; short: string; key: string; created: number };

const saved = (alias: string, host = 'magi.4st.li', created = 1): SavedLink => ({
  alias,
  short: `https://${host}/${alias}`,
  key: `key-of-${alias}`,
  created,
});

function browserAt(host: string, stored: Record<string, string> = {}, hash = '', referrer = '') {
  const items = new Map(Object.entries(stored));
  const page = {
    movedTo: null as string | null,
    items,
    location: {
      host,
      pathname: '/',
      search: '',
      hash,
      replace(url: string) {
        page.movedTo = url;
      },
    },
    history: {
      replaceState(_state: unknown, _title: string, url: string) {
        page.location.hash = new URL(url, 'https://x.invalid').hash;
      },
    },
    localStorage: {
      getItem: (key: string) => items.get(key) ?? null,
      setItem: (key: string, value: string) => void items.set(key, value),
    },
    document: { referrer },
  };
  return page;
}

const linksIn = (page: ReturnType<typeof browserAt>): SavedLink[] =>
  JSON.parse(page.items.get(LINKS_KEY) ?? '[]');

/** Walks the real path: the old address hands over, the short address receives. */
function move(oldHost: string, links: SavedLink[], onShort: Record<string, string> = {}) {
  const old = browserAt(oldHost, { [LINKS_KEY]: JSON.stringify(links) });
  settleHome(old);
  const target = new URL(old.movedTo ?? '');
  const home = browserAt(target.host, onShort, target.hash, `https://${oldHost}/`);
  settleHome(home);
  return { old, home };
}

describe('the homepage moves to the short address', () => {
  test.each(['magi.4st.li', 'r.4st.li'])('from %s, keeping manage keys', (host) => {
    const { home } = move(host, [saved('mine', host)]);
    expect(home.movedTo).toBeNull();
    expect(linksIn(home)).toEqual([{ ...saved('mine', 's.4st.li') }]);
  });

  test('keeps its query string', () => {
    const old = browserAt('magi.4st.li');
    old.location.search = '?season=winter';
    settleHome(old);
    expect(old.movedTo).toBe('https://s.4st.li/?season=winter');
  });

  test('takes nothing along when nothing is saved', () => {
    const old = browserAt('magi.4st.li');
    settleHome(old);
    expect(old.movedTo).toBe('https://s.4st.li/');
  });

  test('takes nothing along when the saved list is broken', () => {
    const old = browserAt('magi.4st.li', { [LINKS_KEY]: 'not json' });
    settleHome(old);
    expect(old.movedTo).toBe('https://s.4st.li/');
  });

  test('leaves the keys behind too, for old live pages on that address', () => {
    const { old } = move('magi.4st.li', [saved('mine')]);
    expect(linksIn(old)).toEqual([saved('mine')]);
  });

  test('hands over only once, so a forgotten link stays forgotten', () => {
    const { old } = move('magi.4st.li', [saved('mine')]);
    settleHome(old);
    expect(old.movedTo).toBe('https://s.4st.li/');
  });

  test('still moves if the browser refuses to write the mark', () => {
    const old = browserAt('magi.4st.li', { [LINKS_KEY]: JSON.stringify([saved('mine')]) });
    old.localStorage.setItem = () => {
      throw new Error('storage is full');
    };
    settleHome(old);
    expect(old.movedTo).toContain('https://s.4st.li/#links=');
  });
});

describe('the short address receives them', () => {
  test('removes the keys from the address bar', () => {
    const { home } = move('magi.4st.li', [saved('mine')]);
    expect(home.location.hash).toBe('');
  });

  test('keeps what it already has, newest first', () => {
    const here = saved('here', 's.4st.li', 3);
    const clash = { ...saved('same', 's.4st.li', 2), key: 'the-key-here' };
    const { home } = move('magi.4st.li', [saved('SAME', 'magi.4st.li', 9), saved('old')], {
      [LINKS_KEY]: JSON.stringify([clash, here]),
    });
    expect(linksIn(home).map(({ alias, key }) => [alias, key])).toEqual([
      ['here', here.key],
      ['same', 'the-key-here'],
      ['old', 'key-of-old'],
    ]);
  });

  test('merges both old addresses', () => {
    const first = move('magi.4st.li', [saved('one')]);
    const second = move('r.4st.li', [saved('two', 'r.4st.li')], {
      [LINKS_KEY]: first.home.items.get(LINKS_KEY) ?? '',
    });
    expect(linksIn(second.home).map(({ alias }) => alias).sort()).toEqual(['one', 'two']);
  });

  test('ignores a crafted link from anywhere else', () => {
    const hash = `#links=${encodeURIComponent(JSON.stringify([saved('planted')]))}`;
    for (const referrer of ['', 'https://evil.example/', 'not a url']) {
      const home = browserAt('s.4st.li', {}, hash, referrer);
      settleHome(home);
      expect(home.items.has(LINKS_KEY)).toBe(false);
      expect(home.location.hash).toBe('');
    }
  });

  test('drops entries that are not our links', () => {
    const bad = [
      { alias: 'script', short: 'javascript:alert(1)', key: 'k', created: 1 },
      { alias: 'away', short: 'https://evil.example/away', key: 'k', created: 1 },
      { alias: 'plain', short: 'http://magi.4st.li/plain', key: 'k', created: 1 },
      { alias: 'nokey', short: 'https://magi.4st.li/nokey', created: 1 },
      { alias: 'broken', short: 'nope', key: 'k', created: 1 },
      { short: 'https://magi.4st.li/noalias', key: 'k', created: 1 },
      null,
    ] as unknown as SavedLink[];
    const { home } = move('magi.4st.li', [...bad, saved('good')]);
    expect(linksIn(home).map(({ alias }) => alias)).toEqual(['good']);
  });

  test.each(['%%%', `${encodeURIComponent('{"not":"a list"}')}`])('survives a bad hand-over: %s', (value) => {
    const home = browserAt('s.4st.li', {}, `#links=${value}`, 'https://magi.4st.li/');
    settleHome(home);
    expect(linksIn(home)).toEqual([]);
    expect(home.location.hash).toBe('');
  });

  test('leaves other fragments alone', () => {
    const home = browserAt('s.4st.li', {}, '#hello', 'https://magi.4st.li/');
    settleHome(home);
    expect(home.location.hash).toBe('#hello');
    expect(home.items.has(LINKS_KEY)).toBe(false);
  });
});

test('a try-out deploy stays where it is', () => {
  const page = browserAt('magi-try.workers.dev', { [LINKS_KEY]: JSON.stringify([saved('mine')]) });
  settleHome(page);
  expect(page.movedTo).toBeNull();
  expect(linksIn(page)).toEqual([saved('mine')]);
});
