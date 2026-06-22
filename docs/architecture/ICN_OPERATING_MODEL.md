---
Status: descriptive
Authority: architecture doctrine — normative for vocabulary, placement, and operating-model grammar; NOT normative for current implementation status
Canonical: no
Current-state source: docs/STATE.md + docs/PHASE_PROGRESS.md
Last Reviewed: 2026-06-22
---

# ICN Operating Model

> **Status: doctrine.** This document is the *grammar map* of ICN. It is **canonical for vocabulary,
> placement, and operating-model doctrine** — it fixes what each layer means, what belongs where, who
> authorizes what, and what receipt proves it. It is **not** a current-state truth root and must not be
> read as one. **This document explains what each layer means. It does not assert that every layer is
> implemented.** For current implementation status, read [`../STATE.md`](../STATE.md) and
> [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) — those remain the only canonical state roots.

## Purpose

ICN's architecture is real and coherent, but its explanation is spread across many architecture and spec
documents. New work — and new agents — keep re-deriving the same questions and occasionally drift into
stale or false claims. This document gives one durable place to answer four questions about any feature,
type, file, route, or service:

1. **What kind of thing is this?**
2. **Where does it belong?**
3. **Who authorizes it?**
4. **What receipt proves it?**

It standardizes vocabulary, fixes a placement grammar, and preserves the meaning firewall. It **cross-links
the canonical definitions rather than redefining them** — where a concept already has a home doc, this
document points there and stays the thin unifying layer above it. It is intentionally generic: it pins no
institution-specific vocabulary inside ICN core.

It is narrower in scope than the integrating narrative in
[`ICN_INTEGRATED_SYSTEM_MODEL.md`](ICN_INTEGRATED_SYSTEM_MODEL.md) and the living repo atlas (#1689): those
explain how the pieces fit and where they live in the tree; this document fixes the *words and placement
rules* used to talk about them.

## What ICN is (and is not)

ICN is a **governed institutional substrate** — a cooperative operating substrate where democratic
institutions define authority, bind tools/services/policies to that authority, execute bounded actions,
prove what happened with receipts, and remain able to export and exit without platform capture.

ICN is **not** an app, a blockchain, a payment system, a SaaS tenant platform, or a federation server. It
hard-codes the institutional *grammar*, not the institution. Institutions compose that grammar through
packages, CCL, templates, tools, services, and surfaces.

The guiding thesis: *make it possible for a cooperative, community, or federation to govern itself through
infrastructure it can understand, inspect, export, repair, and leave — without becoming a tenant inside
someone else's platform.*

## The whole-system stack

```
Infrastructure
  → Node
    → Runtime
      → Engines
        → Domain
          → Package
            → Policy / CCL
              → Binding
                → Tool / Service / Workload
                  → Surface
                    → Receipt / Proof / Sync / Federation
```

In plain language:

```
Nodes run the substrate.
Domains hold authority.
Packages provide local meaning.
Policies define adopted rules.
CCL makes adopted rules executable.
Bindings authorize tools, services, policies, routes, workloads, and package activations.
Runtime engines execute bounded transitions.
Surfaces make participation accessible.
Receipts make truth auditable.
Federation connects domains without swallowing them.
```

## The central civic loop

Every object in the repository should locate somewhere on this loop. If you cannot place a thing on it,
that is a signal the thing is mis-scoped.

```
identity
  → standing
  → authority
  → action card
  → authorized action
  → CCL / runtime evaluation
  → storage / compute / governance / economic transition
  → receipt
  → sync / federation / replication
  → member shell + steward cockpit
  → challenge / repair / next action
```

## Standardized vocabulary

These are the words ICN uses. Each term has a canonical home; this table fixes the meaning and points to it.
**Use these words; do not coin synonyms.** "App" is deliberately retired in favor of the precise terms below.

| Term | Meaning | Canonical home |
|------|---------|----------------|
| **Substrate / Kernel** | Meaning-blind mechanism and constraint enforcement. Identity, network, gossip, store, encoding, runtime, dispatch. | [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) |
| **Engine** | Generic institutional runtime logic inside ICN — governance, ledger, federation, process, policy, identity. Translates meaning into generic effects. | [`ICN_INTEGRATED_SYSTEM_MODEL.md`](ICN_INTEGRATED_SYSTEM_MODEL.md) |
| **Domain** | The governed jurisdiction that holds authority. Not a DNS name, node, repo, tool, member account, bucket, or CCL document. | [`../spec/institutional-domain.md`](../spec/institutional-domain.md) |
| **Policy** | Adopted rule state, often expressed through CCL. Inert until a domain adopts it. | [`../spec/ccl-policy-registry.md`](../spec/ccl-policy-registry.md) |
| **CCL** | Deterministic, fuel-metered, versioned executable policy language. Inert until adopted; does not adopt itself; does not mutate state directly. | [`../spec/ccl-policy-registry.md`](../spec/ccl-policy-registry.md) |
| **Tool** | Replaceable/forkable functionality operating on institution-owned state. Never owns institutional truth. | [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) |
| **Service** | Running operational dependency with identity, custody, backup, exit, and receipts. Reachable ≠ native. | [`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md) |
| **Surface** | Human-facing rendering: member shell, steward cockpit, public site, organizer shell. Renders receipts/evidence; is never the source of authority. | [`../spec/member-shell-v0.md`](../spec/member-shell-v0.md), [`../spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) |
| **Package** | Institution-specific meaning, templates, seeds, workflows (e.g. NYCN). Owns its own roadmap; bound to ICN by agreement. | [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md) |
| **Binding** | A domain-authorized relationship between a domain and a policy/tool/service/route/workload/package. | [`../spec/governed-service-binding.md`](../spec/governed-service-binding.md) |
| **Manifest** | Declared shape of a tool/service/workload: capabilities, data touched, storage, privacy, receipts emitted. | [`../spec/governed-service-binding.md`](../spec/governed-service-binding.md), [`RFC-0017`](../rfcs/RFC-0017-tool-install-infrastructure.md) |
| **Provider** | Substrate-side counterpart that knows how to run a manifest under a binding's constraints. | [`DEBIAN_APPLIANCE_MODEL.md`](DEBIAN_APPLIANCE_MODEL.md) |
| **Instance** | A live concrete object created from a definition/template. | [`ICN_INTEGRATED_SYSTEM_MODEL.md`](ICN_INTEGRATED_SYSTEM_MODEL.md) |
| **Receipt** | Durable proof that a transition occurred, under the receipt-and-provenance envelope. | [`../adr/ADR-0026-receipt-and-provenance-proof-envelope.md`](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) |
| **Bridge** | Connection to external / non-ICN systems. Translates; emits import/translation receipts. | [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) |
| **Artifact** | Content-addressed institutional memory. | [`../spec/artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) |
| **Vault** | Governed private custody for restricted objects; disclosure requires authority and leaves a receipt. | [`../spec/artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) |
| **Agreement** | A cross-domain commitment / federation relation. Coordinates domains without subsuming them. | [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) |
| **Node** | Hardware/instance that runs the substrate. Roles are capacity, not authority. | [`DEBIAN_APPLIANCE_MODEL.md`](DEBIAN_APPLIANCE_MODEL.md) |

The one-line rule that governs them all:

```
Substrate enforces.
Engines transition.
Policies decide conditions.
CCL evaluates adopted rules.
Tools perform functions.
Services run.
Surfaces render.
Packages localize.
Bindings authorize.
Instances live.
Bridges translate.
Receipts prove.
```

## The definition / binding / instance / receipt pattern

This four-part shape recurs across tools, services, policies, processes, packages, agreements, routes,
compute workloads, and artifacts. When you add anything to that list, you should be able to name all four:

```
Definition = reusable declared shape.
Binding    = this domain authorized it.
Instance   = concrete live object.
Receipt    = proof a transition happened.
```

If a thing has a definition but no binding, it is not authorized. If it runs without producing a receipt
for an institutional state change, it is not auditable. Both are smells.

## Layer model

ICN is a stack of responsibility tiers. Meaning flows *down* only as constraints; authority flows *up*
only through governance. (Full treatment: [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md),
[`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md).)

```
Teaching layer (icn-learn) — explains the system; does not define it.
Institution package layer — first-party vocabulary, workflows, partner policy, fixtures.
App / tool / service layer — engines and tools acting on institution-owned state under bindings.
Substrate / kernel layer — identity, network, gossip, store, encoding, runtime, dispatch.
Hardware / infrastructure — nodes, networks, operators, custodians.
```

### The meaning firewall (load-bearing; CI-enforced)

The most important architectural principle:

- **Packages and tools** may understand institutional meaning.
- **Runtime engines** translate meaning into generic effects.
- **The kernel/substrate** enforces primitive constraints without understanding domain meaning.
- **Kernel crates must not import domain/package semantics.** Package-specific nouns must not leak into ICN core.
- **ICN core owns generic institutional grammar; packages own local vocabulary.**

ICN core may know: `Proposal`, `Vote`, `ProcessSession`, `ActionCard`, `Meeting`, `ActionItem`, `Program`,
`Milestone`, `RoleAssignment`, `Standing`, `Mandate`, `AuthorityGrant`, `Obligation`, `Allocation`,
`Settlement`, `Artifact`, `Vault`, `Receipt`, `Agreement`, `BoundaryOutcomeReceipt`.

A **package** (e.g. NYCN) may know: summit, sponsor packet, sponsor pipeline, speaker intake, venue
walkthrough, accessibility intake, organizer role labels, summit stage gates, package-specific
dashboards/playbooks/templates.

If you are about to put `AnnualSummit`, `SponsorPacket`, `VenueLocked`, `PlatinumSponsor`, or any
institution-ceremony word into generic ICN runtime — **stop.** That belongs in the institution package.
These names appear in this document only as examples of what *does not* belong in core. The firewall is
enforced mechanically (`.github/scripts/firewall_denylist.py`); this doctrine is the human-readable form.

## Placement table

Where a feature belongs, by what it is:

| If it is… | It belongs in… | Authorized by | Proven by |
|-----------|----------------|---------------|-----------|
| A generic institutional noun (proposal, milestone, meeting, receipt, agreement) | **ICN core** (governance/kernel-api engines) | charter/role/standing in the domain | the matching receipt class |
| Generic runtime logic that transitions state | **an engine** (governance, ledger, federation, process, policy) | a mandate derived from a decision | effect dispatch evidence + receipt |
| An adopted rule / decision condition | **policy / CCL** (a rule version adopted into a DomainPolicy) | a governance adoption act; inert until adopted | policy-version binding + deterministic effect-plan preview |
| Institution-specific meaning, templates, seeds, workflows | **an institution package** | the package's binding to a domain | activation/process receipts exported as evidence |
| Replaceable functionality over institution-owned state | **a tool** (Tool Commons) | a ToolBinding under domain authority | tool-action receipts; clean export path |
| A running operational dependency (forge, auth, status) | **a service** | a GovernedServiceBinding | service lifecycle/backup/exit receipts |
| A connection to an external / non-ICN system | **a bridge** | an explicit policy granting the external connection | import/translation receipts |
| A cross-domain commitment / federation relation | **the federation / agreement layer** (never a super-domain) | a counter-signed agreement between domains | boundary-outcome receipts; dispute/exit window |
| Human-facing rendering of standing/actions/receipts | **a surface** (member shell, steward cockpit) | nothing — surfaces never hold authority | it renders receipts/evidence honestly |
| Constraint enforcement with no semantic meaning | **the kernel/substrate** | pure data (ConstraintSet, capability) | enforced bound; opaque receipt storage |

## Transition grammar

The verbs an institution applies to the nouns above. A thing moves through these states; each
institution-relevant move should be able to point at a receipt.

```
declare → review → adopt → bind → run → observe → amend → suspend → repair → export → exit → archive
```

Notes that are easy to get wrong:

- **Adopt** is a governance act; an unadopted policy is inert.
- **Bind** is authorization, separate from *run* (capacity). A node profiled as `service-host` gains
  capacity, not authority.
- **Repair** is not deletion. Reversal/challenge appends new receipts/counter-effects; it never erases.
- **Export / exit** are first-class and governed — a thing that cannot answer "how does the institution
  leave cleanly?" should not be installable.

## Feature-placement checklist

Before adding a type, route, tool, service, or surface, answer all six. If any answer is unclear, the
feature is not ready to land.

1. **What kind of thing is this?** (engine logic / core noun / package meaning / tool / service / surface / kernel constraint)
2. **Where does it belong?** (core vs engine vs package vs tool vs service vs surface — use the placement table)
3. **Who authorizes it?** (which domain, which policy, which binding, which mandate)
4. **What receipt proves it?** (which receipt class records the transition; if none, why is that safe?)
5. **Which surface shows it?** (member shell, steward cockpit, public site — or intentionally none)
6. **What custody / privacy / exit policy applies?** (storage class, locality, disclosure, backup, export path)

## Known vocabulary debt

The codebase uses `CoopReplicated` / `Coop(coop_id)` for what is structurally a *generic local-domain*
scope. A community-owned or federation-owned domain should use the same structural vocabulary without
conflation. This is named as compatibility-aware future work in
[`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md) §C3 and is **not** renamed here. The
correct generic vocabulary is "local-domain-scoped" / `LocalDomain(domain_id)`; the rename is deferred.

## Development direction (how to move)

Move by **vertical slices**, not horizontal mega-builds. A good slice cuts thinly through every layer and
ends in a receipt plus an evidence export. A bad slice is "implement all of X."

The immediate vertical spine the project is making runtime-real:

```
package → domain → policy → binding → process/action → receipt → surface → evidence/export
```

The near-term objective is to activate **one** institution package into **one** governed domain, bind
**one–two** policies/tools/services, walk **one** human-facing process, emit receipts, surface them in
member/steward views, and export repo-safe evidence — without claiming production, pilot, or
live-federation readiness. NYCN is the intended first reference package/rehearsal target; its nouns stay
in the package, never in ICN core. The issue-side mirror of this spine is the vertical-spine control issue
(see Related work).

### Parked zero-coverage gaps (named so restraint is visible)

These layer concepts are spec-level with no dedicated runtime-implementation issue yet. They are
intentionally **not** filed as a batch; each depends on the operating model and/or the `DomainPolicy`
spine becoming real enough first:

- CCL evaluator-selection runtime (the `(domain, proposal_kind, policy snapshot) → exactly one evaluator`
  selector; fails closed on zero or multiple).
- `GovernedServiceBinding` runtime (the abstract binding above the existing `ToolBinding` / `ComputeTask`
  / `Executor` projections).
- `BoundaryOutcomeReceipt` / `AgreementRegistry` federation runtime.

## Claims this doctrine does not make

This document is doctrine for vocabulary and placement. It makes **no** current-state claim. In particular,
naming a layer, primitive, or receipt class here is **not** evidence it is implemented. Do not read into
this document — or write into any doc that cites it — any of: Phase 2 completion · production readiness ·
live federation · formal NYCN pilot or activation · entity-aware authorization *enforced* · trusted positive
auth issuance in production · OpenAPI/API completeness · member-shell or steward-cockpit organizer/pilot
readiness (the human assistive-technology pass, #2041, is owed) · service hosting or Tool Commons as
runtime-real · CCL policy registry as implemented · live anti-entropy emission/repair (wire-stable schema
is not live behavior).
Current implementation status lives only in [`../STATE.md`](../STATE.md) and
[`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md).

Use safe, regulatory-aligned vocabulary for ICN-native participation — obligation, allocation, settlement,
unit, position, receipt, provenance, evidence, identity, standing, action card, domain, policy, binding,
custody, exit. Never wallet, token, balance, payment, currency, or blockchain framing.

## Related work

- Integrating narrative: [`ICN_INTEGRATED_SYSTEM_MODEL.md`](ICN_INTEGRATED_SYSTEM_MODEL.md)
- Boundaries: [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) · [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md)
- Domain infrastructure: [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md)
- Tools & services: [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) · [`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md)
- Nodes/appliance: [`DEBIAN_APPLIANCE_MODEL.md`](DEBIAN_APPLIANCE_MODEL.md)
- Spine specs: [`../spec/institutional-domain.md`](../spec/institutional-domain.md) · [`../spec/effect-dispatch-contract.md`](../spec/effect-dispatch-contract.md) · [`../spec/ccl-policy-registry.md`](../spec/ccl-policy-registry.md) · [`../spec/governed-service-binding.md`](../spec/governed-service-binding.md) · [`../spec/artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) · [`../spec/storage-durability-policies.md`](../spec/storage-durability-policies.md) · [`../state/storage-governance-spec.md`](../state/storage-governance-spec.md) · [`../spec/compute-placement-policy.md`](../spec/compute-placement-policy.md) · [`../spec/network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md) · [`../spec/member-shell-v0.md`](../spec/member-shell-v0.md) · [`../spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md)
- Foundational ADRs: [`../adr/ADR-0014-constitutional-object-model.md`](../adr/ADR-0014-constitutional-object-model.md) · [`../adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md`](../adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md) · [`../adr/ADR-0025-institutional-effect-record-canonical-schema.md`](../adr/ADR-0025-institutional-effect-record-canonical-schema.md) · [`../adr/ADR-0026-receipt-and-provenance-proof-envelope.md`](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) · [`../adr/ADR-0027-action-card-contract.md`](../adr/ADR-0027-action-card-contract.md)
- Current state (canonical): [`../STATE.md`](../STATE.md) · [`../PHASE_PROGRESS.md`](../PHASE_PROGRESS.md)
