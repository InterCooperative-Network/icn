/**
 * Request middleware for icn.zone.
 *
 * Resolution order (per `README.md`):
 *   1. Shortlink table → 302 to resolved destination (with scope check for internal shortlinks)
 *   2. Page router (public pages) → render
 *   3. Page router (/inside/*) → check session; redirect to /sign-in if anonymous;
 *      otherwise check scope via policy oracle and record the access receipt.
 *   4. 404.
 *
 * The middleware sets `Astro.locals.identity` so pages and layouts can read
 * who's signed in without re-parsing cookies.
 */

import { defineMiddleware } from "astro:middleware";
import { resolveShortlink } from "./data/shortlinks.js";
import { identityFromRequest } from "./lib/session.js";
import { evaluate, type Scope } from "./lib/scope-policy.js";
import { record as recordReceipt } from "./lib/audit.js";

export const onRequest = defineMiddleware(async (ctx, next) => {
  const url = new URL(ctx.request.url);
  const path = url.pathname;
  const identity = identityFromRequest(ctx.request);

  // Make identity available to pages and layouts.
  ctx.locals.identity = identity;

  // 1) Shortlinks. Strip leading slash for matching.
  const pathNoSlash = path.replace(/^\//, "").replace(/\/$/, "");
  if (pathNoSlash) {
    const m = resolveShortlink(pathNoSlash);
    if (m) {
      if (m.entry.kind === "external") {
        return Response.redirect(m.resolved, 302);
      }
      // Internal shortlink: scope-check first if it requires one
      if (m.entry.kind === "internal" && m.entry.scope) {
        const decision = evaluate({ actor: identity, scope: m.entry.scope as Scope, resource: path });
        // Record the receipt (allow or deny — both leave a trace)
        recordReceipt({ actor: identity, scope: m.entry.scope as Scope, resource: path, decision });
        if (decision.decision === "deny") {
          if (identity.kind === "anonymous") {
            return Response.redirect(`${url.origin}/sign-in?next=${encodeURIComponent(m.resolved)}`, 302);
          }
          return Response.redirect(`${url.origin}/sign-in?denied=${encodeURIComponent(decision.reason)}`, 302);
        }
      }
      return Response.redirect(`${url.origin}${m.resolved}`, 302);
    }
  }

  // 2 & 3) Page router. Auth-gate everything under /inside/*.
  if (path.startsWith("/inside")) {
    if (identity.kind === "anonymous") {
      return Response.redirect(`${url.origin}/sign-in?next=${encodeURIComponent(path)}`, 302);
    }
    // The page itself declares the tighter scope (via ScopedPage layout).
    // Middleware only enforces the staff-or-higher baseline.
    const decision = evaluate({ actor: identity, scope: "staff", resource: path });
    recordReceipt({ actor: identity, scope: "staff", resource: path, decision });
    if (decision.decision === "deny") {
      return Response.redirect(`${url.origin}/sign-in?denied=${encodeURIComponent(decision.reason)}`, 302);
    }
  }

  // 4) Fall through to the page router.
  return next();
});
