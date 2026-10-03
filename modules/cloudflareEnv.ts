import type { KVNamespace } from '@cloudflare/workers-types';

type Env = {
  links: KVNamespace;
  TELEGRAM_BOT_TOKEN?: string;
  TELEGRAM_BOT_USERNAME?: string;
  TELEGRAM_WEBHOOK_SECRET?: string;
  TURNSTILE_SITE_KEY?: string;
  TURNSTILE_SECRET_KEY?: string;
  SAFE_BROWSING_API_KEY?: string;
};

export type { Env };
