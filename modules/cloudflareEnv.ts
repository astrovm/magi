import type { KVNamespace } from '@cloudflare/workers-types';

type Env = {
  links: KVNamespace;
  TELEGRAM_BOT_TOKEN?: string;
  TELEGRAM_BOT_USERNAME?: string;
  TELEGRAM_WEBHOOK_SECRET?: string;
};

export type { Env };
