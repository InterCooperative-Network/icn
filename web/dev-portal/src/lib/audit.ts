/**
 * Audit receipts — interim implementation.
 *
 * Every scope decision (allow or deny) writes a receipt. In the interim,
 * receipts are JSON-Lines records in `runtime/access.log`. Each line is a
 * complete record.
 *
 * MIGRATION: receipts become ADR-0026 receipts routed through the gateway's
 * opaque receipt storage cascade (#1755 / #1757 / #1758 / #1759). The class
 * string is `"access_receipt"`; `key1` is the actor identity; `key2` is the
 * resource path. The cascade preserves auditability without imposing a typed
 * gateway field — see icn/crates/icn-gateway/src/receipt_store.rs.
 */

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

import type { Identity, Scope, PolicyDecision, AuthorityHandle } from "./scope-policy.js";

export interface AccessReceipt {
  /** Schema marker — when this changes, the receipt structure changed. */
  schema: "access-receipt/v0";
  /** Who asked. */
  actor: Identity;
  /** What scope they claimed. */
  scope: Scope;
  /** What path they tried to read. */
  resource: string;
  /** Allow or deny. */
  decision: "allow" | "deny";
  /** Oracle's reason or authority handle. */
  authority?: AuthorityHandle;
  reason?: string;
  /** When (ISO 8601). */
  recorded_at: string;
  /** Content-address of this receipt, computed over the JSON-canonicalized record. */
  receipt_hash: string;
}

// Resolve the log dir relative to this file. In dev: web/dev-portal/runtime/.
const LOG_DIR = path.resolve(import.meta.dirname ?? process.cwd(), "..", "..", "runtime");
const LOG_FILE = path.join(LOG_DIR, "access.log");

function ensureLogDir() {
  try {
    fs.mkdirSync(LOG_DIR, { recursive: true });
  } catch {
    // Non-fatal: if we can't write, audit becomes a no-op. The receipt's
    // absence is itself information — the deploy environment will eventually
    // surface this. See scope-doctrine.md on "auditability of access decisions
    // not being optional, just rotated through the live storage path."
  }
}

/**
 * Record an access decision. Returns the persisted receipt (or `null` if the
 * write failed — caller should not block on persistence).
 */
export function record(input: {
  actor: Identity;
  scope: Scope;
  resource: string;
  decision: PolicyDecision;
}): AccessReceipt | null {
  const now = new Date().toISOString();
  const base = {
    schema: "access-receipt/v0" as const,
    actor: input.actor,
    scope: input.scope,
    resource: input.resource,
    decision: input.decision.decision,
    recorded_at: now,
    ...(input.decision.decision === "allow"
      ? { authority: input.decision.authority }
      : { reason: input.decision.reason }),
  };

  // Content-address over a canonical-key JSON serialization. Stable across
  // runs because we use a fixed key order.
  const canonical = JSON.stringify(base, sortedKeys(base));
  const hash = crypto.createHash("sha256").update(canonical).digest("hex");

  const receipt: AccessReceipt = { ...base, receipt_hash: hash };

  ensureLogDir();
  try {
    fs.appendFileSync(LOG_FILE, JSON.stringify(receipt) + "\n", { encoding: "utf8" });
  } catch {
    return null;
  }

  return receipt;
}

/**
 * Read the last `n` receipts for a given actor + resource (or all if no
 * filters). Used by the AuditShelf component to render a slice of an actor's
 * own audit trail at the bottom of a gated page.
 */
export function tail(options: {
  actor?: Identity;
  resource?: string;
  limit?: number;
}): AccessReceipt[] {
  const limit = options.limit ?? 5;
  ensureLogDir();
  let lines: string[] = [];
  try {
    const buf = fs.readFileSync(LOG_FILE, { encoding: "utf8" });
    lines = buf.split("\n").filter(Boolean);
  } catch {
    return [];
  }

  const records: AccessReceipt[] = [];
  for (let i = lines.length - 1; i >= 0 && records.length < limit; i--) {
    try {
      const line = lines[i];
      if (!line) continue;
      const r: AccessReceipt = JSON.parse(line);
      if (options.resource && r.resource !== options.resource) continue;
      if (options.actor) {
        if (options.actor.kind !== r.actor.kind) continue;
        if (options.actor.kind === "github" && r.actor.kind === "github") {
          if (options.actor.login !== r.actor.login) continue;
        }
      }
      records.push(r);
    } catch {
      // skip malformed line
    }
  }

  return records;
}

/** Stable-order JSON.stringify replacer — guarantees the receipt hash is reproducible. */
function sortedKeys(obj: object) {
  const keys: string[] = [];
  walk(obj, "", keys);
  keys.sort();
  return keys;
}

function walk(obj: unknown, prefix: string, acc: string[]) {
  if (obj === null || typeof obj !== "object") return;
  for (const k of Object.keys(obj as Record<string, unknown>)) {
    const path = prefix ? `${prefix}.${k}` : k;
    acc.push(k);
    walk((obj as Record<string, unknown>)[k], path, acc);
  }
}
