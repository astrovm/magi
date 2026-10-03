# ![Magi](/public/logo3d.webp)

A wizard worm that powers an orb.
It shrinks links. Mostly.

![Wizard worm](/public/magi.webp)

## ✨ Spells

- **Custom or random aliases**: leave the alias empty and the worm picks one, like `sneaky-worm-42`.
- **Paste without `https://`**: `example.com` works.
- **Peek**: add `+` to any link (`s.4st.li/cool+`) to see where it goes before clicking.
- **Password**: visitors have to say the magic word.
- **Self-destruct**: the link dies after N visits.
- **Expiration**: 1 hour, 1 day, 1 week or 30 days.
- **Count visits**: shown on the peek page.
- **Curse**: 10% of visits get rickrolled.
- **Manage key**: shown once when you create a link. Use it to **Transmute** (change the destination) or **Banish** (delete) the link.
- **Telegram alerts**: get a message with the city, country and browser of each visit. No IPs.
- **Fortune cookie**: each visit goes to a random link from your list.
- **Countdown**: visitors wait 5 seconds while the worm charges the orb.
- **QR code** for every new link.
- **My links**: your links and manage keys, saved only in your browser.
- Poke the worm. Leave it alone and it falls asleep. Try the Konami code. Visit in October or December.

## 🛡️ Abuse protection

- Each visitor can create 30 links per hour.
- 5 wrong passwords lock that visitor out of the link for 15 minutes.
- **Turnstile** (optional): set `TURNSTILE_SITE_KEY` and `TURNSTILE_SECRET_KEY` to ask for a human check before creating links.
- **Safe Browsing** (optional): set `SAFE_BROWSING_API_KEY` to refuse links Google flags as phishing or malware.

Visit counts and self-destructs are approximate: Workers KV has no atomic counters and caches reads for up to 5 minutes.

## 🛠️ Development

```sh
bun install
bun run start   # http://localhost:8788
bun run lint && bun run tsc && bun run test
```

## 📡 Telegram alerts

Optional. Without these variables the bot stays off and the site works the same.

1. Create a bot with [@BotFather](https://t.me/BotFather).
2. In the Pages project, set:
   - `TELEGRAM_BOT_TOKEN`: the bot token (secret).
   - `TELEGRAM_WEBHOOK_SECRET`: any random string (secret).
   - `TELEGRAM_BOT_USERNAME`: the bot username, without `@`.
3. Point the bot at the site:

   ```sh
   curl "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/setWebhook" \
     -d url=https://s.4st.li/telegram \
     -d secret_token="$TELEGRAM_WEBHOOK_SECRET"
   ```

Then send `/alert <alias> <manage key>` to the bot. Send it again to stop.
