/**
 * Email-code login (premium only) and session tokens.
 *
 * A session token is `s_` plus 64 random hex characters. Only its SHA-256 is
 * stored, so a read of the database cannot be replayed as a login.
 */
import type { V2Env } from "./env";

const DAY = 86_400_000;
const CODE_TTL = 10 * 60_000;
const MAX_ATTEMPTS = 5;
const CODES_PER_HOUR = 5;
const SESSION_TTL = 90 * DAY;
const MAX_SESSIONS = 3;

/** Sends the login code. Tests set `env.MAILER`; production uses Resend. */
export interface Mailer {
  send(to: string, code: string): Promise<boolean>;
}

export function mailerFor(env: V2Env): Mailer | null {
  if (env.MAILER) return env.MAILER;
  if (env.RESEND_KEY && env.MAIL_FROM) {
    return {
      async send(to, code) {
        const r = await fetch("https://api.resend.com/emails", {
          method: "POST",
          headers: {
            Authorization: `Bearer ${env.RESEND_KEY}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            from: env.MAIL_FROM,
            to,
            subject: "Your Garmin Companion sign-in code",
            text: `Your sign-in code is ${code}. It works for 10 minutes.`,
          }),
        });
        return r.ok;
      },
    };
  }
  return null;
}

export async function sha256Hex(text: string): Promise<string> {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text)));
  return Array.from(d, (b) => b.toString(16).padStart(2, "0")).join("");
}

const randomHex = (bytes: number) =>
  Array.from(crypto.getRandomValues(new Uint8Array(bytes)), (b) =>
    b.toString(16).padStart(2, "0"),
  ).join("");

export const normalizeEmail = (raw: unknown): string | null => {
  const e = typeof raw === "string" ? raw.trim().toLowerCase() : "";
  return e.length <= 254 && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e) ? e : null;
};

export type SendResult = "sent" | "rate_limited" | "mail_unavailable" | "mail_failed";

export async function sendCode(env: V2Env, email: string, now = Date.now()): Promise<SendResult> {
  const mailer = mailerFor(env);
  if (!mailer) return "mail_unavailable";
  const recent = await env.DB.prepare(
    "SELECT COUNT(*) AS n FROM login_code WHERE email = ? AND created_at > ?",
  )
    .bind(email, now - 3_600_000)
    .first<{ n: number }>();
  if ((recent?.n ?? 0) >= CODES_PER_HOUR) return "rate_limited";

  // Six digits from a uniform random value; the modulo bias is below 1e-6.
  const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1_000_000).padStart(6, "0");
  const salt = randomHex(8);
  await env.DB.prepare(
    "INSERT INTO login_code (email, salt, code_hash, created_at, expires_at) VALUES (?, ?, ?, ?, ?)",
  )
    .bind(email, salt, await sha256Hex(`${salt}:${code}`), now, now + CODE_TTL)
    .run();
  return (await mailer.send(email, code)) ? "sent" : "mail_failed";
}

/** A new session token for the email, or null when the code is wrong or spent. */
export async function verifyCode(
  env: V2Env,
  email: string,
  code: string,
  now = Date.now(),
): Promise<{ token: string; userId: string } | null> {
  // Any live code for the email can match, not only the newest. Someone who
  // sends many codes to a known address then cannot lock the owner out: the
  // owner can use any of the mails. There are at most CODES_PER_HOUR live codes.
  const { results } = await env.DB.prepare(
    "SELECT id, salt, code_hash FROM login_code WHERE email = ? AND expires_at > ? AND attempts < ? ORDER BY id DESC",
  )
    .bind(email, now, MAX_ATTEMPTS)
    .all<{ id: number; salt: string; code_hash: string }>();

  let matched = false;
  for (const row of results) {
    // Counted before the compare, and only if still under the limit, so parallel
    // guesses cannot get past MAX_ATTEMPTS on any one code.
    const bumped = await env.DB.prepare(
      "UPDATE login_code SET attempts = attempts + 1 WHERE id = ? AND attempts < ?",
    )
      .bind(row.id, MAX_ATTEMPTS)
      .run();
    if (!bumped.meta.changes) continue;
    if ((await sha256Hex(`${row.salt}:${code}`)) === row.code_hash) matched = true;
  }
  if (!matched) return null;

  await env.DB.prepare("DELETE FROM login_code WHERE email = ?").bind(email).run();
  const userId = await upsertUser(env, email, now);
  return { token: await createSession(env, userId, now), userId };
}

async function upsertUser(env: V2Env, email: string, now: number): Promise<string> {
  await env.DB.prepare("INSERT OR IGNORE INTO user (id, email, created_at) VALUES (?, ?, ?)")
    .bind(randomHex(16), email, now)
    .run();
  const u = await env.DB.prepare("SELECT id FROM user WHERE email = ?")
    .bind(email)
    .first<{ id: string }>();
  return u?.id as string;
}

async function createSession(env: V2Env, userId: string, now: number): Promise<string> {
  const token = `s_${randomHex(32)}`;
  await env.DB.batch([
    env.DB.prepare(
      "INSERT INTO session (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    ).bind(await sha256Hex(token), userId, now, now + SESSION_TTL),
    // Three sessions at most: the oldest ones go.
    env.DB.prepare(
      "DELETE FROM session WHERE user_id = ? AND token_hash NOT IN (SELECT token_hash FROM session WHERE user_id = ? ORDER BY created_at DESC, rowid DESC LIMIT ?)",
    ).bind(userId, userId, MAX_SESSIONS),
  ]);
  return token;
}

export const isSessionToken = (t: string) => /^s_[0-9a-f]{64}$/.test(t);

export async function sessionUser(
  env: V2Env,
  token: string,
  now = Date.now(),
): Promise<{ id: string; email: string } | null> {
  const hash = await sha256Hex(token);
  const row = await env.DB.prepare(
    "SELECT u.id AS id, u.email AS email, s.expires_at AS expires_at FROM session s JOIN user u ON u.id = s.user_id WHERE s.token_hash = ?",
  )
    .bind(hash)
    .first<{ id: string; email: string; expires_at: number }>();
  if (!row || row.expires_at <= now) return null;
  // Rolling: a token in regular use never ages out.
  if (row.expires_at - now < SESSION_TTL - 30 * DAY) {
    await env.DB.prepare("UPDATE session SET expires_at = ? WHERE token_hash = ?")
      .bind(now + SESSION_TTL, hash)
      .run();
  }
  return { id: row.id, email: row.email };
}

export async function signOut(env: V2Env, token: string): Promise<void> {
  await env.DB.prepare("DELETE FROM session WHERE token_hash = ?")
    .bind(await sha256Hex(token))
    .run();
}
