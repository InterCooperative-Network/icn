---
Status: normative
Authority: spec (defines the forward-direction primitives ICN_INTEGRATED_SYSTEM_MODEL.md §"Authority — InstitutionalDomain and DomainPolicy" defers to #1794; some clauses are explicitly forward-direction and named as such)
Canonical: no
Last Reviewed: 2026-05-14
---

# InstitutionalDomain and DomainPolicy

> **Status: spec, work-in-progress.** Defines `InstitutionalDomain` and `DomainPolicy` as the design-level forward-direction primitives that the integrated system model already names. The spec stays at object/lifecycle granularity; it deliberately does not introduce schemas, wire formats, or runtime implementation. Clauses that depend on objects, schema fields, or kernel behavior that have not yet landed are marked as forward-direction. The PR introducing this doc advances #1794 without closing it.

## Purpose

`InstitutionalDomain` is the **governed operating jurisdiction** in ICN. It is the boundary inside which a sovereign entity decides what is legitimate: who its members are, what charters and policies bind them, which services and tools may act, where data lives, which receipts close the loop. `DomainPolicy` is the bundle of rules a domain adopts to make those decisions repeatable.

These two objects are already named by the canon. `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` says: *"`InstitutionalDomain` — the addressable scope of a federation, cooperative, community, or other institution. Owns its own members, charters, agreements, services, storage, and routing decisions"* and *"`DomainPolicy` — the live set of rules adopted by a domain. Bound to versioned charter text. The first input to every authority decision."* The same section flags both as forward-direction primitives "tracked under #1794." `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` adds the operating context — domains, sessions, devices, services, workspaces, agreements, vaults — without specifying the objects themselves.

This spec is that specification. It does not invent new vocabulary; it details the objects already named and locates them precisely inside the existing architecture, the effect dispatch chain (`docs/spec/effect-dispatch-contract.md`), and the institution-package boundary.

## What this spec is not

- Not runtime implementation. No code lands here.
- Not schema migration. No Rust types, JSON schemas, or wire formats are introduced.
- Not authority to mutate DNS, K3s, Forgejo, identity-bridge, or any deployed infrastructure.
- Not a production readiness claim.
- Not a live federation claim.
- Not a formal pilot claim for any partner institution.
- Not a package vocabulary surface. Institution-package nouns (a specific federation's role names, ceremonies, fixtures) stay in their own repositories.
- Not a redefinition of the meaning firewall, the policy-oracle pattern, the kernel/app boundary, or the effect dispatch chain. Where this spec touches those areas, the existing canon remains authoritative.
- Not closure of #1794. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's acceptance criteria.

## Boundary lines (what a domain is NOT)

Many things sit near `InstitutionalDomain` in the architecture; none of them are equivalent to it.

| Concept | Distinct from a domain in this way |
|---|---|
| **DNS name / public route** | DNS routes people; it does not define authority. A domain may move nodes, change routes, lose a name on the public internet, and still be itself. See `docs/architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md`: *"An institutional domain is not a DNS domain. The institutional domain is the ICN object/scope; the DNS domain is a routable address."* |
| **Node** | A node runs infrastructure. A node is operationally responsible for what it hosts. A node does not own institutional truth. A single domain may be served by many nodes; a single node may serve many domains. |
| **Federation** | A federation is an agreement among domains. Federation coordinates; it does not subsume. A federation has no charter, no members, no standing model of its own — it has signatories, treaties, and a routing surface. (Where a federation does have its own membership and charter, that is itself a domain whose owning-entity class is `Federation`, distinct from the relational federation graph.) |
| **Institution package** | A package provides local vocabulary and workflows: this federation's role names, this cooperative's intake form, this community's repair ceremony. The domain is the jurisdiction the package runs inside. ICN core defines the generic domain shape; packages bring the local nouns. See `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`. |
| **App / tool** | Apps and tools act inside domains under explicit binding. The domain authorizes; the tool acts. A `ToolBinding` (per RFC-0017) is the relationship; the domain is the authority that adopts the binding. |
| **Member account** | A member holds standing inside a domain. The member is identified by a DID that survives any single domain; the standing is bounded by the domain that recognizes it. |
| **Storage bucket** | Storage is custody under the domain's policy. The bucket is the byte container; the domain decides what custody class applies and which authority is required to read, modify, or export. |
| **CCL document** | CCL is the executable rule layer adopted *by* a domain (per `docs/spec/effect-dispatch-contract.md` §"CCL hook points"). The CCL document expresses rules; the domain is the body that adopted them. CCL never owns authority. |
| **Tenant** | A tenant is a billing entity inside someone else's product. A domain is a self-governing jurisdiction with its own charter, its own membership, its own evidence trail, and its own right to leave. |

The shorthand the canon already uses, repeated here so the rest of the spec can lean on it:

> DNS routes people. Nodes run infrastructure. Domains hold institutional authority under charter, policy, standing, and receipts. Federations coordinate domains by agreement. Packages express local vocabulary and workflows. ICN core provides the generic institutional grammar.

## Placement in the civic loop

In the civic loop named by `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md`, `InstitutionalDomain` sits at the **Authority** stage. Every later stage — action card, authorized action, CCL evaluation, dispatched transition, receipt, sync/federation, member shell, challenge/repair — derives its legitimacy from a decision rooted in some domain. `DomainPolicy` is what makes those decisions repeatable; it is the persistent shape of the domain's rule set, separate from the proposal-by-proposal acceptances that mutate it.

`InstitutionalDomain` is also the load-bearing object the institution-package boundary turns on. ICN core defines the generic shape here; packages provide the populated vocabulary for a specific federation. A package that tried to redefine `InstitutionalDomain` would breach the boundary; a package that bound a `DomainPolicy` instance to its local charter is doing exactly what the boundary requires.

## InstitutionalDomain (object outline)

The object is defined at design granularity. Each field below is named and explained; no Rust type, no serialization format, no field order is fixed here. The forthcoming schema work (drafted as a follow-up in this PR's handoff) will turn the design into a wire-stable form.

**Identity**

- **Canonical identifier.** A stable DID-style identifier for the domain. Survives node migrations, route changes, route loss, and operator turnover. Distinct from any DNS name that may currently route to it. Pluggable into the identity layer the rest of ICN already uses.
- **Owning entity class.** One of `Individual` / `Cooperative` / `Community` / `Federation`, or another governed entity class where domain policy explicitly permits. The class shapes which adopted-charter forms are permitted and what authority classes the entity may grant.

**Constitutional inputs (refs, not embedded content)**

- **Charter references.** Pointers to the versioned charter(s) the domain has adopted, including their amendment history. The domain does not own charter text; it owns the adoption record.
- **Adopted CCL policy references.** Pointers to the versioned CCL policy documents the domain currently runs against. Adoption is via governance decision; an unadopted CCL document is inert. This connects directly to `docs/spec/effect-dispatch-contract.md` §"CCL hook points" — Stage 2 (mandate composition) and Stage 3 (effect plan) consume the adopted CCL policy.

**Membership and standing**

- **Membership policy reference.** Who may join, by what process, with what evidence, and what disqualifies.
- **Standing model reference.** How active a member is, how that activity decays, how disputes affect it, what membership tiers (if any) the domain recognizes. Standing is what the member shell shows the member; it derives from policy plus history.
- **Role / authority model reference.** Which roles exist, how they are filled, how they are revoked, which authority classes (per ADR-0014 — `Representation`, `Execution`, `Attestation`) each role can hold.

**Devices and services**

- **Device identity & enrollment policy reference.** Which devices may speak for which members and services, how enrollment is approved, how recovery works, when revocation fires. `DeviceIdentity` and `DeviceEnrollment` are named in `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` as forward primitives; their full lifecycle is the subject of a deferred follow-up (see "Open questions" below).
- **Service identity registry.** The set of services the domain has issued long-lived identities to (forge, status page, identity bridge, registry, hosted tool, model runner, import adapter). `ServiceIdentity` per `SERVICE_HOSTING_MODEL.md` and RFC-0017. Issuance, rotation, and revocation are forward work.

**Workspaces, tools, agreements**

- **Workspace registry.** Bounded collaboration scopes inside the domain. A workspace inherits the domain's policy by default; explicit per-workspace policy is allowed when permitted by the domain's charter.
- **Tool registry / `ToolBinding` set.** Tools installed under the domain (per RFC-0017). Each `ToolBinding` carries scoped capability, custody class, audit trail, and exit policy. The domain authorizes the binding; the tool acts only within its bound capability.
- **Agreement registry.** Treaties, MoUs, federation memberships, partner exchanges, custodian arrangements. The eight agreement classes named in `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` populate this registry. Agreements bind one domain to another (or to a third-party custodian) under explicit, receipted terms.

**Storage, compute, routing**

- **Storage / vault policy reference.** The storage taxonomy from `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Storage as custody" — canonical, service state, artifact, volume, scoped vault, secret, cache. The domain owns the policy that classifies each class and binds it to backup/recovery (cross-link `#1816`).
- **Compute policy reference.** Placement (local / cooperative / federation / commons), runtime class allowances (deterministic legitimacy compute, utility compute, container, microVM, accelerator, local device, external bridge), and the authority required for each. Cross-link `#1801`.
- **Routing / DNS bindings.** The current set of `DnsBinding`s the domain has bound to public routes. Bindings are governed objects (per `DOMAIN_ROUTING_AND_DNS_BINDINGS.md`); changing one is a governance act. The route is a binding to authority, never the source of authority.

**Receipts**

- **Receipt policy reference.** Retention defaults per receipt class (see `effect-dispatch-contract.md` §"Receipt class summary"), export policy, archive policy, integrity verification cadence. Cross-link `#1816`.

**Accessibility and translation**

- **Accessibility policy reference.** The domain's binding to the accessibility commitments codified in `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`. Accessibility is treated as an architecture commitment, not a polish layer.
- **Translation policy reference.** Languages the domain supports for member-facing surfaces, review process for translations, fallbacks where translation has not been completed.

**Backup, recovery, exit**

- **Export / backup / recovery policy reference.** What can be exported, by whom, under what authority, on what cadence; what restoration looks like; what disaster recovery proves. Cross-link `#1816`.
- **Exit policy.** The right to leave is constitutional. The exit path includes export of authoritative state, custody handoff (where third parties hold material under agreement), reversal of `DnsBinding`s, and termination of `ToolBinding`s.

**Federation**

- **Federation agreement registry.** The treaties and memberships that bind this domain to others. Per `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md`: a federation is a coordination layer over signatory domains, not a parent of them.

**Challenge, repair, exit**

- **Challenge / repair paths.** Every domain transition must be challengeable. Challenges are themselves governance acts (per `docs/spec/effect-dispatch-contract.md` §"Challenge / reversal / counter-receipt"). The domain registers the paths and the authority required to invoke each.

The whole object is referenced by a single canonical identifier and reified by the references it carries. The domain does not embed the content of its charter, its CCL policy, its membership rolls, its tool bindings; it carries the pointers and the adoption history. This keeps the domain object small, versionable, and exportable.

## DomainPolicy (rule bundle outline)

`DomainPolicy` is the persistent shape of the rules a domain has adopted. It is not the proposal that adopted them; it is the resulting policy state. Every entry is a default, a gate, or both.

**Storage defaults and gates**

- **Storage defaults.** Which `StorageClass` applies to which kind of object by default, which `DataLocality` is permitted (per `docs/state/storage-governance-spec.md`).
- **Replication defaults.** Number of replicas, placement constraints, anti-entropy expectations (cross-link `#1799`).
- **Backup / recovery / archive defaults.** Frequency, target, integrity verification, retention; restore-test cadence; archive immutability requirements (cross-link `#1816`).

**Privacy and disclosure**

- **Privacy / disclosure defaults.** Which fields are public-by-policy, which are scope-restricted, which are vault-stored. Disclosure boundaries (cross-link `#1792`); scoped-vault rules (cross-link `#1767`).
- **Public / private surface policy.** Which member-facing and operator-facing surfaces are publicly indexable, which are auth-gated, which are private-overlay-only.

**Compute placement and review**

- **Compute placement defaults.** Per the pools named in `#1801` and the runtime classes named in the spine doc.
- **Model / agent workload review policy.** Where the domain permits model-assisted drafting or summarization (utility compute), the policy names the review surface and the receipt class that records human ratification. Models propose; governance adopts.

**Receipts**

- **Receipt retention defaults.** Per receipt class, the retention horizon, the export surface, the archive transition (cross-link `effect-dispatch-contract.md` §"Receipt class summary").
- **Receipt export defaults.** Who can export, under what authority, in what form, with what redactions.

**Tools, services, routes**

- **Tool install authority.** Which roles can adopt a new `ToolBinding`, which can suspend one, which can revoke. Cross-link RFC-0017 and `#1815`.
- **Service identity authority.** Which roles can issue or revoke a `ServiceIdentity`.
- **Route / DNS binding authority.** Which roles can propose a `DnsBinding` change, what the review path looks like, what receipt class the change emits.

**Devices and recovery**

- **Member / device recovery policy.** How a member regains access after device loss; how the domain authenticates a recovery; what protections prevent social-engineering recovery; what backup-of-last-resort the domain accepts.

**Accessibility and language**

- **Accessibility gates.** Bound to `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`. The policy names which surfaces must pass which categories.
- **Language gates.** Required languages, optional languages, fallback language, translation review process.

**Authority — ordinary and emergency**

- **Ordinary authority rules.** Mapped onto `AuthorityClass` (`Representation` / `Execution` / `Attestation`) per ADR-0014, with `TypedScope` per ADR-0014.
- **Emergency / temporary authority rules.** When a domain may grant time-bounded extraordinary authority (response to incident, federation request, custodian disability), under what review, with what reversal expectation. Emergency grants are receipted and time-bounded; no authority is ever silent.

**Adoption, versioning, evaluation**

- **Authorship and adoption history.** Every policy version names who drafted it, who reviewed it, who adopted it, and against which charter clauses.
- **Amendment path.** How the policy is amended: which proposal class, which threshold, which review window, which receipt class records the amendment.
- **Stable version identifier.** A content-addressed reference so other systems (the effect dispatch chain, the steward cockpit, the member shell, federation peers) can name the exact policy version that was in force at a given moment.
- **Inertness rule.** An unadopted `DomainPolicy` is inert — referencing it produces no authority, no constraint, no effect. Only the adopted version binds.

The domain may carry multiple `DomainPolicy` versions in history (for audit) but exactly one current version. Transitions between current versions are governance acts with receipts.

## DomainPolicy evaluation

`DomainPolicy` is consumed at well-defined points in the dispatch chain. It does not run inside the kernel.

**Where policy is consulted:**

- **At decision admission.** When a proposal is considered for admission to the process substrate, the relevant `DomainPolicy` evaluates whether the proposal class is permitted, who may sponsor it, what threshold applies.
- **At mandate composition (Stage 2 of effect dispatch).** Per `docs/spec/effect-dispatch-contract.md` §"CCL hook points", a CCL evaluator informed by adopted `DomainPolicy` produces structured grant sets (or returns empty, triggering `Mandate::new_pending_grants`).
- **At effect plan generation (Stage 3).** For proposal kinds CCL governs, the evaluator produces a structured `EffectManifest` whose contents the kernel later enforces blindly.
- **At binding adoption.** A new `ToolBinding`, `DnsBinding`, or agreement is validated against the relevant `DomainPolicy` clauses before adoption.

**Where policy is NOT consulted:**

- **Inside the kernel.** The kernel sees `ConstraintSet` values, not `DomainPolicy` semantics. Per `docs/architecture/KERNEL_APP_SEPARATION.md`, semantic interpretation is forbidden in kernel crates.
- **Inside packages, beyond their bound domain.** A package can read its bound domain's policy; it does not read another domain's policy unless an agreement makes that explicit.

**Determinism and reviewability:**

- Policy evaluation must be deterministic for any given (policy version, input). Non-deterministic evaluation is a bug.
- Policy text is reviewable. The adopted version is human-readable; the amendment history is queryable; the rationale is on record.

This spec does not specify the CCL grammar that expresses policy clauses. CCL policy registry, versioning, and the evaluator-selection contract belong to `#1817`.

## Domain lifecycle

A domain moves through named stages. Each transition requires an authority basis and emits a receipt. Skipping a stage breaks the chain.

| Stage | Transition | Authority required | Receipt class |
|---|---|---|---|
| **Declare** | A founding decision establishes the domain's canonical identifier and owning entity class. | Founding decision in whatever forum the owning entity considers sovereign. | Decision receipt + domain-creation record. |
| **Adopt charter / policy** | The domain binds versioned charter text and an initial `DomainPolicy`. | Founding decision plus ratification per the charter's process. | Decision receipt + adopted-policy record. |
| **Initialize standing / membership** | Initial members are admitted and their standing recorded. | Adopted membership policy. | Membership effect records (per ADR-0025 forward direction). |
| **Bind routes / services / tools** | The domain adopts initial `DnsBinding`s, `ServiceIdentity`s, and `ToolBinding`s. | Authority per `DomainPolicy`. | Binding-creation effect records. |
| **Operate** | Day-to-day acceptance of proposals, dispatch of effects, emission of receipts. | Adopted policy + ongoing governance acts. | The full effect-dispatch chain (per `effect-dispatch-contract.md`). |
| **Amend** | The domain mutates its charter, policy, or registry. | Amendment authority per the charter. | Decision receipt + amendment record. |
| **Federate** | The domain enters or exits a federation agreement. | Agreement-class authority per `DomainPolicy`. | Agreement record + counter-signatures from peer domains. |
| **Suspend / repair** | A challenge produces a temporary suspension, a remediation, or both. | Challenge-resolution path per the charter. | Reversal mandate + counter-effects per `effect-dispatch-contract.md` §"Challenge / reversal / counter-receipt". |
| **Export / exit / archive** | The domain exports authoritative state, terminates `DnsBinding`s and `ToolBinding`s, hands off custody where applicable, and archives. | Exit policy. | Export receipt + archive transition record. |

**Lifecycle invariants:**

- No transition without authority basis.
- No transition without receipt expectation.
- No silent transition. A transition the member shell and steward cockpit cannot see did not happen, institutionally.
- Reversal is a separate transition, not a retroactive edit. The chain is append-only.
- The right to exit is constitutional. A domain may always leave; the exit path is governed but unblockable.

## Relationship to effect dispatch

`DomainPolicy` is the upstream input to multiple stages of the effect dispatch chain specified in `docs/spec/effect-dispatch-contract.md`:

| Effect dispatch stage | DomainPolicy role |
|---|---|
| Decision admission (pre-Stage 1) | Determines whether a proposal class is permitted, who can sponsor, what threshold applies. |
| Stage 1 (Decision recording) | Policy version in force at acceptance time is named in the `GovernanceDecisionReceipt` provenance. |
| Stage 2 (Mandate minting) | CCL evaluator (per `#1817`) reads the adopted policy to compose grants conservatively; informs whether the seam mints `AuthorityGrant`s or falls through to `Mandate::new_pending_grants`. |
| Stage 3 (Effect plan) | CCL evaluator produces a structured `EffectManifest`; the manifest's deterministic translation depends on the adopted policy version. |
| Stage 4 (Effect dispatch) | Kernel sees only `KernelEffect`; policy is not consulted in the kernel. |
| Stage 5 (Application + evidence) | `InstitutionalEffectRecord` and dispatch evidence reference the policy version (forward direction; depends on the schema work tracked under `#1817` and the deferred-schema items named in `effect-dispatch-contract.md` §"Future schema work"). |

`DomainPolicy` is also where the adjacent specs land their content: storage defaults from `#1816`, compute placement from `#1801`, scoped vault custody from `#1767`/`#1792`, action-card rendering from `#1818`/`#1795`, accessibility commitments from `ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`.

## Relationship to future work

This spec cross-links existing canon and forward issues rather than duplicating them.

| Concern | Where it lives |
|---|---|
| Service binding, workload manifest, runtime provider | `#1815`. |
| Backup, replication, recovery, archive policies | `#1816`. |
| CCL policy registry, versioning, evaluator-selection contract | `#1817`. |
| Member shell v0 UX | `#1818`. |
| ArtifactRegistry / ScopedVault | `#1798`. |
| Compute placement policy | `#1801`. |
| Network anti-entropy proof loops | `#1799`. |
| Steward cockpit v0 | `#1795`. |
| Tool install lifecycle | `RFC-0017`. |
| Constitutional object model (AuthorityClass, AuthorityGrant, TypedScope, Mandate) | `ADR-0014`. |
| Minimal runtime root for `InstitutionalDomain` / `DomainPolicy` (Declare + Adopt slice) | `ADR-0083` (#2142). Declare + Adopt-policy slice **landed** in `icn-governance` (`institutional_domain` module: `InstitutionalDomain`, `DomainPolicy` / `DomainPolicyRef`, fail-closed `adopt_policy`; tests cover declare, adopt, structural inertness, and missing/ambiguous/unbound/inactive authority). App-side adoption is now **gate-resolved**: `apps/governance::domain_policy_adoption::adopt_domain_policy_gated` runs the existing `DefaultMandateGate::require()` (actor → active grants → `TypedScope.domain` + Execution class + `domain_policy:adopt` act token + mandate lifecycle) before the pure-core structural commit (#2142 follow-up). The adoption seam is now reachable at the governance app boundary via `GovernanceManager::adopt_domain_policy` (#2166), which fails closed (`DomainPolicyAdoptionError::MissingReceiptBackend`) when no receipt backend is wired and otherwise delegates to the gated helper. Full lifecycle (standing, services, routing, federation, exit) and CCL evaluation remain forward work; a domain-policy **HTTP surface is deferred and blocked** — see "Domain-policy adoption: app boundary and HTTP-surface sequencing" below. |
| Accepted-proposal effect dispatch contract | `docs/spec/effect-dispatch-contract.md` (`#1797`, merged). |
| Integrated cooperative operating model | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` (`#1793`, merged). |

## Domain-policy adoption: app boundary and HTTP-surface sequencing

> **Status: blocker / sequencing note (#2142).** Records why a domain-policy adoption HTTP route is **not** added yet, and what must be decided and built first. No HTTP route exists for domain-policy adoption.

The #2142 lane has landed the adoption path up to the governance **application boundary**:

- **Runtime root (#2162).** `icn-governance::institutional_domain` — `InstitutionalDomain` (keyed by the existing `GovernanceDomainId`), `DomainPolicy` / `DomainPolicyRef` (content-addressed), and a fail-closed structural `InstitutionalDomain::adopt_policy`.
- **Gate-resolved adoption (#2164).** `apps/governance::domain_policy_adoption::adopt_domain_policy_gated` runs the real `DefaultMandateGate::require()` (actor → active grants → `TypedScope.domain` + Execution class + `domain_policy:adopt` act token + mandate lifecycle), then commits through the pure-core structural check as defense-in-depth.
- **Manager seam (#2166).** `GovernanceManager::adopt_domain_policy(&self, &mut InstitutionalDomain, &DomainPolicy, &Did, now)` — fail-closed (`DomainPolicyAdoptionError::MissingReceiptBackend`) without a wired receipt backend; otherwise delegates to the gated helper.

**Why the HTTP route is blocked.** The manager seam takes a **caller-held `&mut InstitutionalDomain`**, because there is **no `InstitutionalDomain` persistence** in the workspace today: no store, no load/save path, and no declare/create path (`InstitutionalDomain::declare` is exercised only in tests). The existing `GovernanceStateStore` persists `GovernanceDomain` (the decision-space config object), **not** `InstitutionalDomain` (the standing authority wrapper) — they remain sibling references per ADR-0083. An HTTP route, by contrast, must **load** the target domain, **mutate** it, and **persist** the result. With no load/save/declare path, any route would either operate on a transient, discarded domain (fake persistence) or accept whole `InstitutionalDomain` / `DomainPolicy` blobs over the wire (a fake boundary). Neither is honest, so the route is deferred.

**This blocker intersects ADR-0083's open questions** — it should not be resolved by silently introducing a store:

- **Open Q2 (Domain vs decision space):** whether `GovernanceDomain` becomes a sub-part of `InstitutionalDomain` or they stay sibling references. An `InstitutionalDomain` store commits to a representation here.
- **Open Q4 (`DomainPolicy`: stored object vs derived view):** whether the adopted policy is a stored record or a view over adoption receipts. A store commits to "stored object."
- **Open Q1 (identifier):** the MVP keys on `GovernanceDomainId` (a string); a persistence layer is where a distinct DID-style `InstitutionalDomainId` would (or would not) be introduced.

**Required sequence to unblock the HTTP route** (each a separate, reviewable lane):

1. **Decide the persistence model** — **RESOLVED BY DESIGN** (not yet in code) in the **ADR-0083 Addendum (2026-06-23): InstitutionalDomain persistence model**. Decisions: persist `InstitutionalDomain` keyed by the existing `GovernanceDomainId` (Q1 — no parallel id); store it as a **separate sibling record**, no `GovernanceDomain` consolidation (Q2); persist only the adopted `current_policy: DomainPolicyRef` pointer, deferring the `DomainPolicy` body to #1817 (Q4); and **extend the existing `GovernanceStateStore`** with default-implemented `get_institutional_domain` / `save_institutional_domain` rather than adding a new store.
2. **Add the `InstitutionalDomain` persistence seam** per that decision (extend `GovernanceStateStore` + `GovernanceManager` persisted load→adopt→save method). *Not started.*
3. **Add a declare/create path** so a domain exists to adopt into (a governance act in its own right; declare-authority gating is a flagged sub-question in the addendum). *Not started.*
4. **Then** add the thin governance HTTP route, which calls `GovernanceManager::adopt_domain_policy` (it must not bypass the seam) over the persisted domain. *Not started.*

Until step 1 is decided, the adoption capability is **complete and tested up to the manager seam** and reachable in-process; no network surface is claimed. #2142 remains open.

## Adjacent concepts (named, not specified here)

These objects appear in the field outline above. Some are already partly specified; some are named only. None are fully specified in this document.

- **`DomainSession`** — bounded session inside a domain for a member or service. Already named in `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`. Full specification is forward work.
- **`DeviceIdentity` / `DeviceEnrollment`** — hardware identity and enrollment chain. Named in `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`. Recovery semantics are a candidate follow-up issue (see "Open questions").
- **`ServiceIdentity`** — long-lived service-side identity issued by a domain. Partially specified by `SERVICE_HOSTING_MODEL.md` and RFC-0017. Full issuance / rotation / revocation lifecycle is forward work.
- **`Workspace`** — bounded collaboration scope. Named in `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`. Ownership and scoping model are forward work.
- **`AgreementRegistry`** — registry of treaties, MoUs, and federation memberships. Eight agreement classes are named in `COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`. The registry shape is forward work.
- **`ToolRegistry`** — set of installed `ToolBinding`s. Tool install lifecycle is RFC-0017.
- **`DnsBinding`** — binding between a domain and a routable name. Specified in `DOMAIN_ROUTING_AND_DNS_BINDINGS.md`.

This spec names these so the field outline is honest about what it references; it does not redefine any of them.

## Open questions and deferred decisions

Each is named so future readers know where this spec stops.

| Question | Deferred to |
|---|---|
| Wire-stable schema for `InstitutionalDomain` and `DomainPolicy` records | Follow-up `schema(domain): …` issue drafted in this PR's handoff. |
| `DeviceIdentity` enrollment and recovery flow | Follow-up `spec(identity): …` issue drafted in this PR's handoff. |
| `ServiceIdentity` issuance, rotation, and revocation lifecycle | Follow-up `spec(identity): …` and RFC-0017 implementation. |
| `Workspace` ownership and cross-domain scoping rules | Follow-up `spec(workspace): …` issue, optional. |
| Federation `AgreementRegistry` schema and lifecycle | Follow-up `spec(agreement): …` issue, drafted in this PR's handoff. |
| `DnsBinding` authority and receipt contract (already partially specified) | Either a follow-up `spec(routing): …` issue or an extension of `DOMAIN_ROUTING_AND_DNS_BINDINGS.md`; flagged for human triage in the handoff. |
| CCL policy registry, versioning, evaluator selection | `#1817`. |
| Effect dispatch deferred-schema work | `effect-dispatch-contract.md` §"Future schema work" and follow-ups after `#1797` spec acceptance. |
| Member shell v0 surfacing of domain state | `#1818`. |
| Steward cockpit v0 surfacing of policy state | `#1795`. |
| Kernel-side mandate enforcement | ADR-0019 "Non-decisions"; future ADR. |

## Non-claims (repeated for clarity)

- This document does not introduce any of the forward-direction primitives it names. They remain forward-direction.
- It does not introduce schema, wire format, or contract changes.
- It does not authorize the kernel to consult `DomainPolicy` directly.
- It does not authorize mutation of any deployed infrastructure (DNS, K3s, Forgejo, identity-bridge, gateway).
- It does not claim production readiness for any subsystem.
- It does not claim live inter-cooperative federation.
- It does not claim that any partner federation is operating under these primitives today. Where this spec gives examples, the examples are generic; institution-package vocabulary stays in institution packages.
- It does not use payment, wallet, balance, currency, or blockchain as ICN-native framing. The discipline list in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full.
- It does not by itself close `#1794`. The PR uses `Refs:`.

## Related documents and issues

Architecture and spine:

- `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` — §"Authority — InstitutionalDomain and DomainPolicy" (the section this spec details).
- `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` — the design-direction overview of cooperative domain infrastructure.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — generic shapes vs. package vocabulary.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — meaning firewall.
- `docs/architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md` — DNS routes, authority does not.
- `docs/architecture/SERVICE_HOSTING_MODEL.md` — hosted → governed → native service maturity.
- `docs/architecture/COOPERATIVE_TOOL_COMMONS.md` — tool commons doctrine.

Specs and state:

- `docs/spec/effect-dispatch-contract.md` — accepted-proposal effect dispatch (`#1797`, merged).
- `docs/spec/KERNEL_CONTRACTS.md` — kernel contract primitives.
- `docs/state/storage-governance-spec.md` — storage classes and locality.

Design and accessibility:

- `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` — accessibility commitments treated as architecture.
- `docs/design/compute-classes.md` — runtime class taxonomy.
- `docs/design/deterministic-core.md` — determinism for legitimacy compute.

ADRs:

- ADR-0014 — Constitutional Object Model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`).
- ADR-0019 — Authority Grant Minting and Mandate Persistence Seam.
- ADR-0020 — Institutional Bootstrap Activation and Standing Read Model.
- ADR-0023 — CCL Institutional Process Language.
- ADR-0024 — Institution Package Manifest Schema.
- ADR-0025 — Institutional Effect Record Canonical Schema (proposed).
- ADR-0026 — Receipt and Provenance Proof Envelope.
- ADR-0027 — Action Card Contract.
- ADR-0030 — Compute Workload Manifest and Authority Boundary.

RFCs:

- RFC-0017 — Tool Install Infrastructure.
- RFC-0016 — Relationship Surface.
- RFC-0001 — Obligation, allocation, settlement primitives.

Issues:

- `#1794` — this spec.
- `#1793` — integrated cooperative operating model (parent; merged).
- `#1797` — accepted-proposal effect dispatch contract (sibling; merged).
- `#1815` — service binding / workload manifest / runtime provider.
- `#1816` — backup / replication / recovery / archive policies.
- `#1817` — CCL policy registry / versioning / governance-effect hook contract.
- `#1818` — member shell v0 UX.
- `#1792`, `#1767` — private data disclosure boundary and encrypted private overlay storage.
- `#1798` — ArtifactRegistry and ScopedVault.
- `#1801` — compute placement.
- `#1799` — network routing, redundancy, anti-entropy proof loops.
- `#1795` — steward cockpit.
- `#1748` — institutional process substrate.
