/*
 * The site lives on magi.4st.li and its links say s.4st.li. A browser keeps
 * "my links" and their manage keys per address, so the homepage on another
 * address moves home and brings them. They travel in the fragment, which never
 * reaches a server.
 */

const HOME = 'magi.4st.li';
const SHORT = 's.4st.li';
const OTHER_ADDRESSES = [SHORT, 'r.4st.li'];
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
function sayingShort(link) {
  const url = addressOf(link?.short);
  const ours = url?.protocol === 'https:' && (url.host === HOME || OTHER_ADDRESSES.includes(url.host));
  if (!ours || typeof link.alias !== 'string' || typeof link.key !== 'string') {
    return null;
  }
  return { ...link, short: `https://${SHORT}${url.pathname}` };
}

function moveHome(page) {
  // Only once, so a link forgotten at home stays forgotten.
  const links = readItem(page, MOVED_KEY) ? [] : parseLinks(readItem(page, MY_LINKS_KEY));
  writeItem(page, MOVED_KEY, '1');
  const carried = links.length > 0 ? `${HAND_OVER}${encodeURIComponent(JSON.stringify(links))}` : '';
  page.location.replace(`https://${HOME}/${page.location.search}${carried}`);
}

function linksHandedOver(page) {
  const { hash, pathname, search } = page.location;
  if (!hash.startsWith(HAND_OVER)) {
    return [];
  }
  page.history.replaceState(null, '', `${pathname}${search}`);
  // Anyone can craft this URL, so only our other addresses get to fill the list.
  if (!OTHER_ADDRESSES.includes(addressOf(page.document.referrer)?.host)) {
    return [];
  }
  try {
    return parseLinks(decodeURIComponent(hash.slice(HAND_OVER.length))).map(sayingShort).filter(Boolean);
  } catch {
    return [];
  }
}

function settleLinksAtHome(page) {
  const stored = readItem(page, MY_LINKS_KEY);
  const links = parseLinks(stored).map((link) => sayingShort(link) ?? link);
  const known = new Set(links.map((link) => String(link?.alias).toLowerCase()));
  const arriving = linksHandedOver(page).filter((link) => !known.has(link.alias.toLowerCase()));
  const newestFirst = arriving.length > 0
    ? [...links, ...arriving].sort((a, b) => (b?.created ?? 0) - (a?.created ?? 0))
    : links;
  const settled = JSON.stringify(newestFirst);
  if (links.length + arriving.length > 0 && settled !== stored) {
    writeItem(page, MY_LINKS_KEY, settled);
  }
}

export function settleHome(page) {
  if (OTHER_ADDRESSES.includes(page.location.host)) {
    moveHome(page);
  } else if (page.location.host === HOME) {
    settleLinksAtHome(page);
  }
}

if (globalThis.document) {
  settleHome(globalThis);
}
