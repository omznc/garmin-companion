# Sync, the Garmin client and sample mode

Read this before you change `garmin-core` sync, the Garmin client, the day
keys, sample mode, or before you add a view command.

## The Garmin client (`crates/garmin-core/src/client.rs`)

- A GET has 3 tries on a transport error, a 429 or a 5xx, with a 1 s and a
  3 s pause. The client obeys `Retry-After`, to a maximum of 30 s.
- A write is never retried. The only write is `create_workout`, and only a
  button press starts it. A timer, a sync or the model never starts it.
- A connect error or a timeout becomes `ApiError::Offline`. The mobile
  `QueryClient` retries only transport errors, because the core already
  retries the rest.

## When a sync stops (`sync.rs`)

- A 429, a transport error or `AuthFailed` (a dead token) stops the sync.
- A 5xx is a warning. Three days of 5xx in a row (`OUTAGE_DAYS`) stop the
  walk.
- 45 empty days in a row stop the walk back in time.
- The extras (`sync_extra.rs`) never fail the whole sync. A 403, a 404 or a
  204 means "no data". A fatal error in an extra becomes a warning.
- A 200 answer with an unknown shape stores nothing.
- The sync is idempotent. A cleared `history-import` marker on mobile means a
  complete history, because of the stop rules above.

## Day keys

Use the local day for a day key, never the UTC day. This bug came back twice
(`e685bae`, `f7a00b2`). `last_sync` stays in UTC. An "as of" filter keeps rows
with a NULL `local_date`.

## Garmin data quirks

- `averageHR: 0` means "no value", not zero.
- Exercise sets can have a duplicate or a missing `messageIndex`. Strength sets
  have no weight.
- A NULL VO2 max means "no outdoor GPS run". It does not mean low fitness.
- Nobody has seen the meal, plan or event shapes with real data. Keep every
  field optional.
- Watch data is only as new as the last upload from Garmin Connect. Read
  `lastUsedDeviceUploadTime` (from `mylastused`). The data age hint compares
  it with the time of that read (`watch_checked_at`), not with now.
- Garmin has no push API for this app, and a server-side poll is not
  possible. Do not propose either.

## Sample mode

Sample mode lets a person (and a store reviewer) use the app with no Garmin
account. It broke about six times, always in the same way.

- **Sample mode never creates or opens the cache file.** A cache file ends
  sample mode.
- A new view command needs a sample answer in `garmin_api::sample`, and it must
  pass the test that runs each command with no cache.
- In `garmin-ffi`, fixture mode does nothing when tokens exist. A sign-in ends
  sample mode and deletes the `sample-mode` marker.
- On Android, the data folder is the parent of the Expo `documents` folder
  (the old Tauri data folder), so old installs keep their cache and login. The
  `sample-mode` marker stays in `files/`.

## Mobile background sync

- Android 15 blocks the network of a background app. The full history import
  runs in a `dataSync` foreground service (`apps/mobile/modules/sync-service`).
  Only the import resumes in the foreground. A general "resume any sync" was
  tried and removed (`da55158`) because of races.
- The background task (`apps/mobile/src/notify/backgroundSync.ts`) runs every
  180 minutes. The core `sync.lock` stops two syncs at the same time.
