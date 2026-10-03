import type { PagesFunction } from '@cloudflare/workers-types';
import type { Env } from '../modules/cloudflareEnv';
import { listHall } from '../modules/kvHelpers';
import { hallPage } from '../modules/pages';

const HALL_SIZE = 20;

export const onRequestGet: PagesFunction<Env> = async ({ env, request }) =>
  hallPage(await listHall(env.links, HALL_SIZE), new URL(request.url).host);
