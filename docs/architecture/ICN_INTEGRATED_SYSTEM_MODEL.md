---
Status: descriptive
Authority: architecture (forward-direction; not yet normative)
Canonical: no
Last Reviewed: 2026-05-14
---

# ICN Integrated System Model

> **Status: design direction.** This document is the integrating spine across ICN's substrate, governance, contract layer, service hosting, workloads, storage, compute, networking, member shell, and steward cockpit. It explains how the existing pieces fit together. It is **not** an implementation report. Where the text names objects, primitives, or lifecycle stages that do not yet exist in the repository, those are explicitly marked as future or planned.

## Purpose

ICN already has the right pieces. What is missing is one stable explanatory thread that answers a single question: how do the pieces fit together?

This document supplies that thread. It is narrower than the living repo atlas (#1689) and narrower than `docs/ARCHITECTURE.md`. It exists to:

- name the integrated operating model of a cooperative institutional domain;
- locate every subsystem inside one civic loop;
- preserve the meaning firewall while still describing end-to-end ceremony;
- separate substrate from app, app from institution package, and institution package from teaching surface;
- mark forward-direction objects as forward-direction, and current truth as current truth;
- give the next round of specifications (#1794, #1797, and their children) a shared frame to extend.

It is intentionally generic. It does not pin any institution-specific vocabulary inside ICN core.

## What this document is not

- Not an implementation report. No code or schema lands here.
- Not a Phase 2 completion claim.
- Not a service hosting implementation claim.
- Not a live federation claim.
- Not a production readiness claim.
- Not a formal pilot claim for any partner institution.
- Not a mutation of K3s, DNS, identity-bridge, or any deployed service.
- Not a redefinition of the meaning firewall, the policy-oracle pattern, or the kernel/app boundary.
- Not the design home for any specific institution package. Institution package nouns live in their own repositories.

The vocabulary discipline reminders below (see the "Vocabulary discipline" section) are not introduced into the model — they live there as anti-claims.

## Repository boundaries

ICN is the generic system. It does not own any institution's local vocabulary.

| Repo | What lives there |
|---|---|
| `icn` (this repo) | Generic primitives, runtime, kernel/app boundary, ADRs, RFCs, the public website source, and forward-direction architecture documents such as this one. No institution-specific nouns. |
| Institution-package repos | First-party application of these designs by a specific federation or cooperative: their package vocabulary, their workspace bindings, their partner policies, their operating playbooks. The first reference implementation is the NY cooperative network package, kept in its own repo. |
| `icn-learn` | Teaching surface only. Links to canonical truth here and to operating truth in institution packages. Does not define ICN. |

This boundary is enforced by [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md). Anywhere this document references a partner federation, it is as a source-of-truth boundary, not as a primitive.

## The civic loop

Every subsystem participates in the same loop.

```text
Identity
  → Standing
  → Authority
  → Action Card
  → Authorized Action
  → CCL / runtime evaluation
  → Storage / Compute / Governance / Economic transition
  → Receipt
  → Sync / Federation / Replication
  → Member shell + Steward cockpit update
  → Challenge / repair / next action
```

Every later section in this document explains how its subsystem enters that loop and what it contributes.

The loop is not a workflow definition. It is the **shape** any legitimate transition must take. A transition that skips authority, skips evaluation, skips receipt, or hides from member/steward surfaces is not a legitimate institutional transition. It may still be an operational event, but it does not become institutional truth.

## Substrate, app, package, teaching

ICN is a layered system. Each layer has a separate responsibility and a separate license for what it knows.

```text
┌───────────────────────────────────────────────────────────────┐
│  Teaching layer (icn-learn)                                   │
│  Explains the system. Does not define it.                     │
├───────────────────────────────────────────────────────────────┤
│  Institution package layer                                    │
│  First-party vocabulary, workflows, partner policy, fixtures. │
│  Owns its own roadmap. Bound to ICN through agreements.       │
├───────────────────────────────────────────────────────────────┤
│  App / tool layer                                             │
│  Policy oracles, governance app, ledger app, membership app,  │
│  charter app, future installable tools, scoped vaults.        │
├───────────────────────────────────────────────────────────────┤
│  Substrate / kernel layer                                     │
│  Identity, network, gossip, store, encoding, runtime, dispatch│
│  Enforces constraints; does not understand meaning.           │
├───────────────────────────────────────────────────────────────┤
│  Hardware / infrastructure                                    │
│  Nodes, networks, operators, custodians.                      │
└───────────────────────────────────────────────────────────────┘
```

Two rules across all layers:

- **Meaning flows down only through constraints.** Apps and packages express intent. The substrate enforces bounded effects without semantic context. See [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md).
- **Authority flows up only through governance.** Lower layers can carry data and produce evidence. They cannot adopt rules. Rules are adopted by governance and made executable by the contract layer.

## Authority — InstitutionalDomain and DomainPolicy

The top of the model is **institutional authority**. Authority is jurisdictional: a domain decides what is legitimate inside its boundary.

Forward-direction primitives (not yet in the repository; tracked under #1794):

- `InstitutionalDomain` — the addressable scope of a federation, cooperative, community, or other institution. Owns its own members, charters, agreements, services, storage, and routing decisions.
- `DomainPolicy` — the live set of rules adopted by a domain. Bound to versioned charter text. The first input to every authority decision.
- `DomainSession` — bounded delegation of authority to a member, device, or service.
- `ServiceIdentity` — long-lived service-side identity issued by a domain, distinct from any operator's personal identity.
- `DeviceEnrollment` — record of which hardware speaks for which member or service.

Current truth: the kernel and app crates already model identity, charter primitives, and capability tokens. These primitives are the forward extension of that surface. They sit in front of the existing identity and trust layers; they do not replace them.

A domain is not a tenant. A tenant is a billing entity inside someone else's product. A domain is a self-governing jurisdiction with its own charter, its own membership, its own evidence trail, and its own right to leave.

## CCL — the executable institutional rule layer

CCL (Cooperative Contract Language) lives inside the governance layer. It is the bridge between adopted text and executable evaluation.

### What CCL is

- A language for expressing adopted charters, policies, and rules in a form that can be executed and audited.
- A way to evaluate whether a proposed transition satisfies the domain policy that governs it.
- A way to produce structured effect plans that the runtime can dispatch as bounded effects.
- A governed, versioned artifact. Every CCL document has authorship, adoption history, and a clear amendment path.

### What CCL is not

- Not the source of authority. CCL does not adopt itself; governance adopts it.
- Not a bypass around governance. A CCL rule that has not been adopted by the relevant domain is inert.
- Not a license for code to mutate arbitrary services. CCL produces effect plans; the runtime dispatches them; the kernel enforces bounds; the receipt closes the loop.
- Not a path for model or agent output to become authoritative. A model may help draft a CCL rule. Governance adopts it.
- Not a place for institution-specific nouns. Generic shapes belong here; package vocabulary stays in packages.

### The core rule

> CCL makes governance executable. It does not make code sovereign.

The CCL layer is reviewable. It produces effect plans, not unilateral mutations. The receipt is what proves anything happened.

A separate forthcoming specification (drafted in this PR's handoff) will define the CCL policy registry, versioning conventions, and the governance-effect hook contract. That document will name how policies are registered, how versions are bound to domain decisions, and how the runtime selects an evaluator for a given proposal kind.

## Governance decision and effect dispatch

Governance produces decisions. Decisions are not effects. An effect is what a decision authorizes.

The contract (tracked under #1797) is roughly:

1. A proposal enters the domain's process substrate with a defined kind.
2. Domain policy and the relevant CCL evaluator determine eligibility, threshold, and the shape of the resulting decision.
3. The decision is recorded with provenance: who decided, under what authority, against which policy version, producing what mandate.
4. The mandate is dispatched as one or more bounded effects. Each effect specifies its target subsystem (storage, compute, governance, economic, service binding), its arguments, and its required receipt class.
5. The subsystem performs the effect under kernel-enforced constraints. It does not interpret the underlying meaning of the decision.
6. Each effect produces a receipt. Receipts carry provenance back to the decision, the policy version, and the actor that authorized the action.
7. Receipts feed sync, replication, and federation. They surface in the member shell and the steward cockpit. They are the durable record.

An operational event that did not come from a decision does not become institutional truth simply because it happened. It is evidence at best.

A decision with no dispatched effect and no receipt is not institutional truth either. Decisions without execution are theatre.

This separation is what allows the kernel to enforce blindly. The kernel sees a constraint set and a target; it does not see "trust class" or "membership tier" or "approved at threshold." That semantic context lives in the policy oracle that produced the constraint.

## Governed workloads and service envelopes

The repository already has a service hosting model ([`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md)) and a tool commons model ([`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md)). What is still missing is the integrating concept that ties hosted services, installable tools, compute jobs, contract execution, and future container or microVM workloads under one envelope.

The forward-direction concept names (all marked as future; drafted as a specification in this PR's handoff):

- `GovernedServiceBinding` — the institutional record that binds a service or workload to a domain. Holds authority context, custody class, runtime class, dispatch policy, capability scope, backup policy, and exit policy.
- `WorkloadManifest` — the declared shape of what a workload is supposed to do. Inputs, outputs, runtime class, custody class, expected receipts, side-effect bounds.
- `RuntimeProvider` — the substrate-side counterpart that knows how to run a manifest under the binding's constraints.

### Runtime classes

Runtime is not one thing. Different workloads need different envelopes. The forward model expects to support several classes side by side:

- Deterministic legitimacy compute — for evaluating CCL and for any computation that records authoritative transitions. Must be deterministic and reproducible. See [`docs/design/deterministic-core.md`](../design/deterministic-core.md).
- Utility computation — for plugins, transforms, summarization, and other helpful work. May be non-deterministic. Produces evidence, not authoritative state.
- Container workloads — for normal service hosting where existing software stacks need to run unchanged.
- Stronger-isolation workloads — for arbitrary user code, untrusted code paths, or workloads that need a tighter blast radius than a container provides. The eventual envelope here is microVM-class isolation.
- Hardware-accelerated workloads — for model inference, deliberation tooling, or other compute that benefits from accelerators. Governed by explicit allocation, not opportunistic capture.
- Local device workloads — for code that runs on a member's own device with no central capture, including offline-first member shells.
- External bridge workloads — for legitimate integration with external systems by explicit policy. Governed; never default.

The core rule across these classes:

> Utility services can propose or produce evidence. Legitimacy compute verifies and records authoritative transitions.

A model agent can draft a proposal. A model agent cannot adopt a charter. A summarizer can produce a digest. A summarizer cannot emit a settlement receipt.

### Compute placement

Compute placement (tracked under #1801) is the policy that decides which pool runs which workload: local-only, cooperative, federation, or commons. Placement is itself a governed decision and produces its own receipts. Placement is also where data locality interacts with workload runtime; sensitive data must not be placed where the binding's custody class forbids it.

## Hosted service maturity stages

[`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md) names three stages: hosted, governed, native. This document extends that arc on both ends so the full maturity progression is visible:

```text
Unmodeled service → Hosted service → Governed service → Adapted service → ICN-native service
```

| Stage | What it means | What the service still needs |
|---|---|---|
| Unmodeled | The service runs somewhere; ICN does not know about it. | Discovery, identification, custody classification. |
| Hosted | The service runs on infrastructure operated by an ICN-aligned operator. Reachable but opaque. | Authority binding, event mapping, receipt emission, exit policy. |
| Governed | Changes to the service are bound to explicit authority. Operator actions leave provenance. | Custody class assignment, backup/restore policy, capability-scoped access. |
| Adapted | The service emits ICN receipts for institutionally relevant transitions. Member-facing surfaces use service identities, not operator identities. | Workload manifest binding, named runtime provider, observable steward surface. |
| ICN-native | The service is governed end-to-end: declared by manifest, bound by `GovernedServiceBinding`, executed by a named `RuntimeProvider`, backed by a custody policy, observable by member shell and steward cockpit, exportable on exit. | Continued review; staged release process for new authority. |

A reachable service is not a native one. Reachability is an operational fact. Nativeness is an institutional fact.

The stage transitions are not automatic. A service moves from hosted to governed only when authority binding lands. A service moves from governed to native only when the full lifecycle commitments hold. The criteria are intentionally strict because each stage adds authority surface, and authority cannot be invisible.

## Storage as custody

Storage is not bytes. Storage is custody. Custody is authority. Authority requires governance.

The repository's storage governance specification ([`docs/state/storage-governance-spec.md`](../state/storage-governance-spec.md)) already names storage classes and data locality as constraints. This section names the wider taxonomy the system must eventually carry.

| Class | Purpose | Custody implications |
|---|---|---|
| Canonical store | Authoritative institutional state: charters, decisions, receipts, agreements. | Must be replicated, backed up, and recoverable. Losing it is losing institutional truth. |
| Service state | The internal state of a hosted or governed service. | Owned by the service binding; subject to backup and exit policy. |
| Artifact / blob store | Files attached to decisions, evidence, exports, releases. Tracked under #1798. | Content-addressable; bound to a domain; subject to retention and locality policy. |
| Volume / block store | Underlying storage for containerized or VM-class workloads. | Provisioned through allocation; subject to placement policy. |
| Scoped vault | Private state with restricted disclosure. Tracked under #1792 and #1767. | Disclosure requires explicit authority; access leaves a receipt. |
| Secret / key material | Operator and service keys, recovery material. | Strictest custody; rotation and recovery are governed events. |
| Cache / derived state | Member-shell offline cache, projections, summaries. | Reconstructible from authoritative state; never the source of truth. |

A storage binding without a custody class is not safe; the system should refuse it. A custody class without a backup, replication, and exit policy is not safe either; the binding is incomplete.

## Backup, redundancy, recovery, archive

This is the most under-specified surface in the current model. Backup work touches all of the storage classes above and is the prerequisite for every legitimate exit-on-conflict story. A separate specification is drafted in this PR's handoff.

The doctrine the model assumes:

- **Redundancy keeps the service alive.** Replication, failover, anti-entropy. It does not preserve history.
- **Backups keep the institution recoverable.** Point-in-time copies, integrity-verified, with bounded restore time.
- **Archives keep the institution accountable.** Long-horizon retention, often write-once, governed by retention policy.
- **Disaster recovery proves the promises are not decorative.** Drills produce receipts that show the institution can rebuild itself.

Forward-direction policy objects (all future; tracked in the drafted backup-policy specification):

- `StorageSpec` — what storage is bound to a workload; class, locality, capacity.
- `BackupPolicy` — frequency, target, integrity, retention.
- `ReplicationPolicy` — replicas, placement, anti-entropy expectations.
- `RecoveryPolicy` — restore objectives, drill cadence, authority required to restore.
- `ArchivePolicy` — long-term retention, immutability requirements.
- `IntegrityPolicy` — verification cadence, integrity-receipt class.

Rules the system should hold:

- Backup and restore operations produce receipts. Restore drills are real events with real receipts; "we have backups" without restore-test receipts is decoration.
- Backups inherit the data locality of the source. A backup that crosses a locality boundary violates the same policy the source enforces.
- Restoring authoritative state is itself a governed event. It is not a routine operator gesture.

This area also intersects with network anti-entropy proof loops (#1799). Replication is not backup, but they share a verification surface; both need observable convergence proofs.

## Networking and routing

The system's network layer must distinguish three things that are easy to conflate:

- **DNS routes people.** It maps human-memorable names to nodes that can serve them. It does not confer authority.
- **Nodes run infrastructure.** They are operationally responsible for what they host. They do not own institutional truth.
- **Domains hold authority.** Authority is institutional, not topological. A domain may be served by many nodes and routed by many names without losing itself.

The repository already names this distinction in [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md). The integrated model extends it: routing is a binding, not a definition. A domain that moves nodes does not change identity. A domain that loses its name on the public internet does not lose itself.

The forward routing/redundancy/anti-entropy proof loops (#1799) cover the substrate-side convergence stories that have to hold for federation to be real. The public-facing routing layer is a separate concern, with its own threat surface and its own exit story.

## Member shell and steward cockpit

Every transition this document describes must surface in two places: the member shell and the steward cockpit. If a member cannot see what was decided, what their standing is, and what action is open to them, the institution has not actually closed the loop. If a steward cannot see whether the substrate is healthy and whether services are honoring their bindings, operability is decoration.

### Member shell

The member shell is the primary participation surface. A forthcoming UX specification is drafted in this PR's handoff. The shell must:

- be mobile-first;
- present action cards derived from the member's standing;
- show receipts as the explanation of every visible state change;
- support offline and low-bandwidth modes;
- maintain a local cache that is treated as derived state, never as the source of truth;
- present safe signing confirmations before any authorized action;
- meet the accessibility commitments codified in [`ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md): large tap targets, plain language, screen-reader compatibility, color-independent affordances, captions where audio is present, keyboard accessibility, low cognitive load.

The member shell does not use the financial-product framing of consumer apps. It does not present participation as account management. The vocabulary discipline captured under "Vocabulary discipline" below applies here in full.

### Steward cockpit

The steward cockpit (tracked under #1795) is the operability surface for node operators, service stewards, and domain administrators. It must show service binding state, custody class compliance, backup and restore status, network convergence, and outstanding mandates. It is the surface that surfaces evidence; it does not adopt rules.

A steward sees what is failing. A member sees what is open to them. Authority sees what was decided. The same receipt feeds all three views.

### Challenge and repair

Every legitimate transition must be challengeable. Every challenge must produce a receipt of its own. Every repair must dispatch through the same effect-dispatch ceremony as the original decision. This is what makes the system reversible and accountable.

A system where actions can be challenged only through human escalation outside the substrate has not closed the loop. The model assumes the challenge path is a first-class flow, surfaced in both member shell and steward cockpit.

## Threat surface (cross-link summary)

The integrated model does not introduce a new threat-model document. Subsystem threat surfaces are already tracked across:

- #1010 — adversarial model and chaos harness for the substrate.
- #1767 — encrypted distributed private-overlay storage threat model.
- #1792 — private data disclosure boundary, scoped vaults, access receipts.
- #1799 — network routing, redundancy, and anti-entropy proof loops.
- #1801 — compute placement policy across local, cooperative, federation, and commons pools.

A consolidated runtime/service/storage threat model is a candidate for future work; for now the recommendation is to cross-link these existing surfaces rather than file a redundant umbrella issue. The PR's handoff captures the rationale.

## Vocabulary discipline

ICN's regulatory and design discipline rules out a small set of words from positive ICN-native framing because they invite the wrong mental model. The enforcement rules live in `docs/scripts/lint-arch.py` and the regulatory compliance linter; this document does not duplicate the enumeration.

The discipline is captured by explicit anti-claims rather than by listing the forbidden words:

- ICN is not a payment system.
- ICN is not a wallet or balance dashboard.
- ICN does not issue currency.
- ICN is not a blockchain. It does not need consensus over a single global ledger; it records institutional transitions as receipts under domain authority.
- ICN does not issue tokens as proxies for participation.
- The model layer assists; it does not adopt rules.

The same discipline applies to the member shell: framing participation as account management or product usage misrepresents the institution. The institutional vocabulary the system actually uses lives in the "Safe vocabulary" section below.

## Safe vocabulary

When the architecture needs words for what the system actually does, it uses these:

- **obligation** — a commitment one party has made to another.
- **allocation** — a placement of resources under explicit authority.
- **settlement** — the recorded resolution of an obligation.
- **unit** — a counted measure inside a recorded position.
- **position** — a recorded standing in an obligation or allocation graph.
- **receipt** — the durable artifact that proves a transition occurred.
- **provenance** — the chain of authority that made a transition legitimate.
- **evidence** — material a process or service produces that is not itself authoritative.
- **identity** — the cryptographic and institutional record of who someone is.
- **standing** — the membership-derived state that determines what actions are open to an actor.
- **action card** — the surfaced, authority-aware participation prompt that closes the loop from standing back to a new transition.

These are the words this document, and the documents downstream of it, should use.

## Cooperative OS framing

There is a longer-horizon framing in which ICN's primitives are eventually packaged as a cooperative operating profile: a deployable bundle that a federation or institution can install to operate its own domain. That framing is real and is named here so it is not lost.

It is also explicitly **packaging direction, not current implementation scope**. The model does not require building a Linux distribution. The substrate runs in containers and on existing operating systems today. The eventual packaging story will mature once the underlying primitives are stable; trying to ship the package first would be backwards.

The package framing remains a forward direction; it is not a milestone of this document.

## Recommended next-step ordering

This is a recommendation, not a commitment. Concrete ordering and ownership are tracked in the issue backlog.

1. #1793 — this document.
2. #1797 — accepted-proposal effect dispatch contract.
3. #1794 — `InstitutionalDomain` and `DomainPolicy` primitive specification.
4. *(new, drafted in this PR's handoff)* — CCL policy registry, versioning, and governance-effect hook contract.
5. *(new, drafted in this PR's handoff)* — `GovernedServiceBinding`, `WorkloadManifest`, and `RuntimeProvider` specification.
6. #1798 plus *(new, drafted in this PR's handoff)* — `ArtifactRegistry` / `ScopedVault` boundary, together with the backup, replication, recovery, and archive policy specification.
7. #1801 — compute placement policy across local, cooperative, federation, and commons pools.
8. #1799 — network routing, redundancy, and anti-entropy proof loops.
9. #1795 — steward cockpit version zero.
10. *(new, drafted in this PR's handoff)* — member shell version zero UX specification.
11. Threat model consolidation — extend #1010 with explicit runtime/service/storage cross-links, or file a dedicated umbrella issue if reviewer judgment prefers separation.

Each step's place in the ladder is informed by what it unblocks. The effect-dispatch contract (#1797) unblocks the CCL registry and the service-binding model. The service-binding model in turn unblocks the backup-policy and compute-placement work. Threat-model consolidation rides last because the surfaces it covers are still settling.

## Related documents

Substrate and boundary:

- [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md) — meaning firewall, policy-oracle pattern.
- [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md) — generic shapes vs. package vocabulary.
- [`../ARCHITECTURE.md`](../ARCHITECTURE.md) — canonical system architecture.

Domain, hosting, tools, networking:

- [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) — domains, sessions, devices, vaults, routing, commons resource sharing.
- [`SERVICE_HOSTING_MODEL.md`](SERVICE_HOSTING_MODEL.md) — hosted-governed-native service stages, custody, exit.
- [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) — installable and forkable tools that emit receipts.
- [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md) — DNS routes bind authority; DNS is not authority itself.
- [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md) — compute assists; it does not govern.
- [`THE_COMMONS.md`](THE_COMMONS.md) — commons resource framing.
- [`PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`](PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md) — protocol choice for member-facing services.

Storage, design, and accessibility:

- [`../state/storage-governance-spec.md`](../state/storage-governance-spec.md) — storage class and locality as governance constraints.
- [`../design/compute-classes.md`](../design/compute-classes.md) — runtime class taxonomy.
- [`../design/deterministic-core.md`](../design/deterministic-core.md) — determinism requirement for legitimacy compute.
- [`../design/institutional-structure-spec.md`](../design/institutional-structure-spec.md) — entity, structure, activity model.
- [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — accessibility commitments at PR-time.

RFCs and forward-direction:

- [`../rfcs/RFC-0017-tool-install-infrastructure.md`](../rfcs/RFC-0017-tool-install-infrastructure.md) — tool install lifecycle, manifest, binding.
- [`../rfcs/RFC-0016-relationship-surface.md`](../rfcs/RFC-0016-relationship-surface.md) — relationship records.
- [`../rfcs/RFC-0001-obligation-allocation-settlement-primitives.md`](../rfcs/RFC-0001-obligation-allocation-settlement-primitives.md) — economic primitives.

Issue backlog (parent and siblings):

- #1689 — living repo atlas (broader scope than this document).
- #1748 — institutional process substrate.
- #1792, #1767 — private data disclosure, encrypted private overlay storage.
- #1793 — this document's parent issue.
- #1794 — `InstitutionalDomain` and `DomainPolicy` primitive.
- #1795 — steward cockpit.
- #1797 — accepted-proposal effect dispatch.
- #1798 — `ArtifactRegistry` and `ScopedVault` boundary.
- #1799 — network routing, redundancy, and anti-entropy proof loops.
- #1801 — compute placement policy.
- #1010 — adversarial model and chaos harness.

## Non-claims (repeated for clarity)

- This document does not implement any of the forward-direction primitives it names.
- It does not introduce new schema, contract, or wire format.
- It does not claim that any service is currently governed in the sense defined here.
- It does not claim that any partner federation is operating under these primitives today.
- It does not claim production readiness for any subsystem.
- It does not claim live inter-cooperative federation.
- It does not mutate deployed infrastructure.
- It does not introduce institution-package vocabulary into ICN core.
- It does not use payment, wallet, balance, currency, token, or blockchain as ICN-native framing; those words appear only inside the explicit anti-claim sentences in the "Vocabulary discipline" section.
- It does not close issue #1793 by itself; the parent issue's acceptance criteria are evaluated separately.
