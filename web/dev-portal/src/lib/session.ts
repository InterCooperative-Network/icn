/**
 * Session — interim implementation.
 *
 * Today: a signed cookie carrying the actor's GitHub login + org-membership
 * flag. Signing uses a server-side secret (`ZONE_SESSION_SECRET` env var, with
 * a dev fallback). Cookie is HTTPS-only, SameSite=Lax, short-lived.
 *
 * MIGRATION: when ICN identity is the auth path, the session payload becomes
 * `{ did, capabilities, exp }` and the signing key becomes a capability token
 * issued at sign-in. The session check is otherwise unchanged.
 */

import crypto from "node:crypto";
import type { Identity } from "./scope-policy.js";

const COOKIE_NAME = "icn_zone_session";
const MAX_AGE_SECONDS = 60 * 60 * 24 * 2; // 2 days

interface SessionPayload {
  /** GitHub login. */
  login: string;
  /** Member of InterCooperative-Network org. */
  orgMember: boolean;
  /** Issued-at, seconds since epoch. */
  iat: number;
}

function secret(): string {
  const s = process.env.ZONE_SESSION_SECRET;
  if (s && s.length >= 32) return s;
  // Dev fallback — sessions are invalidated whenever the server restarts.
  // Production deploy must set ZONE_SESSION_SECRET to a 32+ byte random.
  return "dev-fallback-secret-rotated-on-each-restart-" + process.pid;
}

function sign(payload: string): string {
  return crypto.createHmac("sha256", secret()).update(payload).digest("base64url");
}

function encode(payload: SessionPayload): string {
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  const sig = sign(body);
  return `${body}.${sig}`;
}

function decode(token: string): SessionPayload | null {
  const parts = token.split(".");
  if (parts.length !== 2) return null;
  const [body, sig] = parts;
  if (!body || !sig) return null;
  if (sign(body) !== sig) return null;
  try {
    const payload: SessionPayload = JSON.parse(Buffer.from(body, "base64url").toString("utf8"));
    if (!payload.iat || (Date.now() / 1000) - payload.iat > MAX_AGE_SECONDS) return null;
    return payload;
  } catch {
    return null;
  }
}

/** Parse cookies from a Request, return identity. */
export function identityFromRequest(req: Request): Identity {
  const cookie = req.headers.get("cookie") ?? "";
  const match = cookie.split(/;\s*/).find((p) => p.startsWith(COOKIE_NAME + "="));
  if (!match) return { kind: "anonymous" };
  const token = match.slice(COOKIE_NAME.length + 1);
  const payload = decode(token);
  if (!payload) return { kind: "anonymous" };
  return {
    kind: "github",
    login: payload.login,
    orgMember: payload.orgMember,
  };
}

/** Create a Set-Cookie header value for a fresh session. */
export function makeSetCookie(login: string, orgMember: boolean, secure = true): string {
  const payload: SessionPayload = {
    login,
    orgMember,
    iat: Math.floor(Date.now() / 1000),
  };
  const token = encode(payload);
  const attrs = [
    `${COOKIE_NAME}=${token}`,
    "Path=/",
    `Max-Age=${MAX_AGE_SECONDS}`,
    "HttpOnly",
    "SameSite=Lax",
  ];
  if (secure) attrs.push("Secure");
  return attrs.join("; ");
}

/** Header value to clear the session. */
export function clearCookie(secure = true): string {
  const attrs = [
    `${COOKIE_NAME}=`,
    "Path=/",
    "Max-Age=0",
    "HttpOnly",
    "SameSite=Lax",
  ];
  if (secure) attrs.push("Secure");
  return attrs.join("; ");
}
