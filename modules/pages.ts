import { escapeHtml, renderPage } from './html';
import type { LinkRecord } from './kvHelpers';
import { ORB_LINK_NOT_FOUND_MESSAGE, orbMessage } from './responses';

const PRIVATE_HEADERS = { 'Cache-Control': 'no-store' } as const;

const WRONG_PASSWORD_MESSAGE = 'wrong password. the worm is disappointed in you.';

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

const unlockPage = (alias: string, wrongPassword = false): Response => {
  const safeAlias = escapeHtml(alias);
  const error = wrongPassword ? orbMessage(WRONG_PASSWORD_MESSAGE, 'error') : '';
  return renderPage(
    `<img src="/orb.webp" alt="" class="card-art card-art--orb" />
<h1 class="card-title">🔒 this link is guarded</h1>
<p class="hint">say the magic word.</p>
<form method="post" action="/${safeAlias}" class="spell-form">
  <input type="password" name="password" placeholder="magic word" aria-label="Password" required autofocus />
  <button type="submit" class="btn">Unlock</button>
</form>
${error}`,
    { status: wrongPassword ? 403 : 200, title: 'guarded', headers: PRIVATE_HEADERS },
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
  const { url, meta } = record;
  const safeAlias = escapeHtml(alias);
  const destination = meta.pw
    ? fact('goes to', '🔒 a secret. the orb is sworn to silence.')
    : fact('goes to', `<a href="${escapeHtml(url)}" rel="noopener noreferrer nofollow" class="long-url">${escapeHtml(url)}</a>`);
  const facts = [
    destination,
    meta.created ? fact('born', formatDate(meta.created)) : '',
    meta.count || meta.max ? fact('visits', String(clicks)) : '',
    meta.max ? fact('self-destructs', `after ${Math.max(meta.max - clicks, 0)} more visits`) : '',
    meta.exp ? fact('expires', formatDateTime(meta.exp * 1000)) : '',
    meta.cursed ? fact('curse', '☠️ might rickroll you') : '',
    meta.tg ? fact('snitch', '📡 the owner gets visit alerts') : '',
  ].join('');

  return renderPage(
    `<h1 class="card-title">🔮 the orb reveals</h1>
<p class="short-link">${escapeHtml(host)}/${safeAlias}</p>
<dl class="facts">${facts}</dl>
<a href="/${safeAlias}" class="btn">take me there</a>`,
    { title: alias, headers: PRIVATE_HEADERS },
  );
};

export { notFoundPage, previewPage, PRIVATE_HEADERS, unlockPage, WRONG_PASSWORD_MESSAGE };
export type { PreviewDetails };
