#!/usr/bin/env bash
# Tries the site on real Cloudflare, on its own Worker and KV namespace, then
# deletes both. It doesn't touch the real site. Needs `bunx wrangler login`.
#
# Real rate limits apply here and can't be faked with a header, so this runs the
# tests that only real Cloudflare can answer: Durable Object storage,
# hibernation and sockets. The full suite runs locally with test:http.
set -euo pipefail
cd "$(dirname "$0")/.."

NAME=magi-try
CONFIG=wrangler.try.jsonc
WRANGLER=node_modules/.bin/wrangler
TESTS='two tabs see|edits are saved|hibernates|sees edits live|lock stops|restore brings|banish wipes|other sites cannot'

if ! "$WRANGLER" whoami 2>&1 | grep -q 'associated with the email\|Account ID'; then
  echo "not logged in to Cloudflare. run: bunx wrangler login" >&2
  exit 1
fi

cleanup() {
  "$WRANGLER" delete --name "$NAME" --force >/dev/null 2>&1 || echo "couldn't delete the $NAME Worker" >&2
  if [ -n "${NAMESPACE:-}" ]; then
    "$WRANGLER" kv namespace delete --namespace-id "$NAMESPACE" >/dev/null 2>&1 ||
      echo "couldn't delete KV namespace $NAMESPACE" >&2
  fi
  rm -f "$CONFIG"
}
trap cleanup EXIT

NAMESPACE="$("$WRANGLER" kv namespace create "$NAME" 2>&1 | grep -oE '[0-9a-f]{32}' | head -1)"
if [ -z "$NAMESPACE" ]; then
  echo "couldn't create a KV namespace" >&2
  exit 1
fi

sed -e "s/\"name\": \"magi\"/\"name\": \"$NAME\"/" -e "s/\"id\": \"links\"/\"id\": \"$NAMESPACE\"/" wrangler.jsonc >"$CONFIG"
URL="$("$WRANGLER" deploy --config "$CONFIG" 2>&1 | tee /dev/stderr | grep -oE "https://$NAME\.[a-z0-9.-]+\.workers\.dev" | head -1)"
if [ -z "$URL" ]; then
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

BASE_URL="$URL" bun test tests/http/live.test.ts --timeout 60000 -t "$TESTS"
