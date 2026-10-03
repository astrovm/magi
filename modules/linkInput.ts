import { MAX_URL_LENGTH } from './commonFunctions';
import type { LinkMeta } from './kvHelpers';
import { readStringField } from './requestParsers';
import type { ResponseKey } from './responses';
import Url from './urlClass';

/**
 * Allowed lifetimes in seconds: 1 hour, 1 day, 1 week, 30 days.
 */
const EXPIRY_OPTIONS = ['3600', '86400', '604800', '2592000'];
const MAX_SELF_DESTRUCT = 1000000;
const MAX_PASSWORD_LENGTH = 256;
/** Extra fortune cookie destinations on top of the main link. */
const MAX_FORTUNES = 9;

type Spells = Omit<LinkMeta, 'created' | 'key'> & { password?: string };

const parseSpells = (formFields: FormData, now: number): Spells | null => {
  const burn = readStringField(formFields.get('burn')) ?? '';
  const expires = readStringField(formFields.get('expires')) ?? '';
  const password = readStringField(formFields.get('password')) ?? '';
  const spells: Spells = {};

  if (burn !== '') {
    const max = Number(burn);
    if (!Number.isInteger(max) || max < 1 || max > MAX_SELF_DESTRUCT) {
      return null;
    }
    spells.max = max;
  }

  if (expires !== '') {
    if (!EXPIRY_OPTIONS.includes(expires)) {
      return null;
    }
    spells.exp = Math.floor(now / 1000) + Number(expires);
  }

  if (password.length > MAX_PASSWORD_LENGTH) {
    return null;
  }
  if (password !== '') {
    spells.password = password;
  }
  if (formFields.get('cursed') === 'on') {
    spells.cursed = true;
  }
  if (formFields.get('count') === 'on') {
    spells.count = true;
  }
  if (formFields.get('wait') === 'on') {
    spells.wait = true;
  }
  if (formFields.get('hall') === 'on') {
    // The hall ranks links by visits, so it needs the counter.
    spells.hall = true;
    spells.count = true;
  }

  return spells;
};

const parseTarget = (urlField: string, ownHost: string): Url | ResponseKey => {
  let url: Url;
  try {
    url = Url.fromInput(urlField);
  } catch {
    return 'invalidUrl';
  }

  if (url.lengthIsGreaterThan(MAX_URL_LENGTH)) {
    return 'invalidUrl';
  }
  if (url.getHost() === ownHost) {
    return 'ownTail';
  }
  return url;
};

/**
 * Reads the fortune cookie list: extra links, one per line.
 */
const parseFortunes = (text: string, ownHost: string): string[] | ResponseKey => {
  const lines = text.split(/\r?\n/).map((line) => line.trim()).filter((line) => line !== '');
  if (lines.length > MAX_FORTUNES) {
    return 'badSpell';
  }
  const urls: string[] = [];
  for (const line of lines) {
    const url = parseTarget(line, ownHost);
    if (typeof url === 'string') {
      return url;
    }
    urls.push(url.get());
  }
  return urls;
};

export {
  EXPIRY_OPTIONS,
  MAX_FORTUNES,
  MAX_PASSWORD_LENGTH,
  MAX_SELF_DESTRUCT,
  parseFortunes,
  parseSpells,
  parseTarget,
};
export type { Spells };
