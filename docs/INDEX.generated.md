# ICN Documentation Index

Canonical index of all ICN documentation, organized by category.
---

## Quick Navigation

- [Architecture](#architecture)
- [Design](#design)
- [Guide](#guide)
- [Operations](#operations)
- [Reference](#reference)
- [Security](#security)
- [Status](#status)
- [Strategy](#strategy)

---

## Architecture

### 🔒 **Canonical** [ICN Vision Statement](/VISION.md)

Core vision, values, and long-term aspiration for InterCooperative Network

**For:** `all` | **Updated:** 2026-02-01

### 🔒 **Canonical** [ICN Architecture Reference](/docs/ARCHITECTURE.md)

Single authoritative architecture covering all 8 primitives, kernel-app separation, subsystems, and implementation status

**For:** `developers`, `architects`, `grant-reviewers` | **Updated:** 2026-03-21

### 🔒 **Canonical** [ICN Design Principles](/docs/DESIGN_PRINCIPLES.md)

Canonical three-tier index of operational invariants, firewall contract, and frozen-core governance invariants. Pairs with the design system entry point as the kernel-side counterpart.

**For:** `developers`, `architects`, `designers` | **Updated:** 2026-05-17

### 📝 **Living** [ADR-0001: Orchestration Plane Architecture (decision superseded by ADR-0017)](/docs/adr/ADR-0001-orchestration-plane-architecture.md)

Original multi-repo orchestration-plane decision; physical layout DECISION superseded by ADR-0017, durable principle (explicit orchestration/state plane) retained. ADR remains as institutional memory at canonical path.

**For:** `architects` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0002: MCP Server Registration via ~/.mcp.json (amended by ADR-0017)](/docs/adr/ADR-0002-mcp-server-registration-via-mcp-json.md)

Registration mechanism unchanged; path moved from a separate icn-ops/ repo into ops/mcp/ in the main ICN repo (decision amended by ADR-0017).

**For:** `developers` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0010: App Topology (decision superseded by ADR-0017)](/docs/adr/ADR-0010-app-topology.md)

Architectural decision record on app topology — DECISION superseded by ADR-0017 (canonical-roots table); ADR file remains at canonical path as institutional memory.

**For:** `architects` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0012: Federation State Origin Model (decision amended by ADR-0013)](/docs/adr/ADR-0012-federation-state-origin-model.md)

Model C (Explicit Parallel) for federation clearing state origins. Decision still holds; Step 3 status now owned by ADR-0013.

**For:** `architects` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0013: Federation Clearing Adoption Contract](/docs/adr/ADR-0013-federation-clearing-adoption-contract.md)

Step 3 architecture for federation clearing adoption (3a–3d implemented). Open items remain: FederationProvenance not Sled-persisted, coop_a_did empty in execution handler, store-isolation tests not written. Verified unresolved 2026-04-26.

**For:** `architects` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0014: Constitutional Object Model — AuthorityClass, AuthorityGrant, TypedScope, Mandate](/docs/adr/ADR-0014-constitutional-object-model.md)

Semantic freeze for the four governance constitutional types. Decision: accepted; implementation: partially landed (types + accepted-decision minting seam land at the governance app layer; kernel dispatch is not yet gated by mandates). See ADR-0019 for the minting/persistence seam decision.

**For:** `architects`, `developers` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0015: Service Discovery Auth Semantics — Auth-gated with Enumeration-Safe 404](/docs/adr/ADR-0015-service-discovery-auth-semantics.md)

Decision: all /v1/services/* require JWT; missing/unauthorized -> 404 (enumeration-safe). Implementation status (2026-04-26): needs verification — route-level auth gating not visible in api/services.rs; follow-up audit issue suggested.

**For:** `architects`, `developers` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0017: Monorepo Consolidation with Explicit Internal Boundaries](/docs/adr/ADR-0017-monorepo-consolidation-with-explicit-internal-boundaries.md)

Canonical roots table for the consolidated monorepo: icn/, icn/apps/, apps/, website/, ops/mcp/, ops/state/, docs/, docs/adr/, deploy/, sdk/, web/. Decision supersedes ADR-0001 and ADR-0010; amends ADR-0002.

**For:** `architects`, `developers` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0018: ADR Lifecycle and Canonical Decision Index](/docs/adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md)

ADR lifecycle vocabulary (proposed/accepted/amended/superseded/deprecated), metadata convention (YAML frontmatter going forward, classic + bullet still supported), tooling contract for ops/mcp/src/tools/decisions.ts. Documents the docs/adr/ canonicalization landed in PR #1637.

**For:** `architects`, `developers` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0019: Authority Grant Minting and Mandate Persistence Seam](/docs/adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md)

Records the deterministic seam where accepted governance decisions emit zero or more truthful AuthorityGrants plus exactly one Mandate. Conservative minting (only steward-appointment / steward-reconfirmation today); pending-grants fall-through is the truthful failure mode; kernel dispatch is NOT gated by mandates. Implementation: partially landed.

**For:** `architects`, `developers` | **Updated:** 2026-04-26

### 📝 **Living** [ADR-0020: Institutional Bootstrap Activation and Standing Read Model](/docs/adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md)

Reusable activation chain: package manifest -> private overlay -> bootstrap apply -> charter activation -> entity/structure/role creation -> /me/standing -> (future) action cards. Locks the boundary between ICN runtime contract and institution-package vocabulary. Steps 1-6 implemented; action cards (icn#1608) future.

**For:** `architects`, `developers` | **Updated:** 2026-04-26

### 📝 **Living** [Abuse-case hardening strategy](/docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md)

Strategy / doctrine doc codifying ICN's institutional-failure-mode hardening layer: the substrate must not become an administrative panel with receipts. Ten one-line doctrine rules (receipts prove events not legitimacy; authority shortcuts must label themselves as shortcuts; unresolved standing is not standing in production; accepted is not applied; convenience paths must not become authority paths; bootstrap is not democracy; a capability token is not a mandate; a UI must not launder uncertainty into confidence; privacy posture is not private content; index absence is not record absence). Ten code-anchored abuse stories and matching hardening tracks: narrowing the broad governance:write scope, marking direct membership mutation and direct charter activation as bootstrap-only shortcuts with explicit administrative receipts, fail-closed resolver/checker policy in production, closed lifecycle vocabulary across API/shell/cockpit, per-effect idempotency, governance-parameter sanity bands, shell/cockpit fixture matrix, PrivateEvidence non-rendering regression, typed-receipt atomicity inventory. Strategy only — no runtime change, no new ADR, no new contract URN, no production-readiness claim. Companion to ARCHITECTURE_DUE_DILIGENCE.md and upstream of security/production-hardening.md.

**For:** `architects`, `contributors` | **Updated:** 2026-05-16

### 📝 **Living** [Architectural Gaps & Remediation Plan](/docs/architecture/ARCHITECTURAL_GAPS_AND_FIXES.md)

Analysis of architectural weaknesses and remediation strategies

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Architecture due diligence](/docs/architecture/ARCHITECTURE_DUE_DILIGENCE.md)

Process / principle doc codifying two checklists authors and reviewers run when introducing or changing an architectural surface: (1) convenience-vs-authority (centralized convenience is fine; centralized authority is not — name dependencies and assign them to the correct layer), and (2) participation access (language, plain-language, vision, motor, cognitive, bandwidth, AT compatibility, accommodation privacy — designed-in, not bolted on). Triggered by the rehearsal evidence schema's non-DNS $id decision and grounded in docs/design-language/accessibility.md.

**For:** `architects`, `contributors` | **Updated:** 2026-05-04

### 📝 **Living** [Canonical Encoding](/docs/architecture/CANONICAL_ENCODING.md)

Specification for deterministic serialization of ICN data structures

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Cells and Scopes Architecture](/docs/architecture/CELLS_AND_SCOPES.md)

Design of ICN's cell-based organization model and scope hierarchy

**For:** `architects`, `developers` | **Updated:** 2026-03-15

### 📝 **Living** [Client Model Architecture](/docs/architecture/CLIENT_MODEL.md)

Architecture of ICN client models and their relationship to kernel primitives

**For:** `developers`, `architects` | **Updated:** 2026-03-15

### 📋 **Draft** [Cooperative Domain Infrastructure](/docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md)

Forward-direction architecture for cooperative institutional infrastructure: institutional domains, sessions, devices, services, workspaces, artifacts, vaults, agreements, DNS bindings, hybrid commons cloud, workstations. Names what is implemented today vs what is future buildout.

**For:** `developers`, `architects` | **Updated:** 2026-04-27

### 📋 **Draft** [Cooperative Tool Commons](/docs/architecture/COOPERATIVE_TOOL_COMMONS.md)

Forward-direction tool ecosystem: core base tools, specialized suites, third-party tools, manifests, service identities, capability grants, anti-capture rules. Companion to Cooperative Domain Infrastructure.

**For:** `developers`, `architects` | **Updated:** 2026-04-27

### 📋 **Draft** [Domain Routing and DNS Bindings](/docs/architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md)

Forward-direction design distinguishing institutional domains, icn.zone utility routes, custom public domains, short routes, and binding verification receipts. Companion to Cooperative Domain Infrastructure.

**For:** `developers`, `architects` | **Updated:** 2026-04-27

### 📝 **Living** [Federation Actions](/docs/architecture/FEDERATION_ACTIONS.md)

Design of federated action execution across network boundaries

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Federation Interoperability Contract](/docs/architecture/FEDERATION_INTEROP_CONTRACT.md)

Specification of contracts and interfaces for federation interoperability

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Governance State Machine Architecture](/docs/architecture/GOVERNANCE_STATE_MACHINE.md)

State machine design for governance decision-making and enforcement

**For:** `architects`, `developers` | **Updated:** 2026-03-12

### 📋 **Draft** [ICN Integrated System Model](/docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md)

Forward-direction integrating spine across substrate, governance, CCL contract layer, service hosting, governed workloads, storage as custody, backup and recovery doctrine, networking, member shell, and steward cockpit. Locates every subsystem inside one civic loop (identity → standing → authority → action card → authorized action → CCL/runtime evaluation → storage/compute/governance/economic transition → receipt → sync/federation → member shell + steward cockpit → challenge/repair). Preserves the meaning firewall, separates substrate from app from institution package from teaching surface, and marks forward-direction objects (InstitutionalDomain, DomainPolicy, GovernedServiceBinding, WorkloadManifest, RuntimeProvider, StorageSpec, BackupPolicy, ReplicationPolicy, RecoveryPolicy, ArchivePolicy, IntegrityPolicy) as forward-direction. Names CCL as the executable institutional rule layer inside governance; not sovereign. Names Cooperative OS as packaging direction, not current implementation scope. Advances #1793 — does not by itself close it.

**For:** `developers`, `architects`, `contributors` | **Updated:** 2026-05-14

### ⚪ **active** [ICN Operating Model](/docs/architecture/ICN_OPERATING_MODEL.md)

Doctrine for vocabulary, placement, and operating-model grammar — the grammar map of ICN. Normative for what each layer means, what belongs where (ICN core vs engine vs package vs tool vs service vs surface vs kernel), who authorizes what, and what receipt proves it; NOT normative for current implementation status, which remains docs/STATE.md + docs/PHASE_PROGRESS.md. Fixes the standardized vocabulary (Substrate/Kernel, Engine, Domain, Policy, CCL, Tool, Service, Surface, Package, Binding, Manifest, Provider, Instance, Receipt, Bridge, Artifact, Vault, Agreement, Node) by cross-linking each term's canonical home rather than redefining it; restates the whole-system stack, the central civic loop, the definition/binding/instance/receipt pattern, the meaning-firewall and package/core boundary (package nouns appear only as examples of what does not belong in core), a placement table, a transition grammar (declare->review->adopt->bind->run->observe->amend->suspend->repair->export->exit->archive), and a six-question feature-placement checklist. Names the immediate vertical spine (package->domain->policy->binding->action->receipt->surface->evidence/export) and the parked zero-coverage gaps (CCL evaluator-selection runtime, GovernedServiceBinding runtime, BoundaryOutcomeReceipt/AgreementRegistry runtime). Makes no current-state claim; preserves regulatory vocabulary. Does not introduce schema, wire format, or runtime behavior.

**For:** `developers`, `architects`, `contributors` | **Updated:** 2026-06-22

### 📝 **Living** [Identity and Membership Architecture](/docs/architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md)

Design of identity primitives, membership verification, and member lifecycle

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Institutional Feedback and Support Primitives](/docs/architecture/INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md)

Doctrine for institutional feedback, member signals, governed indicators, action cards, temporary authority, resource governance, and support programs as planned ICN primitives

**For:** `developers`, `architects` | **Updated:** 2026-04-25

### 📝 **Living** [Institution Package Boundary](/docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md)

Normative definition of ICN platform vs institution package boundary, CCL vs host runtime split, and reusable primitive set for NYCN/Summit

**For:** `developers`, `architects` | **Updated:** 2026-04-16

### 📝 **Living** [Kernel/App Separation Architecture](/docs/architecture/KERNEL_APP_SEPARATION.md)

Normative specification of kernel-app boundary, infection vectors, and capability propagation rules

**For:** `developers`, `architects` | **Updated:** 2026-03-17

### 📋 **Draft** [Model Workloads and Deliberation](/docs/architecture/MODEL_WORKLOADS_AND_DELIBERATION.md)

Forward-direction design for model-driven advisory compute workloads, deliberation packets, model registry, advisory vs deterministic classification, and the rule that compute assists but does not govern.

**For:** `developers`, `architects` | **Updated:** 2026-04-27

### 📋 **Draft** [Private Data Disclosure Boundary, Scoped Vaults, and Access Receipts](/docs/architecture/PRIVATE_DATA_DISCLOSURE_BOUNDARY.md)

Design-only architecture boundary contract (#1792) for ICN's generic private-data disclosure/access model: private overlays, scoped vaults, opaque receipt storage, redaction, selective disclosure, disclosure policies, and access/export/made-available receipts, following the landed EvidencePacketExportPreparedReceipt. Names candidate vocabulary and a follow-up sequence, distinguishes disclosure/access policy (#1792) from encrypted private-overlay storage (#1767), and implements no runtime.

**For:** `architects`, `developers` | **Updated:** 2026-07-05

### 📝 **Living** [Scope Bounded Trust](/docs/architecture/SCOPE_BOUNDED_TRUST.md)

Trust model design limiting trust scope to organizational boundaries

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Constitutional Genesis](/docs/genesis.md)

Immutable substrate invariants, bootstrap sequence, and governance boundaries

**For:** `developers`, `agents`, `stakeholders` | **Updated:** 2026-04-10

### 📋 **Draft** [ICN Kernel Contracts Specification](/docs/spec/KERNEL_CONTRACTS.md)

Specification of kernel contract primitives

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Artifact Registry and Scoped Vault — boundary, v0 design](/docs/spec/artifact-registry-and-scoped-vault.md)

Defines the design-level shape of ArtifactRegistry v0 (the institutional record of content-addressed artifacts and their metadata) and ScopedVault (the privacy-enforced container for restricted objects), and the boundary between them. ArtifactRegistry fields: artifact_id, content_hash, blob_location, mime_type, size, artifact_class, scope, created_by/at, access_policy_ref, retention_policy_ref (resolving to BackupPolicy / ArchivePolicy per #1816, where retention is a field — there is no separate RetentionPolicy object), provenance_refs, receipt_refs, version_ref, parent_refs, exportability. ScopedVault fields: vault_id, owning_scope, privacy_class (from the forward-direction #1792 taxonomy, kept explicitly distinct from the existing in-code PrivacyClass enums in icn-kernel-api/src/compute.rs and icn-boundary/src/types.rs), encryption_key_model_placeholder (deferred to #1767), access_policy, retention_policy, backup_export_policy, access_receipt_requirement, export_receipt_requirement, private_overlay_binding. Names ArtifactReceipt (existing Layer 2 receipt in icn-kernel-api/src/proofs.rs:30) as distinct from ArtifactRegistry (the new registry record). Reuses canonical types verbatim: Hash / Did / Signature / StorageClass / DataLocality (kernel-api), and StorageSpec / BackupPolicy / ReplicationPolicy / RecoveryPolicy / ArchivePolicy / IntegrityPolicy (per #1816). Cross-links #1792's forward-direction PrivacyClass / DisclosurePolicy / PrivateObjectRef / AccessReceipt / ExportReceipt / RedactionMap vocabulary; surfaces the existing-PrivacyClass-enum naming collision and defers reconciliation to the implementation tranche. Specifies a closed artifact_class taxonomy (Document per #1536, ComputeOutput per #1815, EvidencePacket per #1748, PrivateEvidence per #1792, Backup per #1816, SettlementRecord per #1634, Other) growable by ADR amendment. Maps six integration points and a 13-row failure/safety table. Identifies first safe implementation slice: ArtifactRegistry v0 schema + Document artifact-class + read-only steward-cockpit registry surface. Advances #1798 — does not by itself close it.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14

### 📋 **Draft** [CCL Policy Registry and Hook Contract](/docs/spec/ccl-policy-registry.md)

Defines the CCL policy registry, policy-version model, adoption contract, evaluator-selection contract, evaluator-output → effect-plan contract, review/audit surfaces, and failure/safety rules. Bridges DomainPolicy (adopted CCL policy references) and the Stage 2/3 CCL hook points in the effect dispatch chain. Reuses existing types from icn-ccl (ContentHash, SemanticVersion, CclDocument, SchemaVersion, Capability) and icn-governance (GovernanceDecisionReceipt, GovernanceProof, Mandate, AuthorityGrant, EffectManifest). Specifies eight-step adoption contract, deterministic evaluator selection with fail-closed semantics for missing/conflicting/deprecated bindings, structured evaluator output (decision suggestion / reasons / effect plan / disclosure policy / receipt expectations / authority basis), audit surfaces (registry shows drafts and adopted versions; receipts carry policy_version_id provenance), and a complete failure/safety table. Extends ADR-0021 (CCL safety), ADR-0022 (schema bridge), and ADR-0023 (institutional process language) without redefining them. Advances #1817 — does not by itself close it. Defers wire-stable schema, evaluator execution envelope, adoption proposal lifecycle, federation mandate recognition, and other forward-direction items to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14

### 📋 **Draft** [Compute Placement Policy](/docs/spec/compute-placement-policy.md)

Defines the placement policy contract sitting between ADR-0030 (compute workload manifest and authority boundary) and ADR-0031 (commons compute admission and settlement policy): the policy decision a workload passes through before admission, execution, or rejection. Names seven closed placement classes (LocalOnly, DomainLocalPreferred, LocalDomainBound, FederationBound, CommonsEligible, ExternalCustodianRequired, RejectedByPolicy) using the corrected scope vocabulary from docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md §C3 (LocalDomain not Coop). Specifies the two-layer decision contract (eighteen candidate inputs; Layer 1 policy-oracle return value is PlacementDecision or PlacementRejected, with optional attached PlacementFallbackReceipt and surfaced ReviewRequiredActionCard; Layer 2 post-placement artifact is ExecutorAdmissionDecision), the placement hierarchy (local-first default), nine boundary rules, structured fallback behavior with PlacementFallbackReceipt as an evidence attachment on the parent PlacementDecision, four example domain policies, sixteen-row failure/safety table, operator/steward dashboard rendering, member-shell rendering, and the explicit receipt mapping (none of the placement artifacts is a new ADR-0026 receipt class; they are evidence-artifact identifiers traveling inside existing EffectDispatchEvidence envelopes per docs/spec/effect-dispatch-contract.md Stage 5). Fixes three vocabulary boundaries: scope (LocalDomain not Coop per #1825), execution vs capacity (execution budget is policy-facing; fuel_limit preserved as runtime field; capacity reserved for executor/node availability; resource envelope, allocation, settlement are spec-facing terms), and settlement vs payment (settlement / unit / position / obligation / allocation / receipt — not payment / currency / balance / wallet). Names the first safe proof-loop (read-only placement-decision rehearsal) and dry-run fallback exercise. Preserves legacy code identifiers (FuelLimit, payment_rate, payment_currency, DataLocality::CoopReplicated, icn-coop crate, coop_core paths, coop-scoped comments in icn-rpc) without endorsement; reconciliation tracked as named follow-ups. Advances #1801 — does not by itself close it. Defers wire-stable PlacementDecision schema, ExecutorAdmissionDecision schema, scheduler integration, fuel/payment legacy reconciliation, federation agreement adoption surface, and external custodian policy surface to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14

### 📋 **Draft** [Effect Dispatch Contract](/docs/spec/effect-dispatch-contract.md)

End-to-end behavior contract for turning an accepted governance decision into bounded effects with receipts. Names the five-stage chain (decision recording → mandate minting → effect plan → dispatch → application + evidence) and harmonizes existing types (GovernanceDecisionReceipt, GovernanceProof, GovernanceDecisionAttestation, Mandate, AuthorityGrant, EffectManifest, KernelEffect, EffectOutcome, InstitutionalEffectRecord, EffectDispatchEvidence) with ADRs 0014, 0019, 0025, 0026, 0027, 0029, 0030, 0031. Splits idempotency, partial-failure, challenge/reversal, CCL hook, privacy/redaction, action-card, and package-boundary rules into current contract vs forward schema work. Identifies #1748 process-transition receipts as the first safe runtime dogfood slice. Advances #1797 — does not by itself close it. Defers kernel-side mandate enforcement, federation mandate recognition, EffectRecord taxonomy implementation, stable idempotency-key schema, and other future-direction items to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14

### 📋 **Draft** [Governed Service Binding, Workload Manifest, and Runtime Provider](/docs/spec/governed-service-binding.md)

Defines GovernedServiceBinding (institutional record binding a workload to a domain), WorkloadManifest (declared shape of what a workload does), and RuntimeProvider (substrate-side executor interface) as the integrating envelope for hosted services, installable tools, compute jobs, CCL evaluators, and future container or microVM workloads. Generalizes existing types: ComputeTask (ADR-0030, implemented in icn-compute) is the compute-specific projection of WorkloadManifest; ToolManifest/ToolBinding (RFC-0017) is the tool-install projection; Executor (icn-compute) is the compute-specific RuntimeProvider; EvaluatorBinding (ccl-policy-registry.md) is a specialized GovernedServiceBinding for CCL evaluators. Names seven closed runtime classes (deterministic legitimacy compute, utility computation, container, microVM, accelerator, local device, external bridge), a ten-state lifecycle (declare → authorize → allocate → bind → run → observe → upgrade → suspend → remove → export), the five-stage hosted→governed→adapted→native maturity progression as binding-state requirements, eight boundary rules, and an eleven-row failure/safety table. Advances #1815 — does not by itself close it. Defers wire-stable schema, generic RuntimeProvider Rust trait, per-class provider specs, stage acceptance gates, BackupPolicy data model, and federation-side binding recognition to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14

### 📋 **Draft** [ICN Civic Shell v0](/docs/spec/icn-civic-shell-v0.md)

Defines the ICN Civic Shell as the v0 composition contract that ties together the public website (truth boundary per ADR-0032 and forward-direction ADR-0033), docs/spec/member-shell-v0.md (#1830), docs/spec/steward-cockpit-v0.md (#1831), docs/pilots/no-cli-organizer-member-rehearsal-workflow.md (#1724/#1726), docs/architecture/SERVICE_HOSTING_MODEL.md, docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md, docs/architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md, docs/strategy/SOVEREIGN_FORGE.md, docs/ops/FORGEJO_DEPLOYMENT_PLAN.md, and docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md into a single top-level public-plus-logged-in institutional operating shell. The first draft used the rejected `ICN Headquarters` metaphor; the v0 name is `ICN Civic Shell`. Composition only — the Civic Shell composes existing ICN surfaces and does not supersede the Member Shell, the Steward Cockpit, the public website, the service-hosting model, or the auth-bridge model. Names the public exterior (maturity-banded status / development updates / public roadmap / service-health posture / public forge window / docs and onboarding routes, anchored to ADR-0032 honesty-over-polish and ADR-0033 evidence-link discipline) and the logged-in interior (identity / active domain / active role / authority scope plus member dashboard, action cards, notifications, governance room, workroom, records / receipts room, forge room, operations control room, communications room, vault / privacy posture, settings / identity). Names the domain-and-route doctrine treating intercooperative.network as canonical public identity/truth and icn.zone as short operational/access/discovery without claiming any icn.zone route exists today, and frames ICN-PRIVATE / ICN-EDGE segmentation as current/planned operational context rather than ICN product doctrine. Names a ten-room model (Public Window, Lobby, Member Desk, Governance Room, Workroom, Records Room, Forge Room, Operations Control Room, Communications Room, Vault / Privacy Posture). Reaffirms the auth-bridge rule (OIDC authenticates sessions; ICN authorizes institutional power; receipts prove institutional transitions) and the hosted → governed → ICN-native progression for services. Anchors Civic Shell status labels to the proof-level taxonomy tracked in #1796. Preserves the closed regulatory-safe vocabulary (settlement / position / obligation / allocation / receipt / provenance). Explicit non-goals: no app implementation, no new endpoint, no auth implementation, no Keycloak / Forgejo / Matrix deployment, no n8n workflow build, no DNS / K3s / VLAN / network mutation, no public admin surfaces, no private data in repo, no Phase 2 completion claim, no formal NYCN pilot claim, no live-federation claim, no production-readiness claim, no replacement of existing member-shell or steward-cockpit specs. Advances the Civic Shell composition concept — does not close any sibling issue. Defers the authenticated shell route map, the public status / development updates content set, the member-dashboard / notification model, the forge-room GitHub-adapter / Forgejo-target adapter, the operations-control-room authority model, the project-index proof-level integration, the service-list / service-hosting reconciliation, and the communications-room Matrix / bridge / announcement boundary to named follow-ups (suggested only; not opened by this PR). network-ops was not read locally in this session; operational context is operator-provided summary only and does not make network-ops a public source of ICN truth.

**For:** `architects`, `developers`, `contributors`, `operators` | **Updated:** 2026-05-21

### 📋 **Draft** [InstitutionalDomain and DomainPolicy](/docs/spec/institutional-domain.md)

Defines InstitutionalDomain as the governed operating jurisdiction and DomainPolicy as the persistent rule bundle a domain adopts. Specifies the design-level object outline for both, the boundary lines (domain is not a DNS name, node, federation, package, app, member account, storage bucket, CCL document, or tenant), placement in the civic loop, DomainPolicy evaluation rules (consulted at decision admission, mandate composition, effect plan generation, binding adoption; never in the kernel), the nine-stage domain lifecycle (declare → adopt charter/policy → initialize standing → bind routes/services/tools → operate → amend → federate → suspend/repair → export/exit/archive), and the relationship to the effect dispatch chain (#1797). Harmonizes with COOPERATIVE_DOMAIN_INFRASTRUCTURE.md (design-direction overview), DOMAIN_ROUTING_AND_DNS_BINDINGS.md, INSTITUTION_PACKAGE_BOUNDARY.md, KERNEL_APP_SEPARATION.md, and ADR-0014's constitutional object model. Identifies adjacent named-only concepts (DomainSession, DeviceIdentity/DeviceEnrollment, ServiceIdentity, Workspace, AgreementRegistry, ToolRegistry, DnsBinding) and defers their full specification to named follow-ups. Advances #1794 — does not by itself close it. No runtime, no schema, no wire format.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14

### 📋 **Draft** [Institutional Powers and Legitimacy Invariants](/docs/spec/institutional-powers.md)

Design-level doctrine defining how ICN encodes institutional power so it cannot become unaccountable state/capital power. Names the legitimacy circuit (authority basis -> adopted policy -> bounded effect -> receipt -> challenge/repair path), the InstitutionalPowerEvent envelope, and four power classes (GovernancePower, ContributionPower, ProtectivePower, RepairPower) as the ICN reconciliations of governance, contribution/taxation, protective force, and justice/repair without sovereign extraction, police power, or carceral logic. States fail-closed legitimacy invariants and maps each circuit stage to existing seams (InstitutionalDomain, DomainPolicy, CCL policy registry, AuthorityGrant, Mandate, TypedScope, EffectManifest, KernelEffect, GovernanceDecisionReceipt, InstitutionalEffectRecord, EffectDispatchEvidence, challenge/reversal/counter-receipt). Docs-only design layer over the Effect Dispatch Contract; introduces no code/schema/CCL/route/runtime change. Refs RFC-0018, ADR-0014/0019/0025/0026/0027, #2061, #2080, #1868, #2082.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-06-25

### 📋 **Draft** [Member Shell v0](/docs/spec/member-shell-v0.md)

Defines the ICN member shell as the primary participation surface at v0: mobile-first, offline-tolerant, accessibility-first, plain-language-first. The shell is an app-side rendering surface that consumes ADR-0020 /me/standing, ADR-0027 ActionCard schema, ADR-0026 receipts, the closed seven-string sync vocabulary from docs/spec/network-anti-entropy-proof-loops.md (#1829), and the closed seven-string placement vocabulary from docs/spec/compute-placement-policy.md (#1826), without redefining any of them. Names five hard boundary lines (vs steward cockpit #1795, vs node operator civic-role surface #1613, vs public website, vs institution-package skin, vs backend/runtime), ten design principles, a ten-surface information architecture (Home/Today, My Standing, Current Scope, Action Cards, Decisions/Governance, Receipts, Records/Artifacts, Privacy/Access, Sync/Offline status, Help/Challenge/Review/Exit), the ActionCard rendering contract (per-field requirements + closed card states), the standing surface contract, the ten-step signing/confirmation flow with reversibility/privacy/sync warnings, the three-tier receipt rendering (plain summary → explanation → formal record under details), offline/low-bandwidth behavior including draft-intent vs sent-waiting-for-receipt vs confirmed labeling, privacy and ScopedVault member affordances (existence + scope + access path only, never body content), the twelve-category accessibility gate inherited from ADR-0028 / docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md, a closed v0 member-facing status vocabulary (sync states + execution-scope strings + action-lifecycle strings + privacy/disclosure strings + receipt-class plain-language labels), eighteen-row failure/safety table, and three fixture-first dogfood slices (read-only standing+ActionCard+receipt+sync-delayed; signing flow rehearsal; offline/degraded sync rehearsal). No new endpoints, no platform decision, no native-app implementation, no schema redefinition, no new receipt classes. Advances #1818 — does not by itself close it. Defers the live UI implementation, the platform decision (iOS/Android/PWA/web), the signal_rule and obligation_lifecycle source-path enablement, the multilingual rendering integration, and the Layer 4 ProvenanceQuery consumption to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-15

### 📋 **Draft** [Network Anti-Entropy Proof Loops](/docs/spec/network-anti-entropy-proof-loops.md)

Defines the design-level proof-loop contract beneath compute placement (#1801), storage durability (#1816), artifact registry (#1798), receipt clearing, federation settlement finality (#1365), steward cockpit (#1795), and member shell (#1818). Names the eight-phase anti-entropy institutional evidence loop (schedule/trigger → probe → compare → classify → plan → apply → evidence → surface), thirteen forward-direction proof-artifact identifiers (AntiEntropyProbe, StateDigest, ReceiptDigest, ArtifactDigest, PeerSyncReport, DivergenceEvidence, RepairPlan, RepairReceipt, SyncDegradedStatus, QuorumSyncCheck, FederationSyncWindow, RoutingProof, RedundancyProof), nine state classes covered (governance state, receipts, artifact metadata, scoped vault refs, storage replicas, compute receipts, settlement records, federation membership, CCL policy versions), eighteen divergence classes, ten load-bearing boundary rules (no silent repair of governance-authoritative state; no repair beyond authority; no raw private content in gossip; no widening of locality / disclosure; no degraded-as-healthy rendering; no federation / commons placement without fresh QuorumSyncCheck within FederationSyncWindow; no settlement finality without anti-entropy proof; no member-facing lie; no production claim; no Coop-prefixed generic primitives), privacy and custody rules, steward cockpit surface vocabulary, member shell surface vocabulary (seven plain-language strings), eighteen-row failure/safety table, and three fixture-first proof-loop slices (read-only receipt-index rehearsal, RedundancyProof simulation, QuorumSyncCheck fixture). Anchors against existing primitives in icn-gossip (BloomFilter, VectorClock, PeerSyncManager, PartitionDetector, anti_entropy.rs module) and icn-core (AntiEntropyConfig, spawn_anti_entropy_task) without redefining them. No new ADR-0026 receipt classes introduced; proof artifacts travel inside existing Stage 5 EffectDispatchEvidence or Layer 2 ArtifactReceipt envelopes. Advances #1799 — does not by itself close it. Defers wire-stable schema, devnet fixture implementation, federation-side quorum window protocol, steward / member surface rendering specs, and private-object digest proof contract to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-15

### 📋 **Draft** [Steward Cockpit v0](/docs/spec/steward-cockpit-v0.md)

Defines the ICN steward cockpit as the operator-facing civic-infrastructure surface for node and domain stewards at v0 — the operator/steward complement of docs/spec/member-shell-v0.md (#1830). Consumes verbatim the 9-field cockpit surface from docs/spec/network-anti-entropy-proof-loops.md (#1829) for the Network / Federation section, the 14-field operator/steward dashboard from docs/spec/compute-placement-policy.md (#1826) for the Compute / Commons section, and the storage durability policy objects from docs/spec/storage-durability-policies.md (#1823). Names six hard boundary lines (vs member shell #1830, vs node operator civic-role surface #1613, vs public website, vs institution-package skin, vs backend/runtime, vs surveillance/admin-control panel), ten v0 design principles (stewardship-not-domination, proof-before-confidence, degraded-is-visible, privacy-posture-not-private-content, receipts-explain-state, required-actions-explicit, authority-basis-visible, scope-visible, member-impact-summary-always-present, no-financial-framing), twelve cockpit information-architecture surfaces (Overview/Required Actions, Node Status, Domain Status, Network/Federation, Receipt Store, Storage/Artifacts/ScopedVault, Governance/Process, Compute/Commons, Participation Access, Privacy Posture, Backup/Export/Recovery, Warnings/Incidents/Repair), fourteen operator action-card scenarios with source class / authority pattern / expected outcome, per-surface rendering contracts that consume merged sibling specs without redefining them, a closed v0 operator-facing status vocabulary plus a verbatim member-impact summary mapping into the merged #1829 member-shell sync vocabulary, twenty-row failure/safety table including the load-bearing 'dashboard says healthy while member shell says degraded' v0 violation, and three fixture-first dogfood slices (read-only receipt-store + anti-entropy degraded/repair fixture; storage replica / backup overdue / restore-test receipt fixture; compute placement review-required fixture). No new endpoints, no frontend technology decision, no surveillance console, no private-data preview, no production-dashboard claim. Advances #1795 — does not by itself close it. Defers the live cockpit implementation, the frontend technology decision, the per-surface implementation specs, and the cross-link audit against icn-obs metric module renaming to named follow-ups.

**For:** `architects`, `developers`, `contributors`, `operators` | **Updated:** 2026-05-15

### 📋 **Draft** [Storage Durability Policies — Backup, Replication, Recovery, Archive, Integrity](/docs/spec/storage-durability-policies.md)

Defines six forward-direction policy objects that bind storage classes to durability commitments: StorageSpec (binding between a workload/service/domain and storage), BackupPolicy (frequency, target, integrity, retention), ReplicationPolicy (replicas, placement, anti-entropy), RecoveryPolicy (restore objective class, drill cadence, restore authority), ArchivePolicy (long-term retention, immutability, access), and IntegrityPolicy (verification cadence, repair path). Reuses existing kernel-level types (StorageClass and DataLocality from icn/crates/icn-kernel-api/src/storage.rs) verbatim and acknowledges the drift between the 3-variant kernel enum and the 7-class spine-doc custody taxonomy (canonical store / service state / artifact-blob / volume-block / scoped vault / secret-key / cache-derived) — reconciliation deferred to a named follow-up. Names restore-test receipts as their own concept; mandates that backups, replicas, archives, and exports inherit source locality and disclosure constraints and MUST NOT broaden them. Specifies authority rules tying restore of authoritative state to ADR-0014 mandates and the effect dispatch chain (Stage 5 evidence). Provides a sixteen-row failure/safety table covering missing spec, locality boundary crossing, restore without authority, cache treated as canonical, secret material in ordinary backup, archive quiet deletion, etc. Advances #1816 — does not by itself close it. Defers wire-stable schema, restore-test receipt envelope, locality/privacy inheritance checks, archive verification contract, backup-provider interface, and anti-entropy integration to named follow-ups.

**For:** `architects`, `developers`, `contributors` | **Updated:** 2026-05-14


## Design

### 📝 **Living** [RFC: ICN Commons Evolution](/docs/design/COMMONS_EVOLUTION.md)

Design for evolving ICN commons governance and stewardship models over phases 0-3

**For:** `architects`, `stakeholders` | **Updated:** 2026-03-15

### 📋 **Draft** [ICN Prompt Library](/docs/design/ICN_PROMPT_LIBRARY.md)

Tactical prompt library for doctrine-aligned image generation and visual exploration

**For:** `architects`, `developers`, `designers` | **Updated:** 2026-04-22

### 📋 **Draft** [ICN Visual Explainer Bible](/docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md)

Control-plane doctrine for every ICN visual explainer — source hierarchy, truth labels, vocabulary rules, accessibility floor, core explainer models, brief gate, generated-image workflow, production-source rule. Governs diagrams, infographics, generated images, and source assets across website, docs, decks, and product surfaces.

**For:** `architects`, `developers`, `designers`, `stakeholders` | **Updated:** 2026-05-13

### 📋 **Draft** [ICN Visual System](/docs/design/ICN_VISUAL_SYSTEM.md)

Stable visual doctrine for ICN across website, docs, product surfaces, onboarding, demo materials, and future institutional deployments

**For:** `architects`, `developers`, `stakeholders` | **Updated:** 2026-04-22

### 📝 **Living** [Minimal Viable Coop Track](/docs/design/MINIMAL-VIABLE-COOP.md)

Program for shipping one end-to-end cooperative use case for production 6-month validation

**For:** `product`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [Organizer / member accessibility gate](/docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md)

PR-time review checklist for any organizer- or member-facing surface (rehearsal shells, action-card surfaces, preview/review surfaces, evidence-packet rendering, receipt/provenance review, member-facing governance flows). Twelve review categories with four-value gate outcomes (Pass / Pass with documented follow-ups / Blocked / N/A with reason); copy-paste PR checklist block. Operational layer beneath ADR-0028, design/ACCESSIBILITY_BASELINE.md, and design-language/accessibility.md; companion to architecture/ARCHITECTURE_DUE_DILIGENCE.md participation-access half. Not a CI gate; not a legal accessibility audit; not production-readiness.

**For:** `architects`, `contributors`, `design` | **Updated:** 2026-05-05

### 📋 **Draft** [Access, made-available, and disclosure receipt decision rung — R1-R10](/docs/design/access-made-available-disclosure-receipt-decision-rung.md)

Design-only decision rung (#2330) for the receipt facts after the landed EvidencePacketExportPreparedReceipt (#2326): recommends EvidencePacketMadeAvailableReceipt as the first runtime slice with narrow fingerprint-only semantics, and pins candidate field layouts for made-available, access, disclosure-decision, and redaction-applied receipts. Distinguishes prepared / made-available / accessed / delivered / received / accepted / audited / certified / legally-sufficient, defers authority adjudication to #1868/#2061 via opaque authority-basis references, and implements no runtime, receipt class, route, or member-shell change.

**For:** `architects`, `developers` | **Updated:** 2026-07-05

### 📋 **Draft** [ActivationCrossedReceipt decision rung — B1/B2/B3](/docs/design/activation-crossed-receipt-decision-rung.md)

Narrow decision document resolving the three implementation blockers named in the merged #2294 ActivationCrossedReceipt design contract (#2293, under #1748/#2141), mirroring the decision-recorded-q4-decision.md cadence — decide hash-participating structure in writing before the icn:gov:activation_crossed:v1 tag is pinned. B1 (decision→activation reference): the receipt carries both the caller-opaque decision_id and the content-addressed decision_record_hash of the DecisionRecordedReceipt it activates — the lane's first inter-receipt link — verified fail-closed (the decision must exist in-session), preserving ADR-0026 self-hashed Layer-2 semantics and replay convergence via put_opaque_if_absent. B2 (gate basis): reuse the closed six-variant ProcessGateKind unchanged (no new variant, no ActivationRequest gate object — a variant would be a Copy-enum breaking change and an ADR-controlled taxonomy change); the receipt carries a content-addressed gate_basis fingerprint over the sorted passed ProcessGateResultReceipt record_hashes, non-empty and verified fail-closed (each declared gate exists in-session and is Pass) without owning a required-set policy. B3 (timestamp): a single caller-supplied recorded_at, hashed but excluded from duplicate identity, byte-parallel with the four landed classes; no distinct crossed_at, no decision-carried effective_at (effective_at is membership-lane only); no wall-clock time is a cross-node identity input. Pins a consolidated candidate :v1 field layout, preconditions, and the implementation PR's validation matrix. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no human/AT execution (Refs #2293, #2294, #1748, #2141, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📋 **Draft** [ActivationCrossedReceipt — Design/Audit Contract](/docs/design/activation-crossed-receipt-runtime-dogfood.md)

Design/audit contract for #2293 (under #1748/#2141): the candidate fifth ProcessTransitionReceipt rung, an ActivationCrossedReceipt witnessing that an already-recorded decision crossed the activation boundary (the framing spine's boundary between deciding and doing) with required ProcessGateResultReceipts observed as pass, before any later mutation/evidence work. Audits current state honestly (ActivationCrossedReceipt / ActivationRequest / Mutation* / EvidencePacket* are framing-only — no Rust seam whatsoever; the four landed classes ProcessSessionOpened/DeliberationEntryRecorded/DecisionRecorded/ProcessGateResult are the only runtime ProcessTransitionReceipts), proposes a candidate icn:gov:activation_crossed:v1 contract subject to implementation proof (session-anchored (domain_id, session_id), caller-opaque activation_id, recorder-not-crosser DID, body_hash-only fingerprint, put_opaque_if_absent idempotence with fail-closed conflict and session precondition), places it at ADR-0026 Layer 2 self-hashed (blake3 record_hash, no signature/merkle — naming the layering caveat), preserves the privacy boundary (no private body text), defers member-shell rendering, and names the three blockers that require a narrow decision rung before implementation: B1 decision→activation cross-receipt reference (the lane's first inter-receipt link), B2 gate-basis representation + whether a new ActivationRequest gate object / ProcessGateKind variant is needed (ADR-controlled taxonomy), and B3 caller-supplied crossed_at vs decision-carried effective_at. Recommendation Option C: land this contract, then a decision rung, then implementation. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change (Refs #2293, #1748, #2141, #2041, #2291, #2292; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-03

### 📋 **Draft** [ICN Visual Asset Register](/docs/design/assets/ASSET_REGISTER.md)

Live register of planned and tracked visual assets for ICN. One row per asset (VE-NNN). Indexes briefs in docs/design/assets/briefs/. Initial rows cover the closure loop, scope model, decision-to-receipt, member shell concept, kernel/app separation, federation, commons/compute, action card anatomy, receipt anatomy, steward cockpit, regulatory-safe state, and what ICN is / is not.

**For:** `architects`, `developers`, `designers`, `stakeholders` | **Updated:** 2026-05-13

### 📋 **Draft** [ICN Visual Assets — Directory README](/docs/design/assets/README.md)

Orientation for the visual-asset planning layer — where production assets live, where sketches live, asset lifecycle, and what does not go in this directory.

**For:** `architects`, `developers`, `designers` | **Updated:** 2026-05-13

### 📋 **Draft** [ICN Visual Review Checklist](/docs/design/assets/VISUAL_REVIEW_CHECKLIST.md)

Pre-ship gate for every ICN visual explainer. Source grounding, truth label, vocabulary, accessibility floor, substrate honesty, kernel/app boundary, scope/package boundary, visual grammar, generated-image rules, production-source rule.

**For:** `architects`, `developers`, `designers` | **Updated:** 2026-05-13

### 📝 **Living** [Capability-Based Feature Gating](/docs/design/capability-based-features.md)

System for graceful version handling via capability advertisement and negotiation

**For:** `developers` | **Updated:** 2026-03-15

### 📝 **Living** [Compute Classes: Legitimacy vs Utility](/docs/design/compute-classes.md)

Design distinguishing between legitimacy compute and utility compute subsystems

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Compute Substrate Design](/docs/design/compute-substrate-design.md)

Design for ICN's execution environment and compute resource management

**For:** `architects`, `developers` | **Updated:** 2025-11-18

### 📋 **Draft** [Governed coop_id to EntityId Resolver — Design Seam](/docs/design/coop-id-entity-resolver.md)

Seam definition for the governed coop_id to EntityId resolver named as the keystone in the entity-aware authorization control map: contract, authority/governance, migration posture, fail-closed rules, and sequenced follow-up slices (consumes the #2082 CoopEntityMap store; mirrors the TokenAuthoritySource/DenyUntilWired issuance seam). Design only — no runtime change (#2061, #2080, #1868, #2082)

**For:** `architects`, `developers` | **Updated:** 2026-06-26

### 📋 **Draft** [CreateTreasury — Treasury entity_id Trust Semantics](/docs/design/create-treasury-entity-id-semantics.md)

Trust-semantics audit/design for the CreateTreasury message path (icn-coop actor + apps/membership coop_core duplicate): no production caller, no authority gate, no CoopEntityMap integration, entity_id None today. Pins why the path must never populate entity_id by bare projection or write map provenance, and defines the single safe future slice (read-only trusted-binding consultation mirroring #2266 activation-populate and the ADR-0084 re-verification discipline) plus the tests any implementation PR requires. Mapping stays zero-authority; UnknownLegacy stays untrusted. Design only — no runtime change (#2082; #2081/#2080 untouched)

**For:** `architects`, `developers` | **Updated:** 2026-07-01

### 📋 **Draft** [DecisionRecordedReceipt Q4 decision — recorded fact vs proposal/vote lineage](/docs/design/decision-recorded-q4-decision.md)

Q4 decision document unblocking DecisionRecordedReceipt implementation (#1748/#2141, merged contract #2280): decides all four Q4 branches — (A) v1 stays an opaque body_hash-only recorded-decision fact (no typed DecisionRecord/HumanDecisionSet payload; the brief positions HumanDecisionSet as a read-model); (B) parallel with explicit non-convergence to the load-bearing proposal/vote GovernanceDecisionReceipt lineage icn:gov:decision:v1/v2/v3 (effect dispatch, mandate/authority-grant indexes, action cards) — the spine names but does not absorb it; any future reference is v2-or-later after its own ADR; (C) no deciding-body handle in v1, recorded_by stays recorder-not-decider; (D) resolution stays deferred out of DeliberationEntryKind v1 with discriminant 10 reserved. Hash-layout consequence: none — the merged #2280 contract is implementation-ready as written. Design decision only — no runtime change (Refs #1748, #2141; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-02

### 📋 **Draft** [DecisionRecordedReceipt — Design/Audit Contract](/docs/design/decision-recorded-receipt.md)

Implementation contract for the fourth ProcessTransitionReceipt class (#1748/#2141): a DecisionRecordedReceipt recording that one decision was recorded against an already-opened (domain_id, session_id) anchor, with caller-opaque decision_id, recorded_by as recorder-not-decider actor evidence, body_hash-only content fingerprint (the body is never stored), stable-identity retry idempotency, fail-closed conflict, and atomic per-decision uniqueness via the landed put_opaque_if_absent pattern. Audits current state (no DecisionRecordedReceipt anywhere; disambiguates the load-bearing proposal/vote GovernanceDecisionReceipt lineage icn:gov:decision:v1/v2/v3, which this class must never duplicate or converge with), keeps the receipt free of outcome/tally/vote/mandate semantics, and triages framing-brief Q4 (HumanDecisionSet/DecisionRecord vs proposal-vote boundary) as the explicit implementation blocker — recommendation Option C: a narrow Q4 decision rung before any implementation. Receipts record facts and grant no authority. Design only — no runtime change (Refs #1748, #2141; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-02

### 📋 **Draft** [DeliberationEntry kind taxonomy — Q3 decision](/docs/design/deliberation-entry-kind-taxonomy.md)

Q3 decision document unblocking DeliberationEntryRecordedReceipt implementation (#1748/#2141, merged contract #2277): chooses Option A — a closed, ADR-controlled entry_kind enum hashed by explicit u8 discriminant (the landed gate_kind_ordinal pattern) over charter-extensible strings, keeping institutional vocabulary mapped at the app layer per the framing-brief vocabulary firewall. Pins a scrutinized ten-kind v1 list with explicit discriminants (resolution deferred in writing as Q4-ambiguous), a never-reorder/never-reuse append-only evolution rule with fail-closed unknown kinds and a required golden vector, and the exact v1 hash layout instruction for the future implementation PR. Q1 target_ref stays deferred; no vote/approval/outcome kinds ever by append. Design decision only — no runtime change (Refs #1748, #2141; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-02

### 📋 **Draft** [DeliberationEntryRecordedReceipt — Design/Audit Contract](/docs/design/deliberation-entry-recorded-receipt.md)

Implementation contract for the third ProcessTransitionReceipt class (#1748/#2141): a DeliberationEntryRecordedReceipt recording one deliberation entry as an institutional fact against an already-opened (domain_id, session_id) anchor (#2276), with caller-opaque entry_id, body_hash-only content fingerprint (the body is never stored), stable-identity retry idempotency, fail-closed different-author/body conflict, and atomic per-entry uniqueness via the landed put_opaque_if_absent pattern. Audits current state (no DeliberationEntryRecordedReceipt, no stored DeliberationThread; disambiguates the test-only icn-baseline-lock namesake), keeps entries free of chat/moderation/vote semantics, and triages framing-brief Q3 (entry_kind taxonomy) as the explicit implementation blocker — recommendation Option C: a narrow Q3 taxonomy decision rung before any implementation. Receipts record facts and grant no authority. Design only — no runtime change (Refs #1748, #2141; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-02

### 📝 **Living** [ICN Deterministic Core Specification](/docs/design/deterministic-core.md)

Specification for deterministic computation substrate ensuring reproducible state machines

**For:** `developers`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [ICN Economic Architecture](/docs/design/economics/ECONOMIC_ARCHITECTURE.md)

Design of value flows, contribution accounting, and economic incentives

**For:** `architects`, `product` | **Updated:** 2026-01-17

### 📝 **Living** [Economic Vision](/docs/design/economics/ECONOMIC_VISION.md)

Long-term vision for ICN's economic model and cooperative ownership

**For:** `architects`, `stakeholders` | **Updated:** 2026-03-10

### 📋 **Draft** [Contribution Credits Design](/docs/design/economics/contribution-credits-design.md)

Design for tracking and accounting for contributions to cooperative entities

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Economic Modeling for Mutual Credit](/docs/design/economics/econ-modeling.md)

Simulation and validation of mutual credit economic models

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Economic Safety Design](/docs/design/economics/economic-safety.md)

Safety mechanisms preventing economic attacks and misuse of economic primitives

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Economics Truth Contract](/docs/design/economics/economics-truth-contract.md)

Truth contract auditing all economics-related code against specification

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Economic Model Validation](/docs/design/economics/model-validation.md)

Maps economic operations against implementation state

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Entity-Aware Authorization Control Map](/docs/design/entity-aware-auth-control-map.md)

Migration control map for gateway authorization: flat coop_id guard vs entity-aware checks, observe-mode, fail-closed trusted issuance, and the coop_id to EntityId resolver keystone (#2061, #2080, #1868)

**For:** `architects`, `developers` | **Updated:** 2026-06-26

### 📝 **Living** [Entity Dissolution: Before and After](/docs/design/entity-dissolution-example.md)

Practical example of entity dissolution workflow

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Entity Dissolution Design](/docs/design/entity-dissolution.md)

Design for graceful shutdown and dissolution of cooperative entities

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Evidence export/delivery boundary decision rung — EX1-EX8](/docs/design/evidence-export-delivery-boundary-decision-rung.md)

Narrow decision document resolving the eight open questions the merged #2322 evidence export/delivery boundary contract named in its §9 (#2323, under #1748/#2141, deferring to #1792), mirroring the evidence-packet-produced-receipt-decision-rung.md cadence — decide hash-participating structure in writing before any icn:gov:evidence_packet_export_prepared:v1 tag is pinned. EX1 (fact name/moment): the v1 fact is export-prepared, candidate class EvidencePacketExportPreparedReceipt — the sender-side preparation/staging of an export (packet bound to a recipient scope under an export policy); the name cannot be read as transmission; pinned surface language: prepared, not delivered, received, or accepted. EX2: made-available and delivered are distinct facts (unilateral custody fact vs claimed transmission report) and BOTH are excluded from v1. EX3: a single caller-opaque recipient_scope_id (no scope-definition hash/registry, no recipient DIDs or enumeration; hard rule: never contact data — mirrors EP3's no-id-without-precedent reasoning). EX4: recipient-side facts (received/accepted) stay out of generic ICN — institution/domain-package and bridge territory on recipient authority. EX5: consolidated 11-field candidate layout (domain_id, session_id, export_id key2, packet_id, packet_produced_record_hash verified fail-closed via get_evidence_packet_produced = the lane's fifth inter-receipt link, packet_hash ECHOED AND VERIFIED against the stored produced receipt in the same fetch — the lane's first verified echoed content field, export_policy_hash body-never-stored, recipient_scope_id, prepared_by recorder/export-witness zero-authority, prepared_at node-stamped hashed excluded from identity, record_hash); multiple exports per packet permitted (export_id = uniqueness unit, how many is charter policy); conflict sentinel evidence_packet_export_prepared_conflict. EX6: no custody/vault/location/retrieval semantics in v1 — access is the #1792 AccessReceipt lane. EX7: challenge/rejection/withdrawal fully deferred (append-only doctrine; #1009 dispute pathway). EX8: smallest dogfood = a later fixture-only member-shell rung mirroring #2312/#2320, only after the runtime class lands. Notes the ninth class extends the family beyond the eight named by the idea-0019 framing and must not be presented as completing a #1748 acceptance gate. Design only — no runtime, no receipt class, no tag pinned, no route/OpenAPI/SDK/member-shell/fixture change, no export performed, no delivery/acceptance/audit/legal-sufficiency claim, no human/AT execution (Refs #2323, #2322, #2321, #2318, #2320, #1748, #2141, #1792, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-05

### 📋 **Draft** [Evidence Export and Delivery Boundary — Design/Audit Contract](/docs/design/evidence-export-delivery-boundary.md)

Design/audit contract for the evidence export/delivery boundary that follows the landed EvidencePacketProducedReceipt (#2318 runtime, #2320 member-shell fixture render), under #1748/#2141 and deferring to #1792's disclosure/vault/access-receipt vocabulary. Prevents semantic drift: produced (a redacted evidence packet artifact was produced and content-addressed) must never be misread as exported, made available, delivered, received, accepted, audited, certified, legally sufficient, human/AT verified, or ready. Disambiguates the two repo meanings of export (the shipped read-only rehearsal-evidence-export summary contract vs the future export lifecycle fact). Defines the boundary taxonomy — produced (landed) / export-prepared-or-exported / made-available / delivered-transmitted / received / accepted / audited-certified (out of near-term scope) / challenged-rejected-withdrawn — with, per row: meaning, permitted recorder (recorder/witness evidence, zero authority), minimum proof-pointer references (predecessor id + record_hash chain), private-data exclusions (no packet/policy/source bodies; no recipient contact data — recipient identity is itself sensitive, scope handle or fingerprint only), explicit non-claims, and member/steward surface language. Pins the witness posture: the substrate witnesses authenticated reports; recipient-side facts (received/accepted) require recipient authority and likely belong to institution/domain-package or bridge semantics. Recommends the single next rung: the export fact (aligned with #1792's candidate ExportReceipt), via a decision rung resolving EX1–EX8 (fact name/moment, availability-vs-delivery, recipient-scope representation, where acceptance lives, minimal v1 field set, vault/access interaction, challenge shape, smallest repo-safe dogfood) before any tag or implementation. Design only — no runtime, no receipt class, no route, no OpenAPI/SDK, no member-shell or fixture change, no external delivery performed, no acceptance/audit/legal-sufficiency claim, no human/AT completion (Refs #2321, #2141, #1748, #1792, #2041, #2320, #2318, #2319; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-05

### 📋 **Draft** [EvidencePacketProducedReceipt decision rung — EP1/EP2/EP3/EP4/EP5](/docs/design/evidence-packet-produced-receipt-decision-rung.md)

Narrow decision document resolving the five implementation blockers the merged #2314 EvidencePacketProducedReceipt design/audit contract named in its §14 (#2315, under #1748/#2141), mirroring the mutation-applied-receipt-decision-rung.md cadence — decide hash-participating structure in writing before the icn:gov:evidence_packet_produced:v1 tag is pinned. Heavier than prior rungs because two pinned fields (receipt_set_hash, redaction_profile_hash) have no precedent anywhere in the repo. EP1 (predecessor linkage): v1 carries all three — caller-opaque mutation_application_id, content-addressed mutation_applied_record_hash (the immediate prior boundary, verified fail-closed via get_mutation_applied then record_hash compare, the lane's fourth inter-receipt link), and receipt_set_hash (the lane's first set commitment); the source set is limited to in-session process/evidence receipt references (record_hashes, never bodies), must include the immediate predecessor, is canonically ordered by receipt-ladder position then record_hash bytewise ascending (canonicalized before hashing so caller input order cannot fork identity), export/delivery artifacts excluded from v1; immediate-predecessor verification is the mandatory fail-closed floor, full set-member verification the recommended default. EP2 (packet-hash coverage): packet_hash covers the public/redacted packet artifact only (body never stored), receipt_set_hash covers ordered source references, neither covers private bodies; packet_hash proves neither correctness/completeness/legal-sufficiency nor delivery/acceptance/audit/human-AT/readiness. EP3 (redaction boundary): redaction_profile_hash-only in v1 (no redaction_profile_id — no repo precedent for id-plus-hash; profile body never stored), proving neither completeness nor legal sufficiency nor human/AT completion. EP4 (meaning of produced): produced = an evidence packet artifact was recorded/produced and content-addressed; the receipt is distinct from export/delivery/acceptance/external-audit/action-card triggering and never delivers/certifies/audits; full non-claims pinned. EP5 (human/AT status): excluded from v1 entirely — no human/AT field, no implied pass, automated a11y/production does not complete #2041, which stays open (referenced only as an open dependency). Pins the consolidated candidate v1 field layout (domain_id, session_id, packet_id, mutation_application_id, mutation_applied_record_hash, receipt_set_hash, packet_hash, redaction_profile_hash, produced_by, produced_at, record_hash), record-hash inputs, stable duplicate identity (produced_at + record_hash excluded), node-stamped produced_at semantics, evidence_packet_produced_conflict sentinel, preconditions, and the implementation PR's validation matrix (including a canonical-set-hash order-independence test). Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no fixture change; no human/AT execution (Refs #2315, #2314, #2313, #2312, #2310, #1748, #2141, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📋 **Draft** [EvidencePacketProducedReceipt — Design/Audit Contract](/docs/design/evidence-packet-produced-receipt.md)

Design/audit contract for #2313 (under #1748/#2141): the candidate eighth ProcessTransitionReceipt rung, an EvidencePacketProducedReceipt witnessing that a redacted evidence packet artifact was produced from a set of prior process receipts, recorded after them (typically following a MutationAppliedReceipt, landed #2310) — the terminal 'evidence' stage of the framing spine. Audits current state honestly (EvidencePacketProducedReceipt has no Rust seam — framing/docs only; the seven landed classes ProcessSessionOpened/DeliberationEntryRecorded/DecisionRecorded/ProcessGateResult/ActivationCrossed/MutationPlanRecorded/MutationApplied are the only runtime ProcessTransitionReceipts; the existing rehearsal-evidence-export fixture is a read-only summary, not a receipt; icn-baseline-lock's EvidencePacket is a separate baseline-lock bundle, not this class; and two proposed hash-participating fields — receipt_set_hash and redaction_profile_hash — have NO precedent anywhere in the repo). Proposes a candidate icn:gov:evidence_packet_produced:v1 contract subject to implementation proof (session-anchored (domain_id, session_id), caller-opaque packet_id, applied-step reference by mutation_application_id + content-addressed mutation_applied_record_hash as the immediate prior boundary verified fail-closed, receipt_set_hash committing to the ordered source-receipt set, packet_hash fingerprinting the public/redacted packet only with the packet body never stored, redaction_profile_hash committing to the redaction profile without storing private data, recorder-not-producer DID granting zero authority, node-stamped produced_at hashed but excluded from identity, put_opaque_if_absent idempotence with fail-closed conflict and session precondition), places it at ADR-0026 Layer 2 self-hashed (blake3 record_hash, no signature/merkle — naming the layering caveat), preserves the meaning-firewall + privacy boundary (no packet body, no private source-receipt bodies, no private organizer/member/sponsor/attendee data), defers member-shell rendering, and names blockers for a narrow decision rung before implementation: EP1 predecessor link (both immediate applied ref and receipt_set_hash), EP2 receipt_set_hash definition (NEW concept — membership/ordering/hashing/verification), EP3 packet_hash coverage (public/redacted artifact only), EP4 redaction boundary representation (redaction_profile_hash-only vs id), EP-time produced_at semantics, EP5 the produced-witness boundary vs delivery/acceptance/audit boundary and whether produced is separate from export/summary. States runtime implementation cannot begin while any hash-participating blocker (EP1/EP2/EP3/EP4) is unresolved. Recommendation Option C: land this contract, then a decision rung, then implementation. Explicitly stops at production-recorded — the receipt never produces, delivers, certifies, audits, validates, authorizes, or rolls back a packet; external delivery, acceptance, audit, evidence-packet producers, human/AT completion (#2041 stays open), and live/private data handling are deferred. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no fixture change; no human/AT execution (Refs #2313, #2312, #2310, #2309, #2307, #1748, #2141, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📝 **Living** [Execution Bridge Specification](/docs/design/execution-bridge-spec.md)

Authoritative design for bridging between ICN and external execution environments

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Governance Broad-Fallback Observability and Retirement Evidence](/docs/design/governance-broad-fallback-observability.md)

Design/control map for #2341: after #2340 every known governance mutation surface is class-first with governance:write retained as an accepted-also compatibility fallback, but the repo has no bounded, privacy-safe way to measure which accepted candidate scope callers actually present. Inventories the post-#2340 class-first surface from live main — 51 governance HTTP mutation handlers (charter 6, proposal 8, federation 7 via extract_federation_common, steward 1, comment 5, meeting 12, activity 8, process 4), three gateway aliases (cast_vote_alias, decision-registry create_meeting, index_decision_endpoint), and the governance JSON-RPC mappings (corrects the informal five-method count to seven by including the two #2113 delegation writes) — and confirms no broad-only or narrow-only-without-fallback handlers remain. Specifies a matched-scope outcome model (class/fallback/class_preferred/rejected_sibling/rejected_unrelated/rejected_missing), a bounded signal schema over closed enums (surface_kind/route_family/required_class/match_outcome/observation_outcome), an explicit privacy budget prohibiting token contents, DIDs, entity/actor/subject/domain/resource/proposal/meeting/activity/program/milestone/receipt IDs, payloads, deliberation content, IPs, user agents, free-form labels/errors, and any high-cardinality value, and an absolute observe-only guarantee (observation failure never changes any authorization, handler, route, receipt, mandate, membership, manager, or persistence outcome). Defines the test matrix and the retirement criteria a separate later proposal would need — measured compatibility across every surface, a defined observation window, candidate (unapproved) thresholds, trusted issuance (#2080), entity-aware subject/target authorization (#2061), and a separate enforcement issue/PR with rollback. States explicitly that #2341 does not authorize fallback removal. Docs only: no runtime, observability, scope, handler, route, receipt, token, enforcement, mandate, entity-auth, vault, encryption, provider-import, NYCN, icn-learn, icn-infra, UI, fixture, or readiness/completion change.

**For:** `architects`, `developers`, `security`, `operators` | **Updated:** 2026-07-06

### 📋 **Draft** [Governance Write Authority Decomposition](/docs/design/governance-write-authority-decomposition.md)

Current-state design/control map for #1868 after the original hybrid governance:write decomposition began landing. Enumerates all 51 current governance HTTP handlers still accepting the broad scope, including six newer direct-only paths; confirms the hybrid of bounded class scopes plus app-side mandate/process/entity authority; maps the seven landed class scopes and proposes governance:process:write for four real process-receipt handlers; maps DomainPolicy adoption and InstitutionalDomain declaration to the landed charter class; distinguishes technical capability, entity-aware subject/target authorization, MandateGate authority, and receipt evidence; and explains why #1868/#2061/#2080/#2081 must advance before AccessReceipt runtime. Docs only: no runtime, scope, handler, route, receipt, token, enforcement, vault, encryption, UI, fixture, downstream-repo, or readiness change.

**For:** `architects`, `developers`, `security`, `operators` | **Updated:** 2026-07-05

### 📝 **Living** [ICN Project Governance](/docs/design/governance/PROJECT_GOVERNANCE.md)

Governance structure for ICN development and decision-making

**For:** `team`, `stakeholders` | **Updated:** 2026-03-10

### 📋 **Draft** [Governance Primitives](/docs/design/governance/governance-primitives.md)

Fundamental governance building blocks and decision-making patterns

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Governance Framework](/docs/design/governance/governance.md)

Comprehensive governance framework for cooperative decision-making

**For:** `architects`, `product` | **Updated:** 2026-03-10

### 📝 **Living** [Governance Model Validation](/docs/design/governance/model-validation.md)

Maps governance operations against implementation state

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Trust-Graph Integration for Witness Validation](/docs/design/governance/witness-trust-validation.md)

Trust-graph integration for witness validation in ledger operations

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Institution-in-a-Box](/docs/design/institution-in-a-box.md)

Design pattern for embedding ICN primitives into legacy digital infrastructure with CRDTs and replication

**For:** `architects`, `product` | **Updated:** 2026-03-15

### 📋 **Draft** [IPv6 Endpoint Sets Design](/docs/design/ipv6-endpoint-sets-design.md)

Design for managing multiple network endpoints with IPv6

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Made-Available, Federation Sync, Access, and Repair Boundary Map](/docs/design/made-available-federation-access-boundary-map.md)

Design/control map (#2336) for the boundary after the landed EvidencePacketMadeAvailableReceipt (#2333/#2335). Distinguishes sender/custodian availability from federation digest propagation, routing proof, peer observation, recipient access, delivery, receipt, acceptance, audit, certification, and legal sufficiency. Maps the design-level ArtifactRegistry, ScopedVault, PrivateObjectRef, DisclosurePolicy, future AccessReceipt, and the wire-stable anti-entropy proof artifacts without claiming live emission or enforcement. Decides that AccessReceipt is design-ready only and runtime-blocked by authority-basis, entity-aware authorization, trusted issuance, and vault-enforcement gaps. Recommends #1868/#2061 authority work as the next lane. No runtime, receipt class, route, UI, fixture, vault, encryption, federation rollout, or readiness change.

**For:** `architects`, `developers`, `operators` | **Updated:** 2026-07-05

### 📋 **Draft** [apps/membership coop_core — Map-Parity Contract (#2082 gap 12b)](/docs/design/membership-coop-core-map-parity.md)

Design/audit contract for the last #2082 structural gap: the apps/membership coop_core actor is a test-harness fixture (icn-core dev-dependency; sole consumer is vertical_slice_integration.rs; no production caller) frozen pre-#2104 — no icn-entity dep, no CoopEntityMap integration, no activation binding/populate, no CreateTreasury consultation. Defines the divergence table, the parity-vs-deprecate decision (Option B deprecate/redirect recommended: migrate the vertical-slice test to icn_coop::CoopActor and freeze/remove the duplicate), the exact parity slices and test matrix if parity is chosen instead, and explicit non-claims. Mapping stays zero-authority; UnknownLegacy stays untrusted. Design only — no runtime change (#2082; #2081/#2080 untouched)

**For:** `architects`, `developers` | **Updated:** 2026-07-02

### 📋 **Draft** [Membership durable timestamp semantics — Design/Audit Contract](/docs/design/membership-durable-timestamp-semantics.md)

Design/audit contract for #2286 after #2284 made membership state_change_hash a deterministic decision-identity fingerprint: durable Member records still persist node-local wall-clock (joined_at field; removed_at/frozen_at/freeze_expires_at/unfrozen_at metadata), so replaying the same governance decision on two nodes diverges in durable bytes. Decides the target semantics — deterministic durable timestamps via a decision-carried effective_at threaded through MembershipEffect/requests (mirroring the KernelProtocolExecutor SetParameter precedent, with the honest caveat that the protocol producer's value is currently degenerate 0 pending #282), local audit timestamps separated from durable convergence state, freeze_expires_at = effective_at + duration_secs, update-member needs no durable timestamp, membership:v2 hash layout unchanged. Names the serialized-effect compatibility choice (fail-closed vs versioned vs legacy-non-convergent) as an explicit future implementation decision — no serde(default) smuggling — plus the implementation PR's test obligations. Design only — no schema/runtime/OpenAPI/SDK change (Refs #2286, #2284, #2283; #2286 stays open)

**For:** `architects`, `developers` | **Updated:** 2026-07-03

### 📋 **Draft** [Multi-Device Identity Design](/docs/design/multi-device-identity-design.md)

Design for managing identity across multiple devices within a single agent

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [MutationAppliedReceipt decision rung — A1/A2/A3/A4](/docs/design/mutation-applied-receipt-decision-rung.md)

Narrow decision document resolving the four implementation blockers named in the merged #2307 MutationAppliedReceipt design/audit contract (#2308, under #1748/#2141), mirroring the mutation-plan-recorded-receipt-decision-rung.md cadence — decide hash-participating structure in writing before the icn:gov:mutation_applied:v1 tag is pinned. A1 (plan→application reference): the receipt carries both the caller-opaque plan_id and the content-addressed plan_record_hash of the MutationPlanRecordedReceipt it applies — the lane's third inter-receipt link — verified fail-closed (get_mutation_plan_recorded then record_hash compare; the referenced plan must exist in-session with a matching plan_id), with activation + decision + gate basis inherited transitively through the plan→activation chain rather than re-referenced in v1. A2 (result representation): result_hash-only v1 (a caller-supplied 32-byte fingerprint of the application-result record, distinct in name from the plan's body_hash; the applied-result body — operation list, target list, effect payload, or any typed operation/result/effect model — is never stored, preserving the meaning firewall and privacy), no application-kind taxonomy. A3 (timestamp): a single caller-supplied applied_at, hashed but excluded from stable duplicate identity, byte-parallel with the six landed classes' recorded_at; no distinct executed_at/effective_at; no wall-clock in cross-node identity. A4 (applied-witness boundary): applied means an application fact was recorded, not executed/authorized/validated/enforced/rolled-back/proven-correct; applied_by is recorder/apply-witness evidence granting zero authority; a caller-supplied result_hash is sufficient for v1; verifiable-effect binding, rollback semantics, typed result models, and evidence-packet production are deferred. Pins a consolidated candidate :v1 field layout (domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash, applied_at, record_hash), preconditions, and the implementation PR's validation matrix. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no human/AT execution (Refs #2308, #2306, #2307, #2305, #2303, #1748, #2141, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📋 **Draft** [MutationAppliedReceipt — Design/Audit Contract](/docs/design/mutation-applied-receipt.md)

Design/audit contract for #2306 (under #1748/#2141): the candidate seventh ProcessTransitionReceipt rung, a MutationAppliedReceipt witnessing that a previously recorded mutation plan (MutationPlanRecordedReceipt, landed #2303) was applied, recorded after the plan. Audits current state honestly (MutationAppliedReceipt has no Rust seam — framing/docs only; the six landed classes ProcessSessionOpened/DeliberationEntryRecorded/DecisionRecorded/ProcessGateResult/ActivationCrossed/MutationPlanRecorded are the only runtime ProcessTransitionReceipts; icn-baseline-lock's EvidencePacket is a separate baseline-lock bundle, not this class). Proposes a candidate icn:gov:mutation_applied:v1 contract subject to implementation proof (session-anchored (domain_id, session_id), caller-opaque application_id, plan reference by plan_id + content-addressed plan_record_hash as the lane's third inter-receipt link verified fail-closed via get_mutation_plan_recorded, recorder-not-applier DID granting zero authority, result_hash-only fingerprint with the applied-result body never stored, applied_at hashed but excluded from identity, put_opaque_if_absent idempotence with fail-closed conflict and session precondition), places it at ADR-0026 Layer 2 self-hashed (blake3 record_hash, no signature/merkle — naming the layering caveat), preserves the meaning-firewall + privacy boundary (no kernel-readable operation/result model, no applied-result body text), defers member-shell rendering, and names blockers for a narrow decision rung before implementation: A1 plan→application reference posture, A2 application body/result representation, A3 applied_at timestamp semantics, A4 the applied-witness boundary vs execution/authority boundary. States runtime implementation cannot begin while any hash-participating blocker (A1/A2/A4) is unresolved. Recommendation Option C: land this contract, then a decision rung, then implementation. Explicitly stops at application-recorded — the receipt never executes, validates, authorizes, or rolls back a mutation; EvidencePacketProducedReceipt, action-card triggers, and any typed/kernel-readable result model are deferred. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no human/AT execution (Refs #2306, #2305, #2303, #2302, #2300, #1748, #2141, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📋 **Draft** [MutationPlanRecordedReceipt decision rung — M1/M2/M3](/docs/design/mutation-plan-recorded-receipt-decision-rung.md)

Narrow decision document resolving the three implementation blockers named in the merged #2300 MutationPlanRecordedReceipt design/audit contract (#2301, under #1748/#2141), mirroring the activation-crossed-receipt-decision-rung.md cadence — decide hash-participating structure in writing before the icn:gov:mutation_plan_recorded:v1 tag is pinned. M1 (plan→activation reference): the receipt carries both the caller-opaque activation_id and the content-addressed activation_record_hash of the ActivationCrossedReceipt it follows — the lane's second inter-receipt link — verified fail-closed (get_activation_crossed then record_hash compare; the referenced activation must exist in-session with a matching activation_id), with decision + gate basis inherited transitively through the activation rather than re-referenced in v1. M2 (plan-body representation): body_hash-only v1 (a caller-supplied 32-byte fingerprint; the MutationPlan body — operation list, target list, effect payload, or any typed operation model — is never stored, preserving the meaning firewall and privacy), no plan-kind taxonomy. M3 (timestamp): a single caller-supplied recorded_at, hashed but excluded from duplicate identity, byte-parallel with the five landed classes; no distinct planned_at; no wall-clock in cross-node identity. Pins a consolidated candidate :v1 field layout, preconditions, and the implementation PR's validation matrix. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no human/AT execution (Refs #2301, #2299, #2300, #1748, #2141, #2041, #2296, #2298; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📋 **Draft** [MutationPlanRecordedReceipt — Design/Audit Contract](/docs/design/mutation-plan-recorded-receipt.md)

Design/audit contract for #2299 (under #1748/#2141): the candidate sixth ProcessTransitionReceipt rung, a MutationPlanRecordedReceipt witnessing that a mutation plan (the framing spine's plan-of-record for what runtime should do as a consequence of an activation) was recorded after an ActivationCrossedReceipt, before any mutation is applied. Audits current state honestly (MutationPlanRecordedReceipt / MutationPlan / MutationAppliedReceipt / EvidencePacketProducedReceipt — and even the framing's proposed read-model PreviewReviewPacket / pending_publish_summary — are framing-only with no Rust seam; the five landed classes ProcessSessionOpened/DeliberationEntryRecorded/DecisionRecorded/ProcessGateResult/ActivationCrossed are the only runtime ProcessTransitionReceipts; icn-baseline-lock's EvidencePacket is a separate baseline-lock bundle, not this class). Proposes a candidate icn:gov:mutation_plan_recorded:v1 contract subject to implementation proof (session-anchored (domain_id, session_id), caller-opaque plan_id, activation reference by activation_id + content-addressed activation_record_hash as the lane's second inter-receipt link verified fail-closed, recorder-not-planner DID granting zero authority, body_hash-only fingerprint with the plan body never stored, put_opaque_if_absent idempotence with fail-closed conflict and session precondition), places it at ADR-0026 Layer 2 self-hashed (blake3 record_hash, no signature/merkle — naming the layering caveat), preserves the meaning-firewall + privacy boundary (no kernel-readable operation list, no plan body text), defers member-shell rendering, and names three blockers for a narrow decision rung before implementation: M1 plan→activation reference posture, M2 body_hash-only vs typed operation model + plan-kind taxonomy, M3 timestamp source. Recommendation Option C: land this contract, then a decision rung, then implementation. Explicitly stops at plan-recorded — MutationAppliedReceipt, EvidencePacketProducedReceipt, action-card triggers, and any typed/kernel-readable plan model are deferred. Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change; no member-shell change; no human/AT execution (Refs #2299, #1748, #2141, #2041, #2296, #2298; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-04

### 📋 **Draft** [NAT Traversal Design](/docs/design/nat-traversal-design.md)

Design for peer-to-peer connectivity across NAT boundaries

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Organizer-steward evidence surface runtime dogfood — Design Contract](/docs/design/organizer-steward-evidence-surface-runtime-dogfood.md)

Implementation-planning contract for #2289 (under #1748/#2141): turns the scoped human-operability slice into an implementation-ready contract for the receipt → surface → evidence/export tail of the vertical spine. Names the receipt path (the four already-landed ADR-0026 Layer 2 classes ProcessSessionOpenedReceipt → DeliberationEntryRecordedReceipt → DecisionRecordedReceipt → ProcessGateResultReceipt, no new class), the human surface (existing web/member-shell/ demo/live surface — plain-language summary + evidence-detail disclosure + fixture/dry-run/live boundary labeling, non-CLI), the evidence/export shape (a repo-safe contract-conformant fixture evidence summary mapped onto urn:icn:contract:rehearsal-evidence-export:v1; no new runtime EvidencePacket producer), a fixture-safe privacy/redaction model (one DeliberationEntry visible to the steward body, redacted from the member/export view, showing redaction reason + record_hash/body_hash proof pointer without leaking private text — honest because deliberation/decision receipts store body_hash only), and the accessibility-gate obligations (ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md §3.11 receipts/provenance/evidence access + §3.12 governance/action access, with #2041 human/AT categories kept visible-pending). Design only — no Rust/UI/schema/OpenAPI/SDK/receipt-class change (Refs #2289, #1748, #2141, #2041; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-03

### 📋 **Draft** [Platform Layer Design](/docs/design/platform-layer-design.md)

Design for platform abstractions and portability across systems

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Post-Quantum Cryptography in ICN](/docs/design/post-quantum-crypto.md)

Experimental post-quantum cryptography integration and migration strategy

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📋 **Draft** [ProcessSession Receipt Anchor — Design/Audit Contract](/docs/design/process-session-receipt-anchor.md)

Implementation contract for the second ProcessTransitionReceipt class (#1748/#2141): a ProcessSessionOpenedReceipt anchoring caller-opaque session_ids to a recorded opening fact (domain-bound blake3 hash, ADR-0026 Layer 2, mirrors the landed #2144 ProcessGateResultReceipt pattern end to end). Audits current state (session_id opaque, no stored ProcessSession, no lifecycle), pins duplicate-open idempotency/conflict semantics, defers target_ref (Q1) and purpose taxonomy in writing, and recommends receipt-only anchoring (no stored session object) with the required test matrix. Receipts record facts and grant no authority. Design only — no runtime change (Refs #1748, #2141; no closure claims)

**For:** `architects`, `developers` | **Updated:** 2026-07-02

### 📋 **Draft** [Razeto Integration Design](/docs/design/razeto-integration-design.md)

Design for integrating external systems via Razeto protocol

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Regulatory-Safe Verifiable State](/docs/design/regulatory-safe-verifiable-state.md)

Design maintaining provable state satisfying regulatory audits without exposing ledger internals

**For:** `architects`, `compliance` | **Updated:** 2026-03-10

### 📋 **Draft** [Repository Reality Map](/docs/design/repo-reality-map.md)

Mapping between git repository structure and architectural reality

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Scheduler Evolution Plan](/docs/design/scheduler-evolution-plan.md)

Plan for evolving ICN's scheduling substrate over multiple phases

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Scope Scheduling Design](/docs/design/scope-scheduling.md)

Design for scheduling and resource allocation within organizational scopes

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Social Recovery Guide](/docs/design/sdis/social-recovery.md)

M-of-N social recovery mechanism for identity recovery

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📋 **Draft** [Social Recovery Design](/docs/design/social-recovery-design.md)

M-of-N social recovery mechanism for lost devices and identity recovery

**For:** `architects`, `security` | **Updated:** 2026-03-10

### 📋 **Draft** [Gossip SignedEnvelope Migration](/docs/development/gossip-signed-envelope-migration.md)

Migration plan for gossip cryptographic authentication

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [IdentityBundle Refactor Plan](/docs/development/identity-bundle-refactor-plan.md)

Hardware-backed signing implementation plan

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module Splitting Analysis](/docs/development/module-splitting-analysis.md)

Analysis of large modules for potential splitting

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Mobile Member UX Spec v1](/docs/mobile/icn-mobile-ux-spec-v1.md)

Mobile application UX specification

**For:** `developers`, `product` | **Updated:** 2026-03-10

### 📋 **Draft** [SDIS: Secure Distributed Identity System](/docs/sdis/SDIS_SYSTEM.md)

SDIS design with implemented flows and planned endpoints

**For:** `architects`, `developers` | **Updated:** 2026-03-10


## Guide

### 🔒 **Canonical** [Contributing to ICN](/CONTRIBUTING.md)

Architectural guardrails, contribution workflow, code standards, and review process

**For:** `contributors` | **Updated:** 2026-03-01

### 🔒 **Canonical** [Getting Started with ICN](/docs/GETTING_STARTED.md)

Quick-start guide for new developers to set up ICN in minutes

**For:** `developers`, `contributors` | **Updated:** 2026-03-15

### 📝 **Living** [Deploy Test Network](/docs/deployment/DEPLOY_TEST_NETWORK.md)

Guide for setting up multi-node test networks for validation

**For:** `developers`, `testers` | **Updated:** 2026-03-10

### 📝 **Living** [Deploy to K3s](/docs/deployment/DEPLOY_TO_K3S.md)

Steps for deploying ICN to Kubernetes using K3s

**For:** `operators`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Quick Deploy Guide](/docs/deployment/QUICK_START.md)

Fast path for getting ICN running locally

**For:** `developers`, `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [ICN UX Language Guide](/docs/dev/language-guide.md)

Enforced communications style guide for regulatory-safe messaging

**For:** `all` | **Updated:** 2026-03-10

### 📝 **Living** [Federation Roadmap Implementation Guide](/docs/development/federation-roadmap-implementation.md)

Step-by-step federation feature implementation guide

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Trust Multi-Graph Migration Guide](/docs/development/trust-multi-graph-migration.md)

Migration guide for multi-graph architecture

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Witness Signature Best Practices](/docs/features/witness-signature-best-practices.md)

Developer guide for witness signature implementation

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Development Environment](/docs/guides/developer/DEV_ENVIRONMENT.md)

Setup guide for icn-dev development VM

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Documentation Style Guide](/docs/guides/developer/DOCUMENTATION_STYLE.md)

Consistent documentation formatting standards

**For:** `contributors` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Developer Guides](/docs/guides/developer/README.md)

Index of guides for developers building on or contributing to ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Internationalization (i18n) Guide](/docs/guides/developer/i18n-guide.md)

i18n implementation across Rust, React Native, web

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Operations Guides](/docs/guides/operations/README.md)

Operational guides for running ICN deployments

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Backup and Recovery Guide](/docs/guides/operations/backup-and-recovery.md)

Operator procedures for node backup and recovery

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Backup and Restore: Operator Recovery](/docs/guides/operations/backup-restore.md)

Low-level Sled database and keystore recovery

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Daemon Mode with Governance Receipts](/docs/guides/operations/daemon-mode-governance.md)

Phase 0 pilot daemon mode operations

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [NAT Traversal Pilot Test Guide](/docs/guides/operations/nat-traversal-pilot-test.md)

Manual testing guide for NAT traversal feature

**For:** `operators`, `testers` | **Updated:** 2026-03-10

### 📝 **Living** [NAT Traversal Operations Guide](/docs/guides/operations/nat-traversal.md)

Configuring and operating nodes behind NAT

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Operations Guide](/docs/guides/operations/operations-guide.md)

Comprehensive operational procedures and workflows

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Smoke Runbook](/docs/guides/operations/pilot-smoke.md)

Deterministic operator check for pilot deployment

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Replication Operations Guide](/docs/guides/operations/replication-operations.md)

Replication procedures for Phase 17 storage hardening

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Troubleshooting Runbooks](/docs/guides/operations/troubleshooting.md)

Step-by-step procedures for diagnosing operational issues

**For:** `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [User Guides](/docs/guides/user/README.md)

End-user documentation and tutorials

**For:** `users` | **Updated:** 2026-03-10

### 📝 **Living** [Cooperative Setup Guide](/docs/guides/user/cooperative-setup-guide.md)

Instructions for setting up a new cooperative on ICN

**For:** `users` | **Updated:** 2026-03-10

### 📝 **Living** [Summit Decision Registry Demo](/docs/guides/user/summit-demo.md)

Demo script for decision registry and treasury spending

**For:** `team`, `demo-users` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Coordinator Guide](/docs/internal/pilots/pilot-coordinator-guide.md)

Practical guide for cooperative coordinators deploying ICN

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Deployment Playbook](/docs/internal/pilots/pilot-playbook.md)

Step-by-step guide for pilot cooperative deployment

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Keystore Version Migration](/docs/migration-guides/keystore-versions.md)

Guide for keystore format version migration

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Version Upgrade Guide](/docs/migration-guides/version-upgrades.md)

Guide for upgrading between ICN versions

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Tail-Based Sampling Configuration](/docs/observability/tail-based-sampling.md)

Tail-based sampling setup for traces

**For:** `operators` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Developer Onboarding Curriculum](/docs/onboarding/README.md)

Structured learning path for new ICN developers

**For:** `developers`, `contributors` | **Updated:** 2026-03-15

### 📝 **Living** [Onboarding Assessments](/docs/onboarding/assessments.md)

Module checkpoints and self-review questions

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Capstone: Local Two-Node ICN + Ledger Flow](/docs/onboarding/capstone.md)

Final capstone project demonstrating end-to-end ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 01: Workspace Setup](/docs/onboarding/labs/lab-01-workspace/README.md)

Learning lab for workspace organization

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 02: Error Receipts and Tracing](/docs/onboarding/labs/lab-02-error-receipt/README.md)

Learning lab for structured errors and tracing

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 03: Mini Actor Runtime](/docs/onboarding/labs/lab-03-mini-actor/README.md)

Learning lab for actor model implementation

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 04: Firewall Oracle (Keystone Lab)](/docs/onboarding/labs/lab-04-firewall-oracle/README.md)

Keystone lab proving the Meaning Firewall boundary

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 05: Mini Ledger with Double-Entry](/docs/onboarding/labs/lab-05-mini-ledger/README.md)

Learning lab for ledger implementation

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 06: Signed Envelopes with Replay Protection](/docs/onboarding/labs/lab-06-signed-envelope/README.md)

Learning lab for cryptographic signatures

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 07: Gossip Sync with Vector Clocks](/docs/onboarding/labs/lab-07-gossip-sync/README.md)

Learning lab for eventual consistency

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Lab 08: Governance Flow](/docs/onboarding/labs/lab-08-governance-flow/README.md)

Learning lab for governance proposal to constraint update

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [01: Environment Setup & Repository Navigation](/docs/onboarding/path/phase-1-foundations/01-environment.md)

Foundational module on environment and repo layout

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [02: Rust Through ICN's Lens](/docs/onboarding/path/phase-1-foundations/02-rust-through-icn.md)

Foundational module on Rust patterns used in ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [03: Errors and Tracing](/docs/onboarding/path/phase-1-foundations/03-errors-and-tracing.md)

Foundational module on error handling and distributed visibility

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Actors and Concurrency](/docs/onboarding/path/phase-2-architecture/04-actors-and-concurrency.md)

Learning module on ICN's actor model and concurrent execution

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [The Meaning Firewall](/docs/onboarding/path/phase-2-architecture/05-the-meaning-firewall.md)

Learning module on ICN's type-safe boundary enforcement

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Persistence and Ledger](/docs/onboarding/path/phase-2-architecture/06-persistence-and-ledger.md)

Learning module on state persistence and event logging

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [07: Identity and Cryptography](/docs/onboarding/path/phase-3-systems/07-identity-and-crypto.md)

Systems module on identity and cryptographic operations

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [08: Network and Gossip](/docs/onboarding/path/phase-3-systems/08-network-and-gossip.md)

Systems module on network communication and eventual consistency

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [09: Governance and Contracts](/docs/onboarding/path/phase-3-systems/09-governance-and-contracts.md)

Systems module on governance and contracts

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [10: Federation and Operations](/docs/onboarding/path/phase-4-ownership/10-federation-and-ops.md)

Ownership module on federation and production deployment

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [11: Maintainer Skills](/docs/onboarding/path/phase-4-ownership/11-maintainer.md)

Ownership module on reshaping architecture safely

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Maintainer Capstone](/docs/onboarding/path/phase-4-ownership/capstone.md)

Final capstone gate for Maintainer tier

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Onboarding Reading Map](/docs/onboarding/reading-map.md)

Links between modules and high-signal source files

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 0: Setup and Tooling](/docs/onboarding/reference/module-00-setup.md)

Reference module on development environment setup

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 1: Rust Fundamentals](/docs/onboarding/reference/module-01-rust-fundamentals.md)

Reference module on Rust concepts for ICN

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 02: Architecture Overview](/docs/onboarding/reference/module-02-architecture-overview.md)

Reference module providing high-level architecture overview

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 3: Runtime and Actor Model](/docs/onboarding/reference/module-03-runtime-actors.md)

Reference module on actor model and constraint engine

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 4: Identity and Trust](/docs/onboarding/reference/module-04-identity-trust.md)

Reference module on identity and trust primitives

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 5: Network and Gossip](/docs/onboarding/reference/module-05-network-gossip.md)

Reference module on network communication and state sync

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 6: Ledger and Contracts](/docs/onboarding/reference/module-06-ledger-contracts.md)

Reference module on ledger and CCL programming

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 8: Web UI Integration](/docs/onboarding/reference/module-08-web-ui.md)

Reference module on web UI integration

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 9: Operations and Deployment](/docs/onboarding/reference/module-09-ops-deploy.md)

Reference module on production deployment

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 10: Contributor Workflow](/docs/onboarding/reference/module-10-contributor-workflow.md)

Reference module on contribution processes

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 11: Federation](/docs/onboarding/reference/module-11-federation.md)

Reference module on federation and inter-cooperative agreements

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 12: Observability and Metrics](/docs/onboarding/reference/module-12-observability.md)

Reference module on monitoring

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Module 13: Security and Privacy](/docs/onboarding/reference/module-13-security-privacy.md)

Reference module on security and privacy in ICN

**For:** `developers`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Module 14: Governance and CCL Deep Dive](/docs/onboarding/reference/module-14-governance-ccl-deep-dive.md)

Reference module on governance and contract language

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Curriculum Gap Analysis](/docs/onboarding/review-plan.md)

Analysis of curriculum gaps and iteration plan

**For:** `team` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Onboarding Syllabus](/docs/onboarding/syllabus.md)

Overall course structure for Foundations and Accelerated tracks

**For:** `developers` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Accelerated Track (4 weeks)](/docs/onboarding/tracks/accelerated.md)

Fast-track onboarding for Rust-intermediate developers

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [ICN Contributor Ladder](/docs/onboarding/tracks/contributor-ladder.md)

Tier-based contributor skill progression (Observer → Contributor → Maintainer → Architect)

**For:** `contributors` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Foundations Track (8 weeks)](/docs/onboarding/tracks/foundations.md)

Comprehensive onboarding track for Rust beginners

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Curriculum Update Process](/docs/onboarding/update-process.md)

Process for keeping onboarding aligned with codebase

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 0: Local Build and Repo Orientation](/docs/onboarding/workshops/workshop-00-setup.md)

Workshop on local build and repository structure

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 1: Rust Fundamentals in Practice](/docs/onboarding/workshops/workshop-01-rust-fundamentals.md)

Workshop applying Rust patterns to ICN code

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 2: Architecture Mapping Exercise](/docs/onboarding/workshops/workshop-02-architecture.md)

Workshop for building architecture mental models

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 3: Runtime and Actor Lifecycle](/docs/onboarding/workshops/workshop-03-runtime.md)

Workshop on actor model and lifecycle

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 4: Identity and Trust Hands-On](/docs/onboarding/workshops/workshop-04-identity-trust.md)

Workshop on identity and trust operations

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 5: Network and Gossip Deep Dive](/docs/onboarding/workshops/workshop-05-network-gossip.md)

Workshop on network and gossip systems

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 6: Ledger and Contract Flow](/docs/onboarding/workshops/workshop-06-ledger-contracts.md)

Workshop on ledger and contract operations

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 8: Web UI Exploration](/docs/onboarding/workshops/workshop-08-web-ui.md)

Workshop on web UI and gateway integration

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 9: Local Deployment and Observability](/docs/onboarding/workshops/workshop-09-ops.md)

Workshop on deployment and monitoring

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 10: Contributor Workflow](/docs/onboarding/workshops/workshop-10-contributor.md)

Workshop on contribution workflow

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 11: Federation Hands-On](/docs/onboarding/workshops/workshop-11-federation.md)

Workshop on federation code paths

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 12: Observability and Metrics](/docs/onboarding/workshops/workshop-12-observability.md)

Workshop on observability

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 13: Security and Privacy](/docs/onboarding/workshops/workshop-13-security-privacy.md)

Workshop on security and privacy layers

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Workshop 14: Governance and CCL Deep Dive](/docs/onboarding/workshops/workshop-14-governance-ccl.md)

Workshop on governance and contract language

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Ops runbooks relocated (stub)](/docs/ops/runbooks/README.md)

Runbooks moved to docs/guides/operations/runbooks/ (tranche 3); use that path for updates

**For:** `operators` | **Updated:** 2026-03-26

### 📝 **Living** [Organizer facilitator walkthrough human AT test plan and evidence template](/docs/pilots/organizer-facilitator-walkthrough-human-at-test-plan.md)

Reusable, deliberately blank human assistive-technology smoke-test plan and repo-safe evidence template for the fixture-backed organizer facilitator walkthrough. Records the required tester, device, OS, browser, AT, input, zoom, theme, locale, task, observation, blocker, and follow-up fields; distinguishes automated/browser-assisted evidence from real human observations; and makes no claim that human AT testing or accessibility completion has occurred.

**For:** `team`, `organizers`, `contributors`, `accessibility-testers` | **Updated:** 2026-06-29

### 📋 **Draft** [Pilot Proposal Template](/docs/pilots/pilot-proposal-template.md)

Template for approaching potential pilot communities

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [SDIS User Guide](/docs/sdis/SDIS_USER_GUIDE.md)

End-user guide for credential presentation system

**For:** `users` | **Updated:** 2026-03-10

### 📝 **Living** [Mobile App Testing Guide](/docs/testing/MOBILE_APP_TESTING_GUIDE.md)

Testing guide for mobile application

**For:** `testers` | **Updated:** 2026-03-10


## Operations

### 📝 **Living** [ICN Deployment Guide](/docs/deployment/DEPLOYMENT_GUIDE.md)

Comprehensive guide for deploying ICN to production environments

**For:** `operators`, `developers` | **Updated:** 2026-03-15

### 📝 **Living** [Code Quality Improvement Tracker](/docs/development/code-quality-improvements.md)

Tracks error handling and code quality audits

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [Network Policy Design — K3s](/docs/guides/operations/network-policies.md)

K3s network policy design (deferred)

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Phase 0 Operational Monitoring](/docs/guides/operations/phase-0-monitoring.md)

Monitoring configuration for Phase 0 pilot

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Emergency Node Restart](/docs/guides/operations/runbooks/01-emergency-restart.md)

Runbook for emergency node restart procedures

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Data Recovery Procedure](/docs/guides/operations/runbooks/02-data-recovery.md)

Runbook for node data recovery from backup

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Version Upgrade Procedure](/docs/guides/operations/runbooks/03-version-upgrade.md)

Runbook for upgrading daemon to new version

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Security Incident Response](/docs/guides/operations/runbooks/04-security-incident.md)

Runbook for security incident response

**For:** `operators`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Troubleshooting Guide](/docs/guides/operations/runbooks/05-troubleshooting.md)

Common issues and solutions for node operations

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Secrets Rotation Procedure](/docs/guides/operations/runbooks/06-secrets-rotation.md)

Runbook for rotating cryptographic secrets

**For:** `operators`, `security` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Vertical Slice Smoke](/docs/guides/operations/runbooks/07-pilot-vertical-slice-smoke.md)

Runbook for pilot deployment verification

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Treasury Entity-Auth Enforce-Mode Runbook](/docs/guides/operations/runbooks/treasury-entity-auth-enforce-mode-runbook.md)

Rehearse/verify the off-by-default treasury entity-auth enforce mode (ICN_TREASURY_ENTITY_AUTH_MODE=enforce-trusted-resolver) before any real enablement

**For:** `operators` | **Updated:** 2026-06-29

### 🔒 **Canonical** [Operations Directory README](/docs/operations/README.md)

Navigation for operations and deployment documentation

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [Operations Deployment Guide](/docs/operations/deployment/deployment-guide.md)

Deep operational guide for production deployments

**For:** `operators` | **Updated:** 2026-03-15

### 📋 **Draft** [Distributed Tracing Setup](/docs/operations/deployment/distributed-tracing.md)

Configuration for distributed tracing and observability

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Incident Response Plan](/docs/operations/deployment/incident-response.md)

Procedures for responding to production incidents

**For:** `operators` | **Updated:** 2026-03-10


## Reference

### 📝 **Living** [Agent Registry](/AGENTS.md)

Catalog of authorized AI agents and their capabilities for ICN development

**For:** `agents`, `team` | **Updated:** 2026-03-01

### 📝 **Living** [Changelog](/CHANGELOG.md)

Release notes and version history

**For:** `contributors`, `public` | **Updated:** 2026-03-21

### 🔒 **Canonical** [Claude Agent Onboarding](/CLAUDE.md)

Guidance for Claude Code sessions working with ICN codebase

**For:** `agents`, `developers` | **Updated:** 2026-06-17

### 🔒 **Canonical** [Code of Conduct](/CODE_OF_CONDUCT.md)

Community guidelines and expected behavior for contributors

**For:** `contributors`, `community` | **Updated:** 2025-12-01

### 🔒 **Canonical** [ICN - InterCooperative Network](/README.md)

Main project README with overview, quick start, and CI/CD status badge

**For:** `contributors`, `public` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Ecosystem Atlas](/docs/ATLAS.md)

Top-level front door composing the project-index map family and the truth spine; cross-repo map (incl. private ops/provider repos), boundary/claims guardrail, and agent preflight. Index, not a source of truth.

**For:** `all`, `agents` | **Updated:** 2026-06-16

### 🔒 **Canonical** [ICN Documentation Control System](/docs/DOCUMENTATION_CONTROL_SYSTEM.md)

Normative development control plane: discovery vs delivery, artifact routing, and documentation governance

**For:** `contributors`, `agents` | **Updated:** 2026-03-26

### 📝 **Living** [ICN Document Registry (human summary)](/docs/DOCUMENT_REGISTRY.md)

Auto-generated summary companion to registry.toml; run doc_control_check.py to refresh

**For:** `contributors`, `agents` | **Updated:** 2026-06-22

### 📝 **Living** [ICN Golden Development Prompt](/docs/GOLDEN_PROMPT.md)

Master context and instructions for AI-assisted development on ICN

**For:** `agents`, `developers` | **Updated:** 2026-03-18

### 🔒 **Canonical** [ICN Documentation Index](/docs/INDEX.md)

Master navigation and directory of all documentation with cross-references

**For:** `all` | **Updated:** 2026-04-15

### 🔒 **Canonical** [Docs Directory README](/docs/README.md)

Overview of documentation structure and navigation guide

**For:** `contributors` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Gateway API Documentation](/docs/api/OPENAPI.md)

HTTP API specification and usage guide for ICN Gateway

**For:** `developers`, `integrators` | **Updated:** 2026-03-15

### 🔒 **Canonical** [API Documentation Index](/docs/api/README.md)

Navigation and overview for API-related documentation

**For:** `developers`, `integrators` | **Updated:** 2026-03-10

### 📝 **Living** [OpenAPI Specification](/docs/api/openapi.yaml)

Machine-readable API specification in OpenAPI 3.0 format

**For:** `developers`, `tools` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Architecture Directory README](/docs/architecture/README.md)

Navigation guide for architecture documentation

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [CI Current Status](/docs/ci/CI_CURRENT_STATUS.md)

Current state of all CI checks and gate enforcement levels

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [CI Gate Ratchet Plan](/docs/ci/GATE_RATCHET_PLAN.md)

CI gates, ratchet phases, required checks, and failure index

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Institution Package ActionCard Contract Notes](/docs/contracts/institution-package/README.md)

Validation guidance for institution packages using action-card.schema.json; emitted vs RFC-gated source kinds; pointers to runtime models, ADR-0027, and issue icn#1713. JSON schema and fictional example live alongside; tiny validator at docs/scripts/validate-action-card.py. Schema $id retained temporarily per docs/contracts/schema-id-audit.md (review by 2026-06-30).

**For:** `contributors`, `organizers` | **Updated:** 2026-05-07

### 📝 **Living** [Pending-publish summary — contract notes](/docs/contracts/pending-publish-summary.md)

Companion notes for the substrate-level row-level read-model contract for pending-publish summaries (urn:icn:contract:pending-publish-summary:v1). Composed body for preview-review.preview_kind = pending_publish_summary; does not replace urn:icn:contract:preview-review:v1. Defines the per-row shape (action item / decision / attendance / obligation / allocation / settlement / evidence note / risk note), review affordances, mutation_preview, expected receipt category, provenance reference, must-not-include list, and validation guidance. Read-only; not a mutation API. JSON schema and fictional example live alongside; validated via docs/scripts/validate-preview-review.py --schema.

**For:** `contributors`, `architects`, `organizers` | **Updated:** 2026-06-09

### 📝 **Living** [Preview / review — contract notes](/docs/contracts/preview-review.md)

Companion notes for the substrate-level read-model contract for human-reviewable previews of pending publish, action items, evidence packets, and fixture demos (urn:icn:contract:preview-review:v1). Defines field shape, must-not-include list, validation guidance, stability, and how the contract fits the no-CLI organizer/member workflow + organizer/member accessibility gate. Closes the 'Generic preview/review API contract' follow-up in icn#1724 / no-CLI workflow §7. Read-only; not a mutation API. JSON schema and fictional example live alongside; tiny validator at docs/scripts/validate-preview-review.py.

**For:** `contributors`, `architects` | **Updated:** 2026-05-05

### 📝 **Living** [Rehearsal evidence export — contract notes](/docs/contracts/rehearsal-evidence-export.md)

Companion notes for the substrate-level repo-safe rehearsal evidence export schema (urn:icn:contract:rehearsal-evidence-export:v1). Defines field shape, must-not-include list, validation guidance, stability, and the non-DNS contract-identity decision. JSON schema and fictional example live alongside; tiny validator at docs/scripts/validate-rehearsal-evidence.py.

**For:** `contributors`, `organizers` | **Updated:** 2026-05-04

### 📝 **Living** [Schema $id audit](/docs/contracts/schema-id-audit.md)

Audit-only record of every JSON schema $id under docs/contracts/, classified DNS-backed vs non-DNS, with per-schema keep/migrate/investigate recommendations and migration safety rules. Performs no migration. First deliberate application of the architecture due-diligence checklist; deliverable for icn#1737.

**For:** `architects`, `contributors` | **Updated:** 2026-05-05

### 📝 **Living** [Demo System Documentation](/docs/demo/README.md)

Documentation for ICN demonstration system

**For:** `team`, `demo-users` | **Updated:** 2026-03-15

### 🔒 **Canonical** [Design Directory README](/docs/design/README.md)

Navigation guide for design documentation

**For:** `architects`, `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Economics Directory README](/docs/design/economics/README.md)

Navigation for economics and value flow documentation

**For:** `architects` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Governance Directory README](/docs/design/governance/README.md)

Navigation guide for governance documentation

**For:** `architects` | **Updated:** 2026-03-10

### 🔒 **Canonical** [SDIS Design Documentation](/docs/design/sdis/README.md)

Navigation guide for Sovereign Digital Identity System design

**For:** `architects` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Development Documentation](/docs/development/README.md)

Navigation guide for development activities

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Testing Documentation](/docs/development/testing/README.md)

Navigation guide for testing guides

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Policy Examples](/docs/examples/policies/README.md)

Example governance policies for cooperative organizations

**For:** `users`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Glossary](/docs/glossary.md)

Reference document with ICN terminology and definitions

**For:** `all` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Operations Runbooks](/docs/guides/operations/runbooks/README.md)

Navigation guide for production runbooks (domain hub; not control-plane canonical)

**For:** `operators` | **Updated:** 2026-03-26

### 🔒 **Canonical** [Internal Documentation](/docs/internal/README.md)

Internal-only documentation for team coordination

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [Imported documentation control context pack (readme)](/docs/internal/documentation-control-system/00_CONTEXT_PACK_README.md)

Index and provenance for bundled control-system specs and templates source pack

**For:** `contributors`, `agents` | **Updated:** 2026-03-26

### 📋 **Draft** [ICN Legal Considerations](/docs/internal/legal-considerations.md)

Legal questions and considerations for cooperative communities

**For:** `compliance` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Deployment Limitations](/docs/internal/pilots/pilot-limitations.md)

Known limitations and constraints in pilot phase

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [Pilot Readiness Gaps](/docs/internal/pilots/pilot-readiness-gaps.md)

Critical gaps between implementation and pilot readiness

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [Storage Metrics Reference](/docs/observability/storage-metrics.md)

Prometheus metrics for storage and database monitoring

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Module Template](/docs/onboarding/module-template.md)

Template for creating onboarding modules

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Common Patterns Reference](/docs/onboarding/patterns.md)

Quick reference for recurring code patterns in ICN

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Performance Documentation](/docs/performance/README.md)

Performance requirements, benchmarks, and optimization guidance

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [Trust Score Benchmark Results](/docs/performance/trust-score-benchmark-results.md)

Benchmark results for trust score performance

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Trust Service Performance Characteristics](/docs/performance/trust-service-performance.md)

Performance characteristics and optimization guidance

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [Decision Registry + Treasury Vote Pilot](/docs/pilots/decision_registry_treasury_vote.md)

Economic receipt chain implementation in pilot

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [Hosted Pilot Approach](/docs/pilots/hosted-approach.md)

Approach for hosted cooperative pilot deployments

**For:** `team` | **Updated:** 2026-03-10

### 📝 **Living** [No-CLI organizer and member rehearsal workflow (generic ICN)](/docs/pilots/no-cli-organizer-member-rehearsal-workflow.md)

Guided browser/mobile-first rehearsal story: standing, action cards, action items, receipts, provenance, repo-safe evidence; organizer vs steward vs member paths; preview-before-mutation; CLI as operator layer only; follow-ups for UI/API/evidence contracts including the landed fixture-backed demo-mode bridge slice. Partner companion doc lives in NYCN repo.

**For:** `team`, `organizers` | **Updated:** 2026-06-28

### 📝 **Living** [Organizer facilitator walkthrough accessibility evidence (partial)](/docs/pilots/organizer-facilitator-walkthrough-accessibility-pass.md)

Partial browser-assisted accessibility evidence for the fixture-backed, read-only organizer facilitator walkthrough merged in #2239. Records keyboard focus-order, semantic-tree, forced-colors, reduced-motion, target-size, narrow/reflow, and network observations against the twelve-category organizer/member gate. Explicitly does not claim a human, screen-reader, switch-control, low-vision, legal-conformance, organizer-readiness, member-facing-readiness, pilot-readiness, mutation, receipt, or evidence-export pass. Keeps #2041/#1726/#1727/#1746 open and records the remaining named human/AT work.

**For:** `team`, `organizers`, `contributors` | **Updated:** 2026-06-28

### 📝 **Living** [Summit Ops Closeout Continuity Packet (generic ICN)](/docs/pilots/summit-ops-closeout-continuity-packet.md)

Docs-only map of the Summit Ops 'close the loop' lifecycle stage: how a package turns post-event work (attendance summary, speaker/sponsor follow-up, reimbursements, accessibility lessons, incident closeout, volunteer appreciation, budget reconciliation, public recap, next-year continuity, evidence export, follow-up register) into repo-safe shapes and future ICN action-card/receipt/evidence candidates. L1 declared shapes / rehearsal-ready — not fixture-backed, not runtime proof; categorical/fictional only, no private data; no pilot-UI change (#2099 gates that).

**For:** `team`, `organizers` | **Updated:** 2026-06-26

### 📝 **Living** [Summit Ops Closeout Recap Fixture Shape (generic ICN)](/docs/pilots/summit-ops-closeout-recap-fixture-shape.md)

Fixture-shape map (spec + validation recipe) for the exact schema-valid (action_item/complete, scope structure, fictional ids) Public Recap Draft Handoff ActionCard a future contributor could append to web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json, plus the matching demo Communications role + public_recap scope that commit must add to standing.json. Leaves the lane fixture-ready, NOT fixture-backed: no runtime fixture is committed and no pilot-UI file is touched (#2099 gates pilot-UI surface). Becomes L2 only after the card + standing are committed and the validation path (per-card schema + validate-rehearsal-shell-fixtures.py + Playwright e2e + Rehearsal Fixture Bundle gate) passes; public-safe categorical/fictional only, no real event data.

**For:** `team`, `organizers` | **Updated:** 2026-06-26

### 📝 **Living** [Summit Ops Lifecycle Package Map (generic ICN)](/docs/pilots/summit-ops-lifecycle-package-map.md)

Generic ICN-side map of how an institution package (NYCN motivating example, 2026 Summit) carries an event's full lifecycle (plan/prepare/run/close) onto the ICN vertical spine, preserving the Google-live / NYCN-package / future-ICN-node boundary. Reuses existing status + proof-level vocabulary; docs-only; no live sync, no partner-repo mutation, no formal-pilot claim.

**For:** `team`, `organizers` | **Updated:** 2026-06-26

### 📝 **Living** [Summit Ops Registration Action-Card Proof Loop (generic ICN)](/docs/pilots/summit-ops-registration-action-card-proof-loop.md)

First proof-loop child of the run-stage facilitator path: a fictional Registration Desk lane walked end-to-end through the ICN proof loop (source packet -> reviewed candidate -> ActionCard candidate -> authorized completion -> receipt candidate -> evidence export -> follow-up). The lane is fixture-backed (L2): a committed fictional schema-valid action_item/complete ActionCard the rehearsal shell loads and the e2e validates — a rehearsal-ready shape, NOT a runtime proof, not live NYCN action cards/receipts, not a node-hosted cockpit; fictional categorical examples only; no real attendee data.

**For:** `team`, `organizers` | **Updated:** 2026-06-26

### 📝 **Living** [Summit Ops Registration Fixture Shape (generic ICN)](/docs/pilots/summit-ops-registration-fixture-shape.md)

Fixture-shape map (spec/rationale + validation recipe) for the exact schema-valid (action_item/complete, scope structure, fictional ids) registration ActionCard in web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json, with the validation path (validate-rehearsal-shell-fixtures.py + the Playwright e2e + Rehearsal Fixture Bundle gate). The card has since been committed and validated, so the Registration Desk lane is fixture-backed (L2) — a committed fictional fixture, not a runtime proof; no real attendee data.

**For:** `team`, `organizers` | **Updated:** 2026-06-26

### 📝 **Living** [Summit Ops Run-Stage Facilitator Path (generic ICN)](/docs/pilots/summit-ops-run-stage-facilitator-path.md)

First concrete child of the Summit Ops lifecycle map: a fixture-backed, no-terminal event-day facilitator path for the run stage, mapping ten generic event-day lanes onto future ICN action cards / receipts / evidence while keeping NYCN private operating detail at boundary level. Docs-only; no live sync, no partner-repo mutation, no node-hosted cockpit claim.

**For:** `team`, `organizers` | **Updated:** 2026-06-26

### 📋 **Draft** [Agent Knowledge Architecture](/docs/planning/agent-knowledge-architecture.md)

Design for AI agent knowledge bases and context management

**For:** `agents`, `architects` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Crate Reference](/docs/planning/icn-crate-reference.md)

Authoritative inventory of workspace crates

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Governance Demo One-Pager](/docs/planning/icn-demo-one-pager.md)

One-page demo explanation and value proposition

**For:** `stakeholders` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Ecosystem Map](/docs/planning/icn-ecosystem-map.md)

System component interconnection map

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [API Reference](/docs/reference/api/API_REFERENCE.md)

Detailed API endpoint reference with examples and error codes

**For:** `developers` | **Updated:** 2026-03-15

### 🔒 **Canonical** [API Reference Documentation](/docs/reference/api/README.md)

Navigation guide for API reference documents

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [API Versioning Strategy](/docs/reference/api/api-versioning.md)

Versioning scheme and compatibility policy for ICN APIs

**For:** `developers`, `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [Topic Subscriptions API](/docs/reference/api/topic-subscriptions-api.md)

API for subscribing to and consuming topic streams

**For:** `developers` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Configuration Management Guide](/docs/reference/config/CONFIGURATION.md)

Complete configuration reference for ICN nodes

**For:** `operators`, `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Configuration Reference](/docs/reference/config/README.md)

Navigation guide for configuration documentation

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Identity Backend Configuration](/docs/reference/config/identity-backend-configuration.md)

Identity keystore backend configuration guide

**For:** `operators` | **Updated:** 2026-03-10

### 📋 **Draft** [Trust Threshold Configuration](/docs/reference/config/trust-threshold-configuration.md)

Trust score threshold configuration guide

**For:** `operators` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Project Index](/docs/reference/project-index/README.md)

Show-ready orientation layer — routes outside readers, contributors, and agents to the right canonical doc, source tree, or external URL. Defers to STATE.md and PHASE_PROGRESS.md for current truth. Includes the Truth Layer / Claim Discipline section cross-linking the existing truth systems.

**For:** `all` | **Updated:** 2026-06-20

### 📝 **Living** [CI / Ops / Deploy Map](/docs/reference/project-index/ci-ops-deploy-map.md)

GitHub Actions workflows, deploy paths, K3s smoke runbooks, monitoring; routing layer to substantive runbooks under guides/operations/.

**For:** `operators`, `contributors` | **Updated:** 2026-04-29

### 📝 **Living** [Claim-Boundary Map](/docs/reference/project-index/claim-boundaries.md)

Operational claim-boundary manual: disambiguates the architectural Meaning Firewall from the claim-discipline firewall, summarizes source precedence and status-vs-proof, tabulates forbidden collapses with their evidence and enforcement status, and gives a reusable inventory/PR claim-discipline checklist. Orientation, not a truth root. Defers to STATE.md and PHASE_PROGRESS.md for current truth.

**For:** `all`, `team` | **Updated:** 2026-06-26

### 📝 **Living** [Current Truth Map](/docs/reference/project-index/current-truth-map.md)

One-screen routing for what is real now, what is not, what gates remain — pointing at STATE.md and PHASE_PROGRESS.md for the per-PR record.

**For:** `all` | **Updated:** 2026-04-29

### 📝 **Living** [Docs Control Map](/docs/reference/project-index/docs-control-map.md)

How INDEX.md, registry.toml, DOCUMENT_REGISTRY.md, and doc_control_check.py relate; truth classes; how to add a doc.

**For:** `contributors`, `agents` | **Updated:** 2026-04-29

### 📝 **Living** [Full Repository Record Protocol](/docs/reference/project-index/full-repo-record.md)

Protocol for recording every tracked file and directory across InterCooperative-Network/icn (and adjacent repos) as a mechanical record plus an interpretive atlas. Defines outputs, generator, classification vocabulary, and privacy boundary.

**For:** `contributors`, `architects` | **Updated:** 2026-05-01

### 📝 **Living** [ICN Invariants Catalog](/docs/reference/project-index/invariants-catalog.md)

Source-linked index of the four canonical ICN invariant families (5 operational / 6 firewall-contract / 10 frozen-core / 7 regulatory = 28), each linked to its canonical source with a stable anchor. Indexes only; it does not define invariants, and the canonical sources remain authoritative. Machine-readable companion: invariants-catalog.toml. Implements the #2114 deliverable.

**For:** `all`, `team` | **Updated:** 2026-06-22

### 📝 **Living** [Proof-Level Taxonomy and Capability Matrix](/docs/reference/project-index/proof-level-taxonomy-capability-matrix.md)

Proof-level taxonomy (L0-L8) as shared claim-boundary vocabulary, plus a capability matrix for the current organizer-rehearsal path. Supports #1746 and narrows #1796. Orthogonal to the project-coverage-matrix status vocabulary. Defers to STATE.md and PHASE_PROGRESS.md for current truth.

**For:** `all`, `team` | **Updated:** 2026-06-26

### 📋 **Draft** [ICN Repo Atlas](/docs/reference/project-index/repo-atlas.md)

Draft interpretive atlas paired with the mechanical full-repo record. Names directory families, Rust-workspace families, and a classification vocabulary for stable atlas authoring across icn / nycn / icn-learn.

**For:** `contributors`, `architects` | **Updated:** 2026-05-01

### 📝 **Living** [Runtime Surface Map](/docs/reference/project-index/runtime-surface-map.md)

Real runtime surfaces a member or app actually touches today (/me/standing, /me/action-cards, completion-receipt retrieval, governance primitives, identity/trust/ledger surfaces).

**For:** `developers`, `architects` | **Updated:** 2026-04-29

### 📝 **Living** [Rust Workspace Map](/docs/reference/project-index/rust-workspace-map.md)

The icn/ Rust workspace grouped by rough layer (kernel, identity, networking, ledger, governance, etc.) plus app crates and binaries. Authority over kernel/app boundaries lives in KERNEL_APP_SEPARATION.md.

**For:** `developers`, `contributors` | **Updated:** 2026-04-29

### 📝 **Living** [Show-Readiness Map](/docs/reference/project-index/show-readiness-map.md)

What can be shown now, what should not be shown as finished, suggested first-demo narrative, and red lines for outside-facing material.

**For:** `all`, `team` | **Updated:** 2026-04-29

### 📝 **Living** [Source Tree Map](/docs/reference/project-index/source-tree-map.md)

Top-level repo surfaces and what each is for; monorepo root vs Rust workspace at icn/.

**For:** `contributors`, `developers` | **Updated:** 2026-04-29

### 📝 **Living** [SDIS API Guide](/docs/sdis/SDIS_API_GUIDE.md)

Complete API guide for Sovereign Digital Identity System

**For:** `developers` | **Updated:** 2026-03-10

### 📋 **Draft** [SDIS + Steward System Status](/docs/sdis/SDIS_STATUS.md)

Snapshot of SDIS deployment status

**For:** `team` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Grant Application Artifacts](/docs/strategy/grants/README.md)

Navigation for grant application templates and materials

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Budget Skeleton](/docs/strategy/grants/budget-skeleton.md)

Grant budget template

**For:** `team` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Documentation Templates](/docs/templates/README.md)

Navigation guide for documentation templates

**For:** `contributors` | **Updated:** 2026-03-10

### 📋 **Draft** [Development Journal Template](/docs/templates/dev-journal.md)

Template for development session journals

**For:** `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Vision & Strategy](/docs/vision/README.md)

Navigation guide for vision and strategy documents

**For:** `all` | **Updated:** 2026-03-10


## Security

### 📝 **Living** [Gateway Content Security Policy](/docs/security/GATEWAY_CSP.md)

CSP configuration for ICN gateway ensuring safe web integration

**For:** `security`, `developers` | **Updated:** 2026-03-10

### 🔒 **Canonical** [Security Documentation Index](/docs/security/README.md)

Navigation and overview of security-related documentation

**For:** `security`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [Secret Management](/docs/security/SECRET_MANAGEMENT.md)

Policy and design for managing cryptographic secrets and sensitive data

**For:** `security`, `operators` | **Updated:** 2026-03-10

### 📝 **Living** [TOFU (Trust-On-First-Use) Security Model](/docs/security/TOFU_SECURITY_MODEL.md)

Trust establishment for first-time peer contact without certificates or CAs

**For:** `security`, `developers` | **Updated:** 2026-03-10

### 📝 **Living** [CodeQL Alert Triage (2026-06-29)](/docs/security/codeql-alert-triage-2026-06-29.md)

Point-in-time static triage of CodeQL alerts #100 and #101, with an inventory of other open alerts

**For:** `security`, `developers` | **Updated:** 2026-06-29

### 📝 **Living** [CodeQL Gossip Nonce Triage (2026-06-29)](/docs/security/codeql-gossip-nonce-triage-2026-06-29.md)

Point-in-time static triage of gossip nonce alerts #30 through #35

**For:** `security`, `developers` | **Updated:** 2026-06-29

### 📝 **Living** [CodeQL Triage Closeout (2026-06-29)](/docs/security/codeql-triage-closeout-2026-06-29.md)

Point-in-time open-alert inventory, detailed remaining triage, and maintainer disposition checklist

**For:** `security`, `developers` | **Updated:** 2026-06-29

### 📋 **Draft** [Phase 10C Security Analysis](/docs/security/phase-10c-security-analysis.md)

Security analysis and hardening for multi-party contracts

**For:** `security` | **Updated:** 2026-03-10

### 📝 **Living** [Production Hardening](/docs/security/production-hardening.md)

Hardening measures protecting against DoS, resource exhaustion, and operational failures

**For:** `operators`, `security` | **Updated:** 2026-03-15

### 📝 **Living** [ICN Security Roadmap](/docs/security/security-roadmap.md)

Security architecture and phased hardening approach

**For:** `security`, `architects` | **Updated:** 2026-03-15

### 📝 **Living** [ICN Threat Model](/docs/security/threat-model.md)

Comprehensive threat model covering attack vectors, adversary capabilities, and mitigations

**For:** `security`, `architects` | **Updated:** 2026-03-15


## Status

### 📝 **Living** [ICN State (living doc)](/docs/STATE.md)

Living snapshot of repo layout, decisions, constraints, and current engineering status

**For:** `developers`, `agents` | **Updated:** 2026-06-26


## Strategy

### 📋 **Draft** [Documentation namespace resolution plan](/docs/planning/documentation-namespace-resolution.md)

Plans vs planning, ops vs operations; no moves executed here

**For:** `contributors` | **Updated:** 2026-03-26

### 📋 **Draft** [SDIS Complete Build Plan](/docs/sdis/SDIS_BUILD_PLAN.md)

Detailed SDIS build plan from API to mobile

**For:** `architects` | **Updated:** 2026-03-10

### 📋 **Draft** [SDIS & Steward Completion Roadmap](/docs/sdis/SDIS_STEWARD_ROADMAP.md)

Roadmap for SDIS and steward system completion

**For:** `architects` | **Updated:** 2026-03-10

### 📝 **Living** [ADR-001: What ICN Is](/docs/strategy/ADR-001-What-ICN-Is.md)

Architectural Decision Record defining ICN scope, non-goals, and boundary conditions

**For:** `developers`, `architects`, `stakeholders` | **Updated:** 2026-02-28

### 📝 **Living** [Cooperative-developer discovery brief](/docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md)

Internal prep doc for discovery conversations with cooperative developers (e.g. launch.coop / comp.coop). Listening + language calibration; not a sales pitch, not a technical demo, not a partnership commitment.

**For:** `internal`, `matt` | **Updated:** 2026-04-29

### 📝 **Living** [What ICN Is](/docs/strategy/ICN-Definition.md)

Canonical definition: problem statement, solution approach, scope and non-goals

**For:** `all` | **Updated:** 2026-03-17

### 📝 **Living** [ICN Evolution Arc](/docs/strategy/ICN-Evolution-Arc.md)

Long-term vision across phases 0-3, from MVP to mature cooperative ecosystem

**For:** `architects`, `stakeholders` | **Updated:** 2026-03-08

### 📝 **Living** [ICN Gap Analysis March 2026](/docs/strategy/ICN-Gap-Analysis-March-2026.md)

Comprehensive implementation assessment across 10 subsystems with evidence and gaps

**For:** `grant-reviewers`, `architects`, `product` | **Updated:** 2026-03-17

### 📝 **Living** [ICN Pitch](/docs/strategy/ICN-Pitch.md)

Elevator pitch, one-pagers, and public communication messaging framework

**For:** `stakeholders`, `public` | **Updated:** 2026-03-10

### 📝 **Living** [ICN Live Roadmap](/docs/strategy/ICN-Roadmap-Live.md)

Long-arc roadmap and rationale companion that points at STATE.md / PHASE_PROGRESS.md as canonical current-state truth. Not a per-PR changelog.

**For:** `team`, `stakeholders` | **Updated:** 2026-04-29

### 📝 **Living** [ICN Roadmap Strategy](/docs/strategy/ICN-Roadmap-Strategy.md)

Strategic roadmap phases 0-3 with dependencies, milestones, and long-term vision

**For:** `architects`, `stakeholders`, `grant-reviewers` | **Updated:** 2026-03-12

### 📝 **Living** [ICN Scenarios](/docs/strategy/ICN-Scenarios.md)

Use case narratives and operational scenarios demonstrating ICN in practice

**For:** `product`, `marketing` | **Updated:** 2026-03-09

### 📝 **Living** [ICN Sprint March 17](/docs/strategy/ICN-Sprint-March17.md)

Specific sprint plan and tactical objectives for week of March 17, 2026

**For:** `team` | **Updated:** 2026-03-17

### 📝 **Living** [ICN Technical Whitepaper](/docs/strategy/ICN-Technical-Whitepaper.md)

Formal technical specification for grants, regulatory review, and architectural validation

**For:** `grant-reviewers`, `architects`, `compliance` | **Updated:** 2026-03-15

### 📝 **Living** [ICN: Infrastructure for the Cooperative Movement](/docs/strategy/ICN_FOR_COOPERATIVE_MOVEMENT.md)

Plain-English ICN introduction for cooperative developers, federation organizers, TA providers, and member-owners. Honest comparisons to existing co-op tech, role-by-role fit, explicit non-claims. Pre-pilot framing throughout; claims bounded by ICN_INTRODUCTION_EVIDENCE_MAP.md.

**For:** `public`, `stakeholders`, `organizers` | **Updated:** 2026-06-09

### 📝 **Living** [ICN, in Plain English](/docs/strategy/ICN_FOR_EVERYONE.md)

General-public ICN introduction starting from 'what is a cooperative'. No jargon wall. Explains receipts as evidence records (not crypto), institutional memory, and accountability. Explicit non-claims; pre-pilot framing throughout.

**For:** `public` | **Updated:** 2026-06-09

### 📝 **Living** [ICN one-page handbill](/docs/strategy/ICN_HANDBILL.md)

One-page handbill: the problem, what ICN does, and where it actually stands (pre-pilot, not production-ready). Links to the evidence map and hard-questions Q&A.

**For:** `public`, `organizers` | **Updated:** 2026-06-09

### 📝 **Living** [ICN Hard Questions and Evidence-Bound Answers](/docs/strategy/ICN_HARD_QUESTIONS.md)

Hard questions answered directly in bad-answer/honest-answer format: production use (no), what works now, fixture-backed vs live vs design-only, capture, surveillance, private data, regulation, blockchain (no), bus factor, smallest safe next step. Adapted from internal hardball rehearsal practice; generalized and depersonalized.

**For:** `public`, `stakeholders`, `organizers`, `reviewers` | **Updated:** 2026-06-09

### 📝 **Living** [ICN Introduction Evidence Map](/docs/strategy/ICN_INTRODUCTION_EVIDENCE_MAP.md)

Maps every claim in the introduction materials to verifiable merged artifacts (icn#1985/#1997/#1998/#1999, nycn#78, icn-learn#3, icn-community-bridge#1) and states what each artifact does NOT prove. Anti-overclaim companion to the intro docs; defers to STATE.md and PHASE_PROGRESS.md for current truth.

**For:** `public`, `stakeholders`, `organizers`, `reviewers` | **Updated:** 2026-06-09

### 📋 **Draft** [Licensing strategy matrix (autonomy review)](/docs/strategy/LICENSING_STRATEGY_MATRIX.md)

Planning artifact only. Component-by-component licensing/autonomy matrix and option families (permissive / AGPL / CAL / policy-layer / hybrid) for a future maintainer/legal review. Not legal advice; not a relicensing decision; no metadata changes.

**For:** `maintainers`, `legal-review` | **Updated:** 2026-05-01

### 📋 **Draft** [ICN Compliance Architecture](/docs/strategy/grants/compliance-architecture.md)

Regulatory-safe design rationale for grants

**For:** `compliance`, `grant-reviewers` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Grant Narrative Core](/docs/strategy/grants/grant-narrative-core.md)

Reusable grant narrative sections

**For:** `team` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Grant One-Pager](/docs/strategy/grants/grant-one-pager.md)

One-page ICN summary for grant applications

**For:** `grant-reviewers` | **Updated:** 2026-03-10

### 📋 **Draft** [ICN Milestones](/docs/strategy/grants/milestones.md)

Project timeline through pilot deployment

**For:** `grant-reviewers` | **Updated:** 2026-03-10

### 📋 **Draft** [Pilot Readiness Assessment](/docs/strategy/grants/pilot-readiness.md)

Assessment of pilot readiness and gaps

**For:** `team` | **Updated:** 2026-03-10


---

## Summary

**Total documents:** 352

**By status:**
- Active: 1
- Canonical: 40
- Draft: 99
- Living: 212
