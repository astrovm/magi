#!/usr/bin/env bash
# Builds the Worker, serves it locally with a throwaway database and runs the
# black-box HTTP tests against it. Extra arguments go to `bun test`.
set -euo pipefail
cd "$(dirname "$0")/.."

PORT="${PORT:-8812}"
STATE="$(mktemp -d)"
LOG="$STATE/wrangler.log"
WRANGLER="node_modules/.bin/wrangler"

cleanup() {
  [ -n "${SERVER:-}" ] && kill "$SERVER" 2>/dev/null
  wait 2>/dev/null
  rm -rf "$STATE"
}
trap cleanup EXIT

"$WRANGLER" dev --port "$PORT" --inspector-port $((PORT + 1000)) --persist-to "$STATE/kv" \
  --var TELEGRAM_BOT_TOKEN:token \
  --var TELEGRAM_WEBHOOK_SECRET:webhook-secret \
  --var TELEGRAM_BOT_USERNAME:MagiOrbBot >"$LOG" 2>&1 &
SERVER=$!

for _ in $(seq 1 240); do
  if [ "$(curl -s -m 1 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/orb")" = 200 ]; then
    READY=1
    break
  fi
  if ! kill -0 "$SERVER" 2>/dev/null; then
    break
  fi
  sleep 0.5
done
if [ -z "${READY:-}" ]; then
  echo "the site never came up:" >&2
  cat "$LOG" >&2
  exit 1
fi

BASE_URL="http://127.0.0.1:$PORT" bun test tests/http --timeout 20000 "$@"
