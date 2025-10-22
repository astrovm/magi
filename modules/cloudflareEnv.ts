import type { KVNamespace } from '@cloudflare/workers-types';

type Env = {
  links: KVNamespace;
};

export type { Env };
