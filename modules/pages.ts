import { escapeHtml, renderPage } from './html';
import type { LinkRecord } from './kvHelpers';
import { ORB_LINK_NOT_FOUND_MESSAGE, orbMessage } from './responses';

const PRIVATE_HEADERS = { 'Cache-Control': 'no-store' } as const;

const WRONG_PASSWORD_MESSAGE = 'wrong password. the worm is disappointed in you.';
const LOCKED_OUT_MESSAGE = 'too many wrong guesses. the worm needs 15 minutes alone.';

/** Seconds a countdown link makes the visitor wait. */
const COUNTDOWN_SECONDS = 5;

const formatDate = (epochMs: number): string => new Date(epochMs).toISOString().slice(0, 10);

const formatDateTime = (epochMs: number): string =>
  `${new Date(epochMs).toISOString().replace('T', ' ').slice(0, 16)} UTC`;

const notFoundPage = (): Response =>
  renderPage(
    `<img src="/worm.webp" alt="" class="card-art card-art--sad" />
<h1 class="card-title">404</h1>
${orbMessage(ORB_LINK_NOT_FOUND_MESSAGE, 'error')}
<p class="hint">it expired, self-destructed, got banished, or never existed.</p>
<a href="/" class="btn">summon a new one</a>`,
    { status: 404, title: 'lost', headers: PRIVATE_HEADERS },
  );

type UnlockState = 'ask' | 'wrong' | 'locked';

const UNLOCK_STATES: Record<UnlockState, { status: number; error: string }> = {
  ask: { status: 200, error: '' },
  wrong: { status: 403, error: orbMessage(WRONG_PASSWORD_MESSAGE, 'error') },
  locked: { status: 429, error: orbMessage(LOCKED_OUT_MESSAGE, 'error') },
};

const unlockPage = (alias: string, state: UnlockState = 'ask'): Response => {
  const safeAlias = escapeHtml(alias);
  const { status, error } = UNLOCK_STATES[state];
  return renderPage(
    `<img src="/orb.webp" alt="" class="card-art card-art--orb" />
<h1 class="card-title">🔒 this link is guarded</h1>
<p class="hint">say the magic word.</p>
<form method="post" action="/${safeAlias}" class="spell-form">
  <input type="password" name="password" placeholder="magic word" aria-label="Password" required autofocus />
  <button type="submit" class="btn">Unlock</button>
</form>
${error}`,
    { status, title: 'guarded', headers: PRIVATE_HEADERS },
  );
};

type PreviewDetails = {
  alias: string;
  host: string;
  record: LinkRecord;
  clicks: number;
};

const fact = (label: string, value: string): string => `<dt>${label}</dt><dd>${value}</dd>`;

const previewPage = ({ alias, host, record, clicks }: PreviewDetails): Response => {
  const { meta } = record;
  const safeAlias = escapeHtml(alias);
  const destinations = [record.url, ...(record.more ?? [])]
    .map((url) => `<a href="${escapeHtml(url)}" rel="noopener noreferrer nofollow" class="long-url">${escapeHtml(url)}</a>`)
    .join('<br />');
  const destination = meta.pw
    ? fact('goes to', '🔒 a secret. the orb is sworn to silence.')
    : fact(record.more ? '🥠 goes to one of' : 'goes to', destinations);
  const facts = [
    destination,
    meta.created ? fact('born', formatDate(meta.created)) : '',
    meta.count || meta.max ? fact('visits', String(clicks)) : '',
    meta.max ? fact('self-destructs', `after ${Math.max(meta.max - clicks, 0)} more visits`) : '',
    meta.exp ? fact('expires', formatDateTime(meta.exp * 1000)) : '',
    meta.cursed ? fact('curse', '☠️ might rickroll you') : '',
    meta.tg ? fact('snitch', '📡 the owner gets visit alerts') : '',
    meta.wait ? fact('countdown', `⏳ ${COUNTDOWN_SECONDS} seconds of suspense`) : '',
  ].join('');

  return renderPage(
    `<h1 class="card-title">🔮 the orb reveals</h1>
<p class="short-link">${escapeHtml(host)}/${safeAlias}</p>
<dl class="facts">${facts}</dl>
<a href="/${safeAlias}" class="btn">take me there</a>`,
    { title: alias, headers: PRIVATE_HEADERS },
  );
};

/**
 * Makes the visitor wait while the worm "charges the orb". Works without JS
 * through the refresh header; the page script animates the count.
 */
const countdownPage = (target: string): Response => {
  const safeTarget = escapeHtml(target);
  return renderPage(
    `<div class="countdown" data-target="${safeTarget}">
<img src="/orb.webp" alt="" class="card-art card-art--orb card-art--charging" />
<h1 class="card-title">⚡ charging the orb</h1>
<p class="countdown-number" data-countdown>${COUNTDOWN_SECONDS}</p>
<a href="${safeTarget}" rel="noopener noreferrer nofollow" class="hint">skip the suspense</a>
</div>`,
    {
      title: 'charging',
      headers: { ...PRIVATE_HEADERS, Refresh: `${COUNTDOWN_SECONDS}; url=${target}` },
    },
  );
};

export {
  COUNTDOWN_SECONDS,
  countdownPage,
  LOCKED_OUT_MESSAGE,
  notFoundPage,
  previewPage,
  PRIVATE_HEADERS,
  unlockPage,
  WRONG_PASSWORD_MESSAGE,
};
export type { PreviewDetails, UnlockState };
