-- Identity and billing for /v2. Balances are not here: they live in the Wallet
-- Durable Object. No message content is stored anywhere.

CREATE TABLE user (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL
);

-- Only the SHA-256 of the token is stored.
CREATE TABLE session (
  token_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
CREATE INDEX session_user ON session (user_id, created_at);

CREATE TABLE login_code (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  email TEXT NOT NULL,
  salt TEXT NOT NULL,
  code_hash TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
CREATE INDEX login_code_email ON login_code (email, created_at);

-- One row per subscription at one source: polar, app_store or play.
-- A user with several rows is still one premium user.
CREATE TABLE entitlement (
  source TEXT NOT NULL,
  external_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  status TEXT NOT NULL,
  current_period_end INTEGER,
  cancel_at_period_end INTEGER NOT NULL DEFAULT 0,
  past_due_since INTEGER,
  last_event_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (source, external_id)
);
CREATE INDEX entitlement_user ON entitlement (user_id);

-- The provider event id is the key, so a replay is a no-op. No body is stored.
CREATE TABLE webhook_event (
  source TEXT NOT NULL,
  event_id TEXT NOT NULL,
  type TEXT NOT NULL,
  received_at INTEGER NOT NULL,
  PRIMARY KEY (source, event_id)
);
