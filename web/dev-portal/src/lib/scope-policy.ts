/**
 * Scope Policy Oracle — interim implementation.
 *
 * This file is the access-decision center for icn.zone. Every gated page or
 * scope-aware operation routes through `evaluate()`.
 *
 * The shape intentionally mirrors `icn-kernel-api::PolicyOracle` in the main
 * Rust workspace. The portal is an "app" in the kernel/app separation sense;
 * the access rules it enforces are ICN policy decisions rendered in TypeScript.
 *
 * ============================================================================
 * INTERIM (today)
 * ============================================================================
 *  - Standing is held in `src/data/standing.ts` — a hand-maintained
 *    allowlist keyed by GitHub login.
 *  - The actor is a GitHub identity carried in a signed session cookie.
 *  - Decisions are made in-process; no network calls.
 *
 * ============================================================================
 * TARGET (when ICN substrate is ready)
 * ============================================================================
 *  - Standing is held in the trust graph and resolved through governance
 *    primitives. The actor is a DID (`did:icn:...`) verified through
 *    ICN identity.
 *  - This file delegates to ICN's actual PolicyOracle registry. Each scope
 *    becomes a domain string ("scope:staff", "scope:steward", etc.) routed
 *    through the registry. No domain logic stays in the portal — the
 *    portal just asks the kernel, "may this actor read this resource?"
 *
 * Plug points for the migration are marked `// MIGRATION:` throughout.
 *
 * See `content/scope-doctrine.md` for the full doctrine.
 */

import { standing } from "../data/standing.js";

// ============================================================================
// Types — these names match the kernel-app pattern intentionally.
// ============================================================================

/** The set of audience scopes recognized by icn.zone. */
export type Scope =
  | "public"             // no auth required; the nerd-surface default
  | "staff"              // signed in; member of InterCooperative-Network org
  | "contributor"        // staff + acknowledged contributor charter
  | "cooperative-member" // standing in a specific cooperative (today: nycn)
  | "steward";           // steward role for a surface (deploy, security)

export type Identity =
  | { kind: "anonymous" }
  | { kind: "github"; login: string; orgMember: boolean }
  // MIGRATION: when ICN identity lands, add:
  // | { kind: "icn"; did: string; capabilities: string[] }
  ;

export interface ScopeRequest {
  actor: Identity;
  scope: Scope;
  resource: string; // request path
}

export type PolicyDecision =
  | { decision: "allow"; authority: AuthorityHandle }
  | { decision: "deny"; reason: string };

/**
 * What gave the actor authority for this access. In the interim implementation
 * this is a short string identifying the rule that fired. In the target
 * implementation it will be a capability-token handle or a charter-clause
 * reference — same shape, richer content.
 */
export type AuthorityHandle = {
  kind: "scope-rule";
  rule: string;
};

// ============================================================================
// The oracle
// ============================================================================

/**
 * Evaluate a scope request. Pure function. No side effects — recording the
 * decision as a receipt is the caller's job (see `src/lib/audit.ts`).
 *
 * MIGRATION: this whole function body becomes
 *   `return await kernelOracle.evaluate({ domain: "scope:" + req.scope, ... })`
 * once ICN identity + PolicyOracle registry are in place.
 */
export function evaluate(req: ScopeRequest): PolicyDecision {
  // Public scope is always allowed. The audit shelf still records visits at a
  // coarser granularity for the page-view counter — but no per-actor receipt.
  if (req.scope === "public") {
    return { decision: "allow", authority: { kind: "scope-rule", rule: "public-scope" } };
  }

  // Everything else needs a signed-in actor.
  if (req.actor.kind === "anonymous") {
    return { decision: "deny", reason: "sign-in required" };
  }

  // Staff: org membership is the credential.
  if (req.scope === "staff") {
    // MIGRATION: replace `orgMember` with capability check against the
    // contributor-charter capability issued at sign-in.
    if (req.actor.kind === "github" && req.actor.orgMember) {
      return { decision: "allow", authority: { kind: "scope-rule", rule: "github-org-member" } };
    }
    return { decision: "deny", reason: "not a member of InterCooperative-Network org" };
  }

  // Tighter scopes look up the allowlist in src/data/standing.ts.
  const login = req.actor.kind === "github" ? req.actor.login : null;
  if (!login) return { decision: "deny", reason: "unrecognized identity kind" };

  const grants = standing[login] ?? {};

  if (req.scope === "contributor") {
    if (grants.contributor) {
      return { decision: "allow", authority: { kind: "scope-rule", rule: "contributor-charter-acknowledged" } };
    }
    return { decision: "deny", reason: "contributor charter not acknowledged" };
  }

  if (req.scope === "cooperative-member") {
    if (grants.cooperativeMember && grants.cooperativeMember.length > 0) {
      return {
        decision: "allow",
        authority: { kind: "scope-rule", rule: `cooperative-member:${grants.cooperativeMember.join(",")}` },
      };
    }
    return { decision: "deny", reason: "no cooperative-member standing" };
  }

  if (req.scope === "steward") {
    if (grants.steward && grants.steward.length > 0) {
      return {
        decision: "allow",
        authority: { kind: "scope-rule", rule: `steward:${grants.steward.join(",")}` },
      };
    }
    return { decision: "deny", reason: "no steward standing" };
  }

  // Exhaustiveness check.
  const _exhaustive: never = req.scope;
  return { decision: "deny", reason: `unknown scope: ${String(_exhaustive)}` };
}

/** Human-readable label for a scope (used by ScopeBadge component). */
export function scopeLabel(scope: Scope): string {
  switch (scope) {
    case "public": return "Public";
    case "staff": return "Staff";
    case "contributor": return "Contributor";
    case "cooperative-member": return "Coop Member";
    case "steward": return "Steward";
  }
}

/** CSS class hint for the badge color. */
export function scopeTone(scope: Scope): "neutral" | "info" | "warn" | "critical" {
  switch (scope) {
    case "public": return "neutral";
    case "staff": return "info";
    case "contributor": return "info";
    case "cooperative-member": return "warn";
    case "steward": return "critical";
  }
}
