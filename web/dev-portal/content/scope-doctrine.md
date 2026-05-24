# Scope Doctrine — icn.zone

## The principle

"Public by default" in ICN does not mean *everything is readable by anyone*. It means **every action and decision leaves an auditable trace that affected parties can legitimately access through process they themselves consent to**.

Privacy of content coexists with public legitimacy of process. Private data stays private. The fact that a decision was made about that data — by whom, under what authority, with what proof — is auditable through legitimate channels by the people the decision affects.

icn.zone is the first tool that has to be true about itself.

## What scope means

A page on icn.zone is not "public" or "private." It declares an **audience scope** — the set of people who have standing to see it, the channel through which their standing is verified, and the trace left when they access it.

### Scopes today

| Scope | Standing test | What lives here |
|---|---|---|
| `public` | None — no sign-in required | The technical landing surface. Dashboard, roadmap, repos, architecture, spec ladder, feed, wiki, glossary, contributor on-ramp. The nerd-surface content. Lightweight aggregate counters may be recorded; no per-actor receipt. |
| `staff` | Authenticated; member of the `InterCooperative-Network` GitHub org | The inside dashboard, notices, standups, contributor directory, audit-log (own data), inside feed. Per-actor receipts recorded. |
| `contributor` | Staff + has acknowledged the contributor charter | More detailed dev internals, RFC drafts in flight, agent handoffs that depend on context. |
| `cooperative-member` | Has standing as a member of a specific cooperative (today: NYCN) | Partner-touching pages: NYCN data summaries, partner-bound decisions, member-facing dashboards. |
| `steward` | Has the steward role for a specific operational surface | Deploy: K3s admin, deployed image SHA, raw smoke-test logs. Security: unredacted findings before disclosure. |

A scope is **additive over standing, not exclusionary by class**. Everyone with `steward` standing has `contributor` standing has `staff` standing. Standing is held in identity, not assigned per request.

### Scope ≠ secrecy

A scope says "these are the people who have standing to see this through the normal channel." It does not say "no one else can ever see this." A receipt of an access decision is itself a public artifact (after appropriate redaction) — that's how the system stays auditable. A `steward`-only page has fewer everyday readers, but the *fact* that someone read it, *when*, and *under what authority*, is preserved as a public receipt — visible to anyone who can ask the system who has been reading it (subject to the redaction policy for the receipt class itself).

## How access is decided

Every request that touches a scoped page is mediated by the **scope policy oracle** at [`src/lib/scope-policy.ts`](../src/lib/scope-policy.ts).

The oracle takes three things and returns one:

```
   ScopeRequest = { actor: Identity, scope: Scope, resource: Path }
        ↓
   ScopePolicyOracle.evaluate(request)
        ↓
   PolicyDecision = Allow { constraints } | Deny { reason }
```

This intentionally mirrors the kernel/app `PolicyOracle` pattern in [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../../docs/architecture/KERNEL_APP_SEPARATION.md). The portal is an app; the access rules it enforces are ICN policy.

### Today (interim)

- `actor` is a GitHub identity (login + org membership) carried in a signed session cookie.
- `staff` standing = "is in `InterCooperative-Network` org, public or private membership."
- `contributor`, `cooperative-member`, `steward` are flags in a hand-maintained allowlist in `src/data/standing.ts`.
- The oracle is a pure-TypeScript function; decisions are made in-process.
- Receipts are recorded to a local log file (or memory in dev).

### Target (when substrate is ready)

- `actor` is a DID (`did:icn:...`) verified through ICN identity.
- `staff` standing is a capability token issued under the contributor charter.
- `contributor`, `cooperative-member`, `steward` standings are governance-mediated: a CCL contract names the criteria; the trust graph names the holders; the standing flows from there.
- The oracle delegates to ICN's actual `PolicyOracle` registry.
- Receipts are real ADR-0026 receipts written to the ledger via the gateway. Anyone with audit standing can retrieve them; the receipt's privacy class controls what's visible to whom.

The migration is mechanical because the *shape* of the decision is the same. The signature of `evaluate(request) → decision` does not change. The mock oracle has comments marking exactly where each ICN primitive plugs in.

## What gets recorded

Every access decision — `Allow` or `Deny` — writes an audit entry through [`src/lib/audit.ts`](../src/lib/audit.ts):

```
   AccessReceipt {
     actor: DID | GitHubLogin           // who asked
     scope: Scope                       // what scope they claimed
     resource: Path                     // what they tried to access
     decision: "allow" | "deny"
     reason: string                     // why (oracle's explanation)
     authority: AuthorityHandle         // what authority basis (charter clause, role grant, etc.)
     timestamp: ISO8601
     receipt_hash: string               // content-addressed
   }
```

In the interim implementation, receipts are written to `runtime/access.log` (or in-memory in dev). Each receipt is a JSON-Lines record.

Eventually receipts become **ADR-0026 receipts** through the opaque cascade landed in May 2026 (#1755 / #1757 / #1758 / #1759), routed to the gateway's opaque receipt storage. The class string will be `"access_receipt"`. The `key1` will be the actor identity. The `key2` will be the resource path.

The audit shelf on each gated page renders a slice of these receipts — "your last 5 visits to this page, with the authority basis each time" — making the audit trail visible to the person whose access is being recorded.

## What this is *not*

- **Not a paywall.** Auth gates content for legitimate-channel access, not against payment.
- **Not security through obscurity.** Receipts make access visible *more* than a public page would. The point isn't to hide; it's to mediate.
- **Not a substitute for content redaction.** If something genuinely should not be in the system at all (raw member-private bytes), it doesn't go in the system. Scope is about who legitimately sees what's there; it's not a fig leaf for stuff that doesn't belong here at all.
- **Not negotiable per access.** The policy oracle decides; access is not a back-and-forth negotiation. If the oracle denies, the path to acquire standing is through governance, not appeal.
- **Not implemented in production form.** Today's implementation is interim — GitHub-allowlist mock with file-backed audit. The shape is real; the substrate isn't yet.

## Why this matters

ICN's broader argument is that internal infrastructure can be operated in ways that respect both privacy and legitimacy — that auditability isn't a tradeoff against confidentiality, and that legitimate access can be mediated by process rather than ad-hoc admin discretion.

icn.zone is the first place where ICN's team is itself organized that way. It's a worked example, not a marketing claim. When a cooperative deploys an institutional tool downstream, the question "how do we run our internal docs / dashboards / coordination?" has an answer that doesn't reduce to "stick it on Notion behind SSO." This is the prototype of the answer.
