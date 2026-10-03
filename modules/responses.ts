import { renderSVG } from 'uqr';
import { escapeHtml, renderPage } from './html';

const HTML_HEADERS = { 'Content-Type': 'text/html; charset=utf-8' } as const;

const ORB_REJECTED_REQUEST_MESSAGE = 'the orb rejected your request.';
const ORB_REJECTED_ALIAS_MESSAGE = 'the orb rejected your alias.';
const ORB_REJECTED_INVALID_URL_MESSAGE = 'the orb rejected your invalid url.';
const ORB_REJECTED_ALIAS_REWRITE_MESSAGE = 'the orb rejected your access to rewrite this link.';
const ORB_LINK_NOT_FOUND_MESSAGE = "the orb didn't found this link.";
const ORB_OWN_TAIL_MESSAGE = 'the orb refuses to eat its own tail.';
const ORB_WRONG_KEY_MESSAGE = "the orb doesn't recognize that manage key.";
const ORB_BAD_SPELL_MESSAGE = 'the orb rejected your weird spell settings.';
const ORB_NO_ALIAS_LEFT_MESSAGE = 'the worm ran out of ideas. try a custom alias.';
const ORB_DANGER_MESSAGE = 'the orb smells something fishy. that link looks like phishing or malware.';
const ORB_TOO_MANY_MESSAGE = 'the worm is exhausted. try again later.';
const ORB_NOT_HUMAN_MESSAGE = "the orb couldn't tell if you're human. try again.";
const WORM_LINK_CREATED_MESSAGE = 'the worm summoned your link';
const WORM_LINK_TRANSMUTED_MESSAGE = 'the worm transmuted your link.';
const WORM_LINK_BANISHED_MESSAGE = 'the worm banished your link to the shadow realm.';

type ResponseKey =
  | 'invalidRequest'
  | 'invalidAlias'
  | 'invalidUrl'
  | 'aliasLocked'
  | 'ownTail'
  | 'wrongKey'
  | 'badSpell'
  | 'noAliasLeft'
  | 'linkNotFound'
  | 'dangerUrl'
  | 'tooMany'
  | 'notHuman'
  | 'linkTransmuted'
  | 'linkBanished';

type Tone = 'error' | 'success';

const RESPONSES: Record<ResponseKey, { message: string; status: number; tone: Tone }> = {
  invalidRequest: { message: ORB_REJECTED_REQUEST_MESSAGE, status: 400, tone: 'error' },
  invalidAlias: { message: ORB_REJECTED_ALIAS_MESSAGE, status: 400, tone: 'error' },
  invalidUrl: { message: ORB_REJECTED_INVALID_URL_MESSAGE, status: 400, tone: 'error' },
  aliasLocked: { message: ORB_REJECTED_ALIAS_REWRITE_MESSAGE, status: 409, tone: 'error' },
  ownTail: { message: ORB_OWN_TAIL_MESSAGE, status: 400, tone: 'error' },
  wrongKey: { message: ORB_WRONG_KEY_MESSAGE, status: 403, tone: 'error' },
  badSpell: { message: ORB_BAD_SPELL_MESSAGE, status: 400, tone: 'error' },
  noAliasLeft: { message: ORB_NO_ALIAS_LEFT_MESSAGE, status: 503, tone: 'error' },
  linkNotFound: { message: ORB_LINK_NOT_FOUND_MESSAGE, status: 404, tone: 'error' },
  dangerUrl: { message: ORB_DANGER_MESSAGE, status: 400, tone: 'error' },
  tooMany: { message: ORB_TOO_MANY_MESSAGE, status: 429, tone: 'error' },
  notHuman: { message: ORB_NOT_HUMAN_MESSAGE, status: 403, tone: 'error' },
  linkTransmuted: { message: WORM_LINK_TRANSMUTED_MESSAGE, status: 200, tone: 'success' },
  linkBanished: { message: WORM_LINK_BANISHED_MESSAGE, status: 200, tone: 'success' },
};

const orbMessage = (message: string, tone: Tone): string =>
  `<p class="orb-msg orb-msg--${tone}">${message}</p>`;

/**
 * The homepage script asks for bare HTML fragments with `X-Orb: fragment`.
 * Plain form posts (no JS) get a full page instead.
 */
const wantsFragment = (request: Request): boolean => request.headers.get('X-Orb') === 'fragment';

const reply = (request: Request, body: string, status = 200): Response => {
  if (wantsFragment(request)) {
    return new Response(body, { status, headers: HTML_HEADERS });
  }
  return renderPage(body, { status });
};

const getResponse = (request: Request, key: ResponseKey): Response => {
  const { message, status, tone } = RESPONSES[key];
  return reply(request, orbMessage(message, tone), status);
};

type CreatedLink = {
  shortUrl: string;
  manageKey: string;
  alias: string;
  telegramBot?: string;
};

const copyRow = (content: string, value: string): string =>
  `<div class="copy-row">${content}<button type="button" class="copy-btn" data-copy="${escapeHtml(value)}">copy</button></div>`;

const linkCreatedHtml = ({ shortUrl, manageKey, alias, telegramBot }: CreatedLink): string => {
  const shortLabel = escapeHtml(shortUrl.replace(/^https?:\/\//, ''));
  const safeUrl = escapeHtml(shortUrl);
  const alertCommand = escapeHtml(`/alert ${alias} ${manageKey}`);
  const telegram = telegramBot
    ? `<p class="hint">📡 visit alerts: send <code>${alertCommand}</code> to <a href="https://t.me/${escapeHtml(telegramBot)}">@${escapeHtml(telegramBot)}</a></p>`
    : '';

  // The page script reads these to save the link under "my links".
  return `<div class="summoned" data-alias="${escapeHtml(alias)}" data-short="${safeUrl}" data-key="${escapeHtml(manageKey)}">
  ${orbMessage(WORM_LINK_CREATED_MESSAGE, 'success')}
  ${copyRow(`<a class="short-link" href="${safeUrl}">${shortLabel}</a>`, shortUrl)}
  <p class="hint">🔍 peek first: <a href="${safeUrl}+">${shortLabel}+</a></p>
  <details class="qr">
    <summary>📱 QR code</summary>
    <div class="qr-code">${renderSVG(shortUrl, { border: 2 })}</div>
  </details>
  <div class="scroll">
    <p class="hint">🔑 manage key. shown once, the worm has the memory of a worm.</p>
    ${copyRow(`<code class="secret">${escapeHtml(manageKey)}</code>`, manageKey)}
  </div>
  ${telegram}
</div>`;
};

const linkCreated = (request: Request, link: CreatedLink): Response =>
  reply(request, linkCreatedHtml(link), 201);

export {
  getResponse,
  HTML_HEADERS,
  linkCreated,
  orbMessage,
  reply,
  ORB_BAD_SPELL_MESSAGE,
  ORB_DANGER_MESSAGE,
  ORB_NOT_HUMAN_MESSAGE,
  ORB_TOO_MANY_MESSAGE,
  ORB_LINK_NOT_FOUND_MESSAGE,
  ORB_NO_ALIAS_LEFT_MESSAGE,
  ORB_OWN_TAIL_MESSAGE,
  ORB_REJECTED_ALIAS_MESSAGE,
  ORB_REJECTED_ALIAS_REWRITE_MESSAGE,
  ORB_REJECTED_INVALID_URL_MESSAGE,
  ORB_REJECTED_REQUEST_MESSAGE,
  ORB_WRONG_KEY_MESSAGE,
  WORM_LINK_BANISHED_MESSAGE,
  WORM_LINK_CREATED_MESSAGE,
  WORM_LINK_TRANSMUTED_MESSAGE,
  wantsFragment,
};
export type { CreatedLink, ResponseKey, Tone };
