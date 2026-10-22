# The hosted coach

A Cloudflare Worker in front of one OpenRouter key. The apps use it as the
default answer to "which model should read your data?". The user does not need
an account at another service.

The worker serves the paid coach under `/v2`. The contract for the app is in
[V2.md](V2.md). The worker does not read the conversation, and it does not keep
one.

## Routes

- `/health` answers `ok`.
- `/v2/*` is the coach. See [V2.md](V2.md).
- `/v1/*` answers `410 Gone` with a JSON body that tells the user to update the
  app. The old v0.9.1 desktop app used these routes. They were removed so that
  the old app gets a clear answer.

## What it costs whoever runs it

Everything. Know what the app does before you put money behind it:

- One question is up to **seven** requests. The model calls tools against the
  local cache, and each round is a new completion.
- Each round sends the system prompt and the tool schemas again, about two
  thousand tokens. The app marks that prefix as cacheable, and the worker
  passes the marker through.
- An answer is followed by a small suggestions call.

Set a spend limit on the OpenRouter key. The `DAILY_BUDGET_V2_USD` guard in the
worker has a soft edge.

## You need the Workers Paid plan

The free plan allows 1,000 KV writes a day. The paid plan lifts that cap. The
worker also uses Durable Objects.

## Setting it up

```sh
pnpm install

# The KV namespace for install ids. Put the id it prints into cloudflare.config.ts.
cf kv namespaces create --title COUNTERS

# The key everything is billed to. See `cf workers secrets update --help`.
cf workers secrets update OPENROUTER_KEY --worker coach --type secret_text --text "<key>"

cf deploy
```

The config is `cloudflare.config.ts` (made with `cf migrate`). `cf deploy`
needs the `wrangler` dev dependency at 4.136 or newer.

`OPENROUTER_KEY` is also the salt for the address buckets in `src/v2/index.ts`,
and the fallback upstream key when `OPENROUTER_KEY_V2` is not set. Do not
remove it.

## /v2, the paid coach

`src/v2/` holds the paid coach.

- Credits live in one `Wallet` Durable Object per install or user. Users,
  sessions, entitlements and webhook ids live in D1 (`migrations/`).
- The model comes from `MODEL_PROFILE` (`haiku` or `deepseek`) in
  `src/v2/models.ts`. The body cap, prices and reasoning setting come from there.
- Billing is Polar (production). Set the secrets `POLAR_TOKEN` and
  `POLAR_WEBHOOK_SECRET`. The Polar webhook URL is `<worker>/v2/webhooks/polar`.
  Events: `subscription.created`, `.active`, `.updated`, `.canceled`,
  `.uncanceled`, `.revoked`, and `order.refunded`.
- Login mail uses Resend. Set `RESEND_KEY` and `MAIL_FROM` (a sender on a domain
  with SPF and DKIM). Until both are set, `/v2/auth/email/send` answers 503.
- Apply the schema with `cf d1 migrations apply <database id>`.
- Attach the Anthropic key in the OpenRouter account (BYOK), and set a spend
  limit on the OpenRouter key and in the Anthropic Console.

The app uses `CLOUD_BASE` in `crates/garmin-coach/src/cloud.rs`. It is the
deployed URL with `/v2` on the end.

## What it knows about the people using it

An install id, a wallet balance, and, for one day at a time, a hashed bucket per
address that asked for an id.

The id is 128 random bits. It has no account, no email and no link to a Garmin
login. The address bucket is a truncated SHA-256 of the address. It is salted
with the day and with the worker's own API key. So it changes each night, and
nobody can reverse it by walking the IPv4 space. The raw address is never
written.

The bodies are health data: heart rates, sleep, weight, and what the model
asked the local cache for. The worker checks the shape and size, and forwards
them. Nothing writes one down. `cloudflare.config.ts` has no `observability`
block on purpose, because it would keep request metadata for these requests. If
you deploy this for other people, you must keep that promise.

To block one install, write the key `v2block:<install-id>` with the value `1`
in the `COUNTERS` namespace. Use `cf kv keys put`.
