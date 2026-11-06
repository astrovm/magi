#!/usr/bin/env bash
# Tries the site on real Cloudflare, on its own Worker and KV namespace, then
# deletes both. It doesn't touch the real site. Needs `bunx wrangler login`.
#
# Real rate limits apply here and can't be faked with a header, so this runs the
# tests that only real Cloudflare can answer: Durable Object storage,
# hibernation and sockets. The full suite runs locally with test:http.
# Pass a command to run that instead of the tests, with BASE_URL set.
set -euo pipefail
cd "$(dirname "$0")/.."

NAME=magi-try
CONFIG=wrangler.try.jsonc
WRANGLER=node_modules/.bin/wrangler
TESTS='two tabs see|edits are saved|hibernates|sees edits live|lock stops|restore brings|banish wipes|other sites cannot'

LOG="$(mktemp)"

if ! "$WRANGLER" whoami >"$LOG" 2>&1; then
  cat "$LOG" >&2
  echo "not logged in to Cloudflare. run: bunx wrangler login" >&2
  exit 1
fi

cleanup() {
  if [ -n "${DEPLOYED:-}" ] && ! "$WRANGLER" delete --config "$CONFIG" --force >"$LOG" 2>&1; then
    cat "$LOG" >&2
    echo "couldn't delete the $NAME Worker" >&2
  fi
  if [ -n "${NAMESPACE:-}" ] && ! "$WRANGLER" kv namespace delete --namespace-id "$NAMESPACE" >"$LOG" 2>&1 </dev/null; then
    cat "$LOG" >&2
    echo "couldn't delete KV namespace $NAMESPACE" >&2
  fi
  rm -f "$CONFIG" "$LOG"
}
trap cleanup EXIT
trap 'exit 1' INT TERM

if ! "$WRANGLER" kv namespace create "$NAME" >"$LOG" 2>&1 </dev/null; then
  cat "$LOG" >&2
  echo "couldn't create a KV namespace" >&2
  exit 1
fi
NAMESPACE="$(grep -oE '"id": "[0-9a-f]{32}"' "$LOG" | grep -oE '[0-9a-f]{32}')"

sed -e "s/\"name\": \"magi\"/\"name\": \"$NAME\"/" -e "s/\"id\": \"links\"/\"id\": \"$NAMESPACE\"/" wrangler.jsonc >"$CONFIG"
DEPLOYED=1
if ! "$WRANGLER" deploy --config "$CONFIG" >"$LOG" 2>&1 </dev/null; then
  cat "$LOG" >&2
  echo "the deploy failed" >&2
  exit 1
fi
URL="$(grep -oE "https://$NAME\.[a-z0-9.-]+\.workers\.dev" "$LOG" | head -1)"
if [ -z "$URL" ]; then
  cat "$LOG" >&2
  echo "the deploy gave no URL" >&2
  exit 1
fi
echo "deployed to $URL"

for _ in $(seq 1 30); do
  [ "$(curl -s -m 5 -o /dev/null -w '%{http_code}' "$URL/")" = 200 ] && break
  sleep 2
done

ALIAS="try-$(date +%s)"
curl -sf -o /dev/null -F url=https://example.com/ -F alias="$ALIAS" "$URL/orb"
sleep 2
curl -s -o /dev/null -w "link redirects with %{http_code}\n" "$URL/$ALIAS"

export BASE_URL="$URL"
if [ "$#" -gt 0 ]; then
  "$@"
else
  bun test tests/http/live.test.ts --timeout 60000 -t "$TESTS"
fi
