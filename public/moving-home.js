/*
 * The site lives on s.4st.li. A browser keeps "my links" and their manage keys
 * per address, so the homepage on an old address moves there and brings them.
 * They travel in the fragment, which never reaches a server.
 */

const HOME = 's.4st.li';
const OLD_ADDRESSES = ['magi.4st.li', 'r.4st.li'];
const MY_LINKS_KEY = 'magi:links';
const MOVED_KEY = 'magi:links-moved';
const HAND_OVER = '#links=';

function readItem(page, key) {
  try {
    return page.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeItem(page, key, value) {
  try {
    page.localStorage.setItem(key, value);
  } catch {
    // Private mode or full storage: the list just won't persist.
  }
}

function parseLinks(text) {
  try {
    const links = JSON.parse(text ?? '[]');
    return Array.isArray(links) ? links : [];
  } catch {
    return [];
  }
}

function addressOf(url) {
  try {
    return new URL(url);
  } catch {
    return null;
  }
}

/** The link as it should be saved now, or null if it isn't one of ours. */
function onShortAddress(link) {
  const url = addressOf(link?.short);
  const ours = url?.protocol === 'https:' && (url.host === HOME || OLD_ADDRESSES.includes(url.host));
  if (!ours || typeof link.alias !== 'string' || typeof link.key !== 'string') {
    return null;
  }
  return { ...link, short: `https://${HOME}${url.pathname}` };
}

function moveHome(page) {
  // Only once: a link forgotten on the new address must stay forgotten. The
  // list stays here, because old live pages on this address still read it.
  const links = readItem(page, MOVED_KEY) ? [] : parseLinks(readItem(page, MY_LINKS_KEY));
  writeItem(page, MOVED_KEY, '1');
  const carried = links.length > 0 ? `${HAND_OVER}${encodeURIComponent(JSON.stringify(links))}` : '';
  page.location.replace(`https://${HOME}/${page.location.search}${carried}`);
}

function receiveLinks(page) {
  const { hash, pathname, search } = page.location;
  if (!hash.startsWith(HAND_OVER)) {
    return;
  }
  page.history.replaceState(null, '', `${pathname}${search}`);
  // Anyone can craft this URL, so only the old addresses get to fill the list.
  if (!OLD_ADDRESSES.includes(addressOf(page.document.referrer)?.host)) {
    return;
  }
  let carried;
  try {
    carried = parseLinks(decodeURIComponent(hash.slice(HAND_OVER.length)));
  } catch {
    return;
  }
  const links = parseLinks(readItem(page, MY_LINKS_KEY));
  const known = new Set(links.map((link) => String(link?.alias).toLowerCase()));
  const arriving = carried
    .map(onShortAddress)
    .filter((link) => link && !known.has(link.alias.toLowerCase()));
  if (arriving.length > 0) {
    const newestFirst = [...links, ...arriving].sort((a, b) => (b.created ?? 0) - (a.created ?? 0));
    writeItem(page, MY_LINKS_KEY, JSON.stringify(newestFirst));
  }
}

export function settleHome(page) {
  if (OLD_ADDRESSES.includes(page.location.host)) {
    moveHome(page);
  } else if (page.location.host === HOME) {
    receiveLinks(page);
  }
}

if (globalThis.document) {
  settleHome(globalThis);
}
