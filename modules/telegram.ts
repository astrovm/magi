const TELEGRAM_API = 'https://api.telegram.org';

const sendTelegramMessage = async (token: string, chatId: number, text: string): Promise<void> => {
  await fetch(`${TELEGRAM_API}/bot${token}/sendMessage`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ chat_id: chatId, text, disable_web_page_preview: true }),
  });
};

const BROWSERS: Array<[RegExp, string]> = [
  [/bot|crawl|spider|preview/i, '🤖 a bot'],
  [/Firefox\//, 'Firefox'],
  [/Edg\//, 'Edge'],
  [/OPR\//, 'Opera'],
  [/Chrome\//, 'Chrome'],
  [/Safari\//, 'Safari'],
];

const SYSTEMS: Array<[RegExp, string]> = [
  [/Android/, 'Android'],
  [/iPhone|iPad/, 'iOS'],
  [/Windows/, 'Windows'],
  [/Mac OS X/, 'macOS'],
  [/Linux/, 'Linux'],
];

const firstMatch = (text: string, patterns: Array<[RegExp, string]>, fallback: string): string =>
  patterns.find(([pattern]) => pattern.test(text))?.[1] ?? fallback;

/**
 * Rough device summary for visit alerts. No IPs: the orb snitches, it doesn't dox.
 */
const describeDevice = (userAgent: string): string =>
  `${firstMatch(userAgent, BROWSERS, 'a mystery browser')} on ${firstMatch(userAgent, SYSTEMS, 'a mystery device')}`;

type VisitorCf = { city?: unknown; country?: unknown };

const describePlace = (cf: VisitorCf | undefined): string => {
  const place = [cf?.city, cf?.country].filter((part): part is string => typeof part === 'string' && part !== '');
  return place.length > 0 ? place.join(', ') : 'somewhere in the void';
};

type VisitAlert = {
  shortLink: string;
  target: string;
  request: Request;
};

const visitAlertText = ({ shortLink, target, request }: VisitAlert): string => {
  const cf = (request as Request & { cf?: VisitorCf }).cf;
  const userAgent = request.headers.get('User-Agent') ?? '';
  return [
    `🔮 someone opened ${shortLink}`,
    `🌍 ${describePlace(cf)}`,
    `🧭 ${describeDevice(userAgent)}`,
    `➡️ ${target}`,
  ].join('\n');
};

type TelegramUpdate = {
  message?: {
    text?: unknown;
    chat?: { id?: unknown };
  };
};

type TelegramCommand = {
  chatId: number;
  command: string;
  args: string[];
};

const parseTelegramCommand = (update: TelegramUpdate): TelegramCommand | null => {
  const text = update.message?.text;
  const chatId = update.message?.chat?.id;
  if (typeof text !== 'string' || typeof chatId !== 'number') {
    return null;
  }
  const [rawCommand, ...args] = text.trim().split(/\s+/);
  const command = rawCommand.replace(/@.*$/, '').replace(/^\//, '').toLowerCase();
  return { chatId, command, args };
};

export { describeDevice, describePlace, parseTelegramCommand, sendTelegramMessage, TELEGRAM_API, visitAlertText };
export type { TelegramCommand, TelegramUpdate, VisitAlert };
