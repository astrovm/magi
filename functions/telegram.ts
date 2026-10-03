import type { PagesFunction } from '@cloudflare/workers-types';
import Alias from '../modules/aliasClass';
import type { Env } from '../modules/cloudflareEnv';
import { getLink, putLink } from '../modules/kvHelpers';
import { ORB_LINK_NOT_FOUND_MESSAGE, ORB_WRONG_KEY_MESSAGE } from '../modules/responses';
import { sha256Hex, timingSafeEqual } from '../modules/secrets';
import { parseTelegramCommand, sendTelegramMessage } from '../modules/telegram';
import type { TelegramCommand, TelegramUpdate } from '../modules/telegram';

const HELP_MESSAGE = [
  "hi, i'm the orb's snitch 🔮",
  'send /alert <alias> <manage key> and i\'ll ping you when someone opens that link.',
  'send it again to make me stop.',
].join('\n');

const toggleAlerts = async (env: Env, { chatId, args }: TelegramCommand): Promise<string> => {
  const [aliasArg, key] = args;
  if (!aliasArg || !key) {
    return 'usage: /alert <alias> <manage key>';
  }

  const alias = new Alias(aliasArg);
  alias.prepareForLookup('-');
  const aliasHash = await alias.getHash();
  const record = await getLink(env.links, aliasHash);
  if (record === null) {
    return ORB_LINK_NOT_FOUND_MESSAGE;
  }
  if (!record.meta.key || !timingSafeEqual(await sha256Hex(key), record.meta.key)) {
    return ORB_WRONG_KEY_MESSAGE;
  }

  const { tg, ...meta } = record.meta;
  const enable = tg !== chatId;
  await putLink(env.links, aliasHash, { url: record.url, meta: enable ? { ...meta, tg: chatId } : meta });
  return enable
    ? `👀 watching ${alias.get()}. i'll snitch on every visit.`
    : `😴 stopped watching ${alias.get()}.`;
};

const answer = (env: Env, command: TelegramCommand): Promise<string> | string => {
  if (command.command === 'alert') {
    return toggleAlerts(env, command);
  }
  if (command.command === 'getid') {
    return `chat id: ${command.chatId}`;
  }
  return HELP_MESSAGE;
};

export const onRequestPost: PagesFunction<Env> = async ({ env, request }) => {
  const secret = request.headers.get('X-Telegram-Bot-Api-Secret-Token') ?? '';
  if (!env.TELEGRAM_BOT_TOKEN || !env.TELEGRAM_WEBHOOK_SECRET || !timingSafeEqual(secret, env.TELEGRAM_WEBHOOK_SECRET)) {
    return new Response(null, { status: 404 });
  }

  const command = parseTelegramCommand(await request.json<TelegramUpdate>());
  if (command) {
    await sendTelegramMessage(env.TELEGRAM_BOT_TOKEN, command.chatId, await answer(env, command));
  }
  return new Response(null, { status: 200 });
};
