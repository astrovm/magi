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

export { EXPIRY_OPTIONS, MAX_PASSWORD_LENGTH, MAX_SELF_DESTRUCT, parseSpells, parseTarget };
export type { Spells };
