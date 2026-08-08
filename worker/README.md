# The hosted coach

A Cloudflare Worker sitting in front of one OpenRouter key, so the desktop app
has an answer to "which model should read your data?" that doesn't start with
"go and make an account somewhere".

It is a proxy and close to nothing else. It hands each install an id, then
checks that a request carries one it issued, that the install is within its
limits, that the shared budget for the day isn't spent, and that the body is the
shape it should be — then it forwards to OpenRouter and streams the answer back.
It does not read the conversation, and it does not keep one.

## What it costs whoever runs it

Everything. That is the point of the option, and it is worth knowing what the
app actually does before pointing money at it:

- One question is up to **seven** requests — the model calls tools against the
  local cache and each round is a fresh completion.
- Every round re-sends the system prompt and the tool schemas, about two
  thousand tokens of fixed prefix. The app marks that prefix cacheable and this
  worker passes the marker through, which is what keeps it from being paid for
  seven times.
- An answer is followed by a small suggestions call, and the Weight screen
  generates a summary when the numbers behind it move.

`DAILY_BUDGET_USD` is the number that bounds all of it. Set it to what you can
afford to lose in a bad week, because the counters behind it are eventually
consistent and will undercount under load.

## You need the Workers Paid plan

Not a preference — the free plan allows **1,000 KV writes a day**, and this
writes one or two per request. At up to seven requests a question that is
roughly **70 questions a day across everyone**, after which the counters stop
updating and the budget ceiling stops meaning anything. The paid plan is $5/mo
and lifts the daily write cap.

KV also allows only one write per second *to the same key*, which is why the
day's spend is spread over `SPEND_SHARDS` keys and summed on read. Without that,
the counter meant to be the hard ceiling would be the one drifting furthest
below the truth.

## Setting it up

```sh
pnpm install

# The KV namespace the counters live in. Put the id it prints into wrangler.jsonc.
npx wrangler kv namespace create COUNTERS

# The key everything is billed to.
npx wrangler secret put OPENROUTER_KEY

npx wrangler deploy
```

Then point the app at it. `CLOUD_BASE` in `app/src-tauri/src/chat.rs` is the
deployed URL with `/v1` on the end, and it is overridable at build time:

```sh
GARMIN_CLOUD_BASE=http://localhost:8787/v1 pnpm tauri dev   # against wrangler dev
```

## Enrolment

An install asks for its id once, on the first hosted question:

```sh
curl -X POST https://coach.example.workers.dev/v1/install
# {"id":"0123456789abcdef0123456789abcdef"}
```

The app keeps that in the OS keyring and sends it as the bearer token from then
on. There is no reset for it in the UI, and no need for one — an id it doesn't
have is one it asks for again.

This used to be the other way round: the app generated its own id and the worker
accepted any 32 hex characters. That made every per-install limit below a
suggestion, because the way past one was to send different characters. Issuing
the ids doesn't make them precious, but it makes them finite, which is the
difference between a counter and a decoration.

What it does not do is prove the caller is the app, and it can't. A binary
anyone can download cannot keep a secret from the person running it, so anything
shipped in it to sign a request with is a secret they already have. The honest
version is what's here: ids cost an address and a wait, and the budget ceiling
is what actually can't be argued with.

## The limits, and what each one is actually for

| Limit | Where | What it stops |
|---|---|---|
| An id this worker issued | KV `install:<id>` | Anyone who found the URL and pointed curl at it |
| 3 new ids/min per address | `ISSUE` rate-limit binding | A script sitting on `/v1/install` |
| 3 new ids/day per address | KV `issued:<day>:<bucket>` | One machine collecting ids to spread its usage over |
| 200 new ids/day across everyone | KV `issued:<day>:total` | A botnet doing the same from many addresses |
| 30 req/min per install | `BURST` rate-limit binding | A client stuck in a retry loop |
| 400 req/day per install | KV `count:<day>:<id>` | One install quietly costing more than everyone else |
| `DAILY_BUDGET_USD` across everyone | KV `spend:<day>` | Everything else, whatever it turns out to be |
| One model, allowlisted fields | `sanitize()` | Callers choosing how much a request costs |

The budget ceiling is still the only unconditional one, and the global cap on
new ids is the only limit that can turn away someone innocent — a genuinely
popular day locks out new installs until midnight. It is set far above any real
day for a project this size, and the refusal says to come back tomorrow or use
their own key.

To block one install:

```sh
npx wrangler kv key put --binding COUNTERS "block:<install-id>" 1
```

A block is a 403 and the app does not enrol its way past one; a forgotten id is
a 401 carrying `"type": "unknown_install"`, which it does re-enrol on, because
the alternative is an install that can never ask anything again.

## What it knows about the people using it

An install id, a request count, a running dollar total — and, for a day at a
time, a hashed bucket per address that has asked for an id.

The id is 128 random bits with nothing behind it: no account, no email, no link
to a Garmin login. It is stored under `install:<id>` with the date it was last
used, which is what keeps an install from ageing out mid-winter.

The address bucket is the one thing here derived from something identifying, and
it exists because rate-limiting new ids by address is the only lever available
without asking people to make an account. It is a truncated SHA-256 of the
address, salted with the day and with the worker's own API key, under
`issued:<day>:<bucket>` with a two-day TTL. So it rotates nightly, it is not
reversible by walking the IPv4 space, and the raw address is never written
anywhere.

The bodies are health data — heart rates, sleep, weight, and whatever the model
asked the local cache for. They are checked for shape and size and forwarded.
Nothing here writes one down, and `wrangler.jsonc` has no `observability` block
on purpose, since that would persist request metadata for exactly these
requests. If you deploy this for other people, that promise is now yours to
keep, and it is worth saying somewhere they'll read it.
