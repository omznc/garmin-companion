# worker

The `coach` Cloudflare Worker: the cloud coach proxy to OpenRouter, the
credits, the email login and Polar billing. Read the root `CLAUDE.md` first.
`V2.md` is the contract with the apps. `README.md` is the setup.

## Commands

Use the `cf` CLI, not `wrangler`.

```bash
pnpm -C worker install --frozen-lockfile
pnpm -C worker test          # vitest, D1 on node:sqlite with the real migration
pnpm -C worker typecheck
pnpm -C worker dev           # cf dev
pnpm -C worker exec cf deploy
```

## Layout

- `src/index.ts`: `/health`, `/v2/*` to `handleV2`, `/v1/*` answers 410, a
  daily cron for `purgeExpired`.
- `src/v2/index.ts`: the routes (`/install`, `/balance`, `/me`,
  `/chat/completions`, `/auth/email/*`, `/auth/signout`, `/billing/*`,
  `/checkout/done`,
  `/webhooks/polar`). Errors use the OpenAI shape (`http.ts` `fail()`). The
  apps act on `error.type`.
- `wallet.ts`: one `Wallet` Durable Object for each install or user. A request
  holds credits. Then it settles the hold, or releases it. `X-Request-Id` stops a second charge.
- `limits.ts`: the `Limits` Durable Object. The install, account, mail and
  budget caps. A check and its count are one atomic step.
- `models.ts`: the model profiles. `MODEL_PROFILE` in `cloudflare.config.ts`
  chooses one.
- `billing.ts`: Polar only. The store sources exist in the `entitlement`
  table but nothing writes them.
- `auth.ts`: email codes through Resend. `migrations/`: the D1 schema.

## Rules

- **Never log or store message content.** Do not add an `observability` block
  to `cloudflare.config.ts`. Because of this, the `upstream_message` in a 502
  is the only way to see an upstream error.
- **Do not remove `OPENROUTER_KEY`.** It is also the salt of the network
  buckets and the fallback upstream key.
- **Never refund a cut stream,** or a caller can abort for free. A stream with
  no `data:` line for 60 s or more costs nothing.
- **The free and premium budgets are separate.** Free installs must not pause
  premium.
- **Caps live in the `Limits` Durable Object.** A KV check and increment is
  not atomic (`5e34202`).
- **A model id must exist on OpenRouter.** An unknown id in `models` fails the
  whole request with a 404.
- **`max_price` must match a real endpoint price,** or every request fails with
  "No endpoints found that satisfy the max price". Check the endpoint price
  before you change a cap.
- **Set `reasoning: {effort: "low"}`.** At the default effort a model spent the
  whole token budget on reasoning and wrote no answer.
- **Webhook ids are recorded after the event applies.** The write is
  idempotent through `last_event_at`.

## Gotchas

- `cf deploy` builds the folder it runs in. Run it with
  `pnpm -C worker exec cf deploy`. A run from the repository root made a new
  config, a lockfile and a root `node_modules`.
- Deploy only from a pushed commit.
- A rollback to a version from before the `Limits` Durable Object fails
  ("orphaned_provisioned_namespace"). Fix forward.
- pnpm 11 stops `pnpm install` with `ERR_PNPM_IGNORED_BUILDS` (esbuild,
  workerd). Put `allowBuilds: {esbuild: true, workerd: true}` in an untracked
  `worker/pnpm-workspace.yaml`. CI uses pnpm 10.
- CI does not deploy the worker.
