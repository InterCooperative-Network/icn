---
Status: operational
Canonical: yes
Last Reviewed: 2026-04-15
---

## Where things live (control plane)

| What | Where |
|------|--------|
| **Canonical entry docs** | [STATE.md](STATE.md) (living snapshot), [ARCHITECTURE.md](ARCHITECTURE.md) (system truth), [DOCUMENTATION_CONTROL_SYSTEM.md](DOCUMENTATION_CONTROL_SYSTEM.md) (doc/process policy), and this [INDEX.md](INDEX.md) (navigation). |
| **Machine registry** | [`registry.toml`](registry.toml) — `[control]`, `[[doc_path_defaults]]`, `[docs."path"]` rows. |
| **Human registry summary** | [DOCUMENT_REGISTRY.md](DOCUMENT_REGISTRY.md) — regenerate via `doc_control_check.py --write-document-registry`. |
| **Validator** | [`scripts/doc_control_check.py`](scripts/doc_control_check.py) |
| **Historical material** | [`archive/`](archive/) (by year); session-style content often still under `development/` until archived. |
| **Supplemental `state/` tree** | [state/README.md](state/README.md) — not a substitute for root [STATE.md](STATE.md). |

**Control-plane canonical (single definition):** Only the paths listed in `[control].canonical_doc_paths` in [`registry.toml`](registry.toml) are “canonical” for CI header checks. Other docs may use `canonical = "yes"` in a registry row for emphasis, but that does not add them to the control-plane set unless `canonical_doc_paths` is updated.

**INDEX vs generated index:** This [`INDEX.md`](INDEX.md) is the **authoritative** hand-maintained navigation. [`INDEX.generated.md`](INDEX.generated.md) is **derived** (registry/corpus snapshot for drift detection in CI); if the generator disagrees with this file, reconcile here intentionally.

**Canon vs history:** Prefer YAML on important docs: `Status:` (`normative` / `descriptive` / `operational` / `historical` / `draft`) and `Canonical: yes|no`. Do not treat undated snapshots or `archive/` neighbors as current truth without checking [STATE.md](STATE.md) and dates.

**One-line check:** `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml` (optional `--strict` before merge).

# ICN Documentation Index

Welcome to the ICN (Intercooperative Network) documentation! This index provides clear navigation to all documentation.

**Last Updated**: 2026-06-01
**Version**: 2.1 (canon-sync pass)

---

## 📋 Quick Navigation

- **Whole ecosystem at a glance?** → [Ecosystem Atlas](ATLAS.md) (cross-repo map + source-of-truth index + agent preflight)
- **New to ICN?** → Start with [Getting Started Guide](GETTING_STARTED.md)
- **Looking for the public engagement map?** → Start with [intercooperative.network/get-involved](https://intercooperative.network/get-involved)
- **Understanding the system?** → Read [Architecture Overview](ARCHITECTURE.md)
- **Building the public site?** → Start with [Design Language Brief v0](design-language/brief-v0.md)
- **Building features?** → Check [Developer Guides](#developer-guides)
- **Deploying ICN?** → See [Operations Guides](#operations-guides)
- **Looking for APIs?** → Browse [API Reference](#api-reference)
- **Historical context?** → Explore [Archives](#archives)

### Start here by role

- **Developer or technical contributor**: [GETTING_STARTED.md](GETTING_STARTED.md) → [CONTRIBUTING.md](../CONTRIBUTING.md) → [onboarding/README.md](onboarding/README.md) → [good first issues](https://github.com/InterCooperative-Network/icn/issues?q=is%3Aissue+is%3Aopen+label%3Agood-first-issue)
- **Non-technical contributor**: [intercooperative.network/get-involved](https://intercooperative.network/get-involved) → [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions)
- **Institutional partner or cooperative evaluator**: [intercooperative.network/for-cooperatives](https://intercooperative.network/for-cooperatives) → [intercooperative.network/whats-real-now](https://intercooperative.network/whats-real-now) → [GitHub Discussions](https://github.com/InterCooperative-Network/icn/discussions)
- **Financial supporter**: [GitHub Sponsors](https://github.com/sponsors/InterCooperative-Network)

### Design language (canonical)

The ICN public website is the first implementation surface of the universal civic design language. These docs are the source of truth for every public-facing design decision:

- **[design-language/brief-v0.md](design-language/brief-v0.md)** — the canonical design language brief (principles, semantic layers, visual primitives, anti-patterns)
- **[design-language/concept-map.md](design-language/concept-map.md)** — canonical → public plain-language label → localization notes for every ICN concept
- **[design-language/accessibility.md](design-language/accessibility.md)** — WCAG rules, contrast requirements, the review checklist every PR must pass

### Visual explainers and assets

Control plane for ICN's visual explainer system — the doctrine, asset register, briefs, and review gate that govern every diagram, infographic, generated image, and source asset:

- **[design/ICN_VISUAL_EXPLAINER_BIBLE.md](design/ICN_VISUAL_EXPLAINER_BIBLE.md)** — purpose, source hierarchy, truth labels, vocabulary rules, accessibility floor, core explainer models, brief gate, generated-image workflow, production-source rule
- **[design/assets/README.md](design/assets/README.md)** — directory orientation: where production assets live, where sketches live, the asset lifecycle
- **[design/assets/ASSET_REGISTER.md](design/assets/ASSET_REGISTER.md)** — live register of planned and tracked visual assets (VE-001…VE-012 initial rows)
- **[design/assets/VISUAL_REVIEW_CHECKLIST.md](design/assets/VISUAL_REVIEW_CHECKLIST.md)** — the gate every visual explainer clears before shipping
- **[design/assets/briefs/](design/assets/briefs/)** — per-asset briefs (closure loop, scope model, decision-to-receipt, member shell concept, kernel/app separation)

### Claude Design — seed review and handoff

Governance scaffold for taking Claude Design seed artifacts into the canonical repo without confusing generated output with repo truth. Default: a seed is a proposal, not a design system:

- **[design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md](design/CLAUDE_DESIGN_REVIEW_PROTOCOL.md)** — seven review gates (truth-label, accessibility, vocabulary, implementation-status, language/RTL, low-bandwidth/reduced-motion/large-text, source-doc drift); holds the canonical-vs-generated boundary
- **[design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md](design/CLAUDE_DESIGN_HANDOFF_TEMPLATE.md)** — seed-level handoff template for passing a Claude Design seed to Claude Code (one filled copy per seed)
- **[design/MUST_NOT_SHIP.md](design/MUST_NOT_SHIP.md)** — twelve-item hard rejection floor for design artifacts
- **[design/claude-design-seed/README.md](design/claude-design-seed/README.md)** — seed directory workflow; what canonical truth is and how it is preserved
- **[design/claude-design-seed/CHANGELOG.md](design/claude-design-seed/CHANGELOG.md)** — imported seed versions, one entry per seed
- **[design/claude-design-seed/REVIEW_NOTES.md](design/claude-design-seed/REVIEW_NOTES.md)** — human-readable per-seed review summary
- **[design/icons/CANDIDATES.md](design/icons/CANDIDATES.md)** — 14 candidate icons proposed for promotion into the production icon set; companion contact sheet at [`design/icons/contact-sheet.svg`](design/icons/contact-sheet.svg)

---

## 📚 Core Documentation

Essential reading for all ICN users and contributors.

| Document | Description |
|----------|-------------|
| [README.md](README.md) | Documentation overview and structure |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Comprehensive system architecture (160KB+) |
| [DESIGN_PRINCIPLES.md](DESIGN_PRINCIPLES.md) | Canonical three-tier index of operational, firewall, and frozen-core invariants |
| [design/ICN_DESIGN_SYSTEM.md](design/ICN_DESIGN_SYSTEM.md) | UI/UX/visual/accessibility design system entry point (kernel-bound via §"Kernel architecture binding") |
| [design/CLAUDE_DESIGN_SETUP.md](design/CLAUDE_DESIGN_SETUP.md) | How to use Claude Design with ICN — paste flow, local `/design:*` skills, job-card templates, failure modes |
| [GETTING_STARTED.md](GETTING_STARTED.md) | Quick start guide for developers |
| [PHASE_HISTORY.md](PHASE_HISTORY.md) | Development phase completion history |
| [PHASE_PROGRESS.md](PHASE_PROGRESS.md) | Active phase tracking and cross-cutting metrics |
| [STATE.md](STATE.md) | Current project state snapshot |
| [TODO.md](TODO.md) | Active work items and priorities |
| [glossary.md](glossary.md) | ICN terminology and definitions |

---

## 🏗️ Architecture & Design

In-depth architectural documentation and design specifications.

### Architecture Documentation (`architecture/`)

Comprehensive architectural reviews and design decisions:

- [ICN_OPERATING_MODEL.md](architecture/ICN_OPERATING_MODEL.md) - Vocabulary, placement & operating-model doctrine (grammar map; not a current-state truth root)
- [ARCHITECTURE_INDEX.md](architecture/ARCHITECTURE_INDEX.md) - Architecture document index
- [ARCHITECTURE_MAP.md](architecture/ARCHITECTURE_MAP.md) - Visual architecture guide (197KB)
- [ARCHITECTURE_QUICK_REF.md](architecture/ARCHITECTURE_QUICK_REF.md) - Quick reference card
- [CANONICAL_ENCODING.md](architecture/CANONICAL_ENCODING.md) - Wire format specifications
- [CELLS_AND_SCOPES.md](architecture/CELLS_AND_SCOPES.md) - Cell-based federation model
- [SCOPE_BOUNDED_TRUST.md](architecture/SCOPE_BOUNDED_TRUST.md) - Trust scope architecture
- [KERNEL_APP_SEPARATION.md](architecture/KERNEL_APP_SEPARATION.md) - Kernel/app boundary design
- [PRIVATE_DATA_DISCLOSURE_BOUNDARY.md](architecture/PRIVATE_DATA_DISCLOSURE_BOUNDARY.md) - Private-data disclosure/access boundary: scoped vaults, opaque receipt storage, disclosure policies, and access/export receipts (#1792, design-only)
- [ABUSE_CASE_HARDENING_STRATEGY.md](architecture/ABUSE_CASE_HARDENING_STRATEGY.md) - Institutional-failure-mode hardening doctrine
- [DEBIAN_APPLIANCE_MODEL.md](architecture/DEBIAN_APPLIANCE_MODEL.md) - Debian appliance / installable node image (scaffold-only; not production)
- [FEDERATION_INTEROP_CONTRACT.md](architecture/FEDERATION_INTEROP_CONTRACT.md) - Federation protocols
- [CLIENT_MODEL.md](architecture/CLIENT_MODEL.md) - Client architecture patterns
- [GOVERNANCE_STATE_MACHINE.md](architecture/GOVERNANCE_STATE_MACHINE.md) - Governance flow design
- [IDENTITY_MEMBERSHIP_ARCHITECTURE.md](architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md) - Identity & membership
- [NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md](architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md) - Downstream demand signal from NYCN's fake airlock rehearsals: what ICN must provide (Tool Commons / GovernedServiceBinding / ToolManifest / receipts) before a governed bridge can move real rows into custody (requirements note, not doctrine)
- Plus audit reports and gap analyses

### Design Documents (`design/`)

Feature designs, proposals, and evolution plans:

**Core Systems:**
- [COMMONS_EVOLUTION.md](design/COMMONS_EVOLUTION.md) - Commons-based governance evolution
- [MINIMAL-VIABLE-COOP.md](design/MINIMAL-VIABLE-COOP.md) - MVC specification
- [capability-based-features.md](design/capability-based-features.md) - Capability system design
- [passport-keyring-position-receipt.md](design/passport-keyring-position-receipt.md) - Passport/Keyring/Position/Receipt identity & custody vocabulary boundary (deprecates "wallet")
- [compute-substrate-design.md](design/compute-substrate-design.md) - Distributed compute layer
- [scheduler-evolution-plan.md](design/scheduler-evolution-plan.md) - Task scheduler design
- [multi-device-identity-design.md](design/multi-device-identity-design.md) - Multi-device identity
- [nat-traversal-design.md](design/nat-traversal-design.md) - NAT traversal strategy
- [post-quantum-crypto.md](design/post-quantum-crypto.md) - PQ cryptography design
- [platform-layer-design.md](design/platform-layer-design.md) - Platform abstraction
- [razeto-integration-design.md](design/razeto-integration-design.md) - Razeto model integration
- [social-recovery-design.md](design/social-recovery-design.md) - Social recovery mechanisms
- [entity-dissolution.md](design/entity-dissolution.md) & [entity-dissolution-example.md](design/entity-dissolution-example.md) - Entity lifecycle
- [coop-id-entity-resolver.md](design/coop-id-entity-resolver.md) - Governed coop_id → EntityId resolver seam (design only)
- [create-treasury-entity-id-semantics.md](design/create-treasury-entity-id-semantics.md) - CreateTreasury treasury entity_id trust semantics (design/audit only)
- [membership-coop-core-map-parity.md](design/membership-coop-core-map-parity.md) - apps/membership coop_core map-parity contract, #2082 gap 12b (design/audit only)
- [process-session-receipt-anchor.md](design/process-session-receipt-anchor.md) - ProcessSessionOpenedReceipt anchor contract, #1748/#2141 process spine (design/audit only)
- [deliberation-entry-recorded-receipt.md](design/deliberation-entry-recorded-receipt.md) - DeliberationEntryRecordedReceipt contract, #1748/#2141 process spine; Q3 taxonomy blocker triaged (design/audit only)
- [deliberation-entry-kind-taxonomy.md](design/deliberation-entry-kind-taxonomy.md) - Q3 decision: closed ADR-controlled entry_kind enum, ten-kind v1 list + discriminant rule (design decision only)
- [decision-recorded-receipt.md](design/decision-recorded-receipt.md) - DecisionRecordedReceipt contract, #1748/#2141 process spine; Q4 boundary blocker triaged (design/audit only)
- [decision-recorded-q4-decision.md](design/decision-recorded-q4-decision.md) - Q4 decision: generic recorded-decision fact, parallel/non-convergent with the proposal/vote decision lineage (design decision only)
- [institution-in-a-box.md](design/institution-in-a-box.md) - Organizational templates

**Economics (`design/economics/`):**
- [README.md](design/economics/README.md) - Economics documentation index
- [ECONOMIC_VISION.md](design/economics/ECONOMIC_VISION.md) - Strategic economic vision
- [ECONOMIC_ARCHITECTURE.md](design/economics/ECONOMIC_ARCHITECTURE.md) - Layered economy design
- [contribution-credits-design.md](design/economics/contribution-credits-design.md) - Credit system
- [economic-safety.md](design/economics/economic-safety.md) - Economic safety rails
- [econ-modeling.md](design/economics/econ-modeling.md) - Economic simulations

**Governance (`design/governance/`):**
- [README.md](design/governance/README.md) - Governance documentation index
- [PROJECT_GOVERNANCE.md](design/governance/PROJECT_GOVERNANCE.md) - Project governance model
- [governance.md](design/governance/governance.md) - Governance system design
- [governance-primitives.md](design/governance/governance-primitives.md) - Governance building blocks
- [witness-trust-validation.md](design/governance/witness-trust-validation.md) - Witness validation
- [decision-receipt-authority-and-grant-minting.md](design/governance/decision-receipt-authority-and-grant-minting.md) - Decision-receipt authority modes + grant-minting design (#1868)

**SDIS (`design/sdis/`):**
- [README.md](design/sdis/README.md) - SDIS design documentation index
- [social-recovery.md](design/sdis/social-recovery.md) - SDIS social recovery design

### Specifications (`spec/`)

Formal protocol and contract specifications:

- [KERNEL_CONTRACTS.md](spec/KERNEL_CONTRACTS.md) - Kernel contract specifications
- [effect-dispatch-contract.md](spec/effect-dispatch-contract.md) - Accepted-proposal effect dispatch contract (governance decision → mandate → effect plan → dispatch → application + evidence)
- [institutional-domain.md](spec/institutional-domain.md) - `InstitutionalDomain` and `DomainPolicy` primitive (the governed operating jurisdiction)
- [ccl-policy-registry.md](spec/ccl-policy-registry.md) - CCL policy registry, versioning, adoption contract, and governance-effect hook contract
- [governed-service-binding.md](spec/governed-service-binding.md) - `GovernedServiceBinding`, `WorkloadManifest`, and `RuntimeProvider` (the integrating envelope for hosted services, tools, compute jobs, and CCL evaluators)
- [storage-durability-policies.md](spec/storage-durability-policies.md) - `StorageSpec`, `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy`, restore-test receipts, locality + privacy inheritance, restore authority
- [artifact-registry-and-scoped-vault.md](spec/artifact-registry-and-scoped-vault.md) - `ArtifactRegistry` v0 (content-addressed artifact metadata) and `ScopedVault` (privacy-enforced container), the boundary between them, the artifact-class taxonomy, and six integration points (documents, compute outputs, evidence packets, private evidence, mobile cache, replication)
- [governed-bridge-receipts.md](spec/governed-bridge-receipts.md) - Governed bridge receipt vocabulary (draft spec / requirements vocabulary, #2365) — names the expected/future receipt family a governed bridge import would emit (`BridgeDryRunReceipt`, `BridgeReviewDecisionReceipt`, `BridgeImportReceipt` + minimum evidence, `VaultObjectWriteReceipt`, `ArtifactRegistrationReceipt`, `ExternalReferenceObservationReceipt`, discard/policy-block classes, and a provisional follow-up underlying-object receipt), with the evidence boundaries: `ArtifactReceipt` is transfer proof only (not registry registration), and action cards are derived read views (no card-write receipt). Derived from the NYCN airlock requirements note; vocabulary/boundary planning only, implements nothing
- [governed-bridge-toolmanifest-modes.md](spec/governed-bridge-toolmanifest-modes.md) - Governed bridge `ToolManifest` modes (draft spec / capability declaration model, #2366) — models how a future bridge tool (a `ToolRuntimeMode: bridge adapter` per RFC-0017) declares safety modes: `source_shape_read` vs gated `real_row_read`, `dry_run_preview`, `no_default_write`, `steward_review_handoff`, `custody_write`, `external_reference_observe`, `refusal_policy_enforcement`, `export_delete_recovery`; ties each mode to the receipt evidence contract in `governed-bridge-receipts.md` and defers per-run source/scope/target pinning to `GovernedServiceBinding` (#2367). Illustrative non-normative manifest sketch; no ToolManifest change, implements nothing
- [governed-bridge-service-binding.md](spec/governed-bridge-service-binding.md) - Governed bridge service binding (draft spec / binding model, #2367) — models how a future governed bridge **refines** `GovernedServiceBinding` (#1815) to pin per-`(record, field)` custody for one import run: `allowed_source_systems`, `field_custody_map` (source-authority → privacy class → custody target), `allowed_scoped_vault_targets` / `allowed_artifact_registry_namespaces` / `allowed_governed_object_targets`, `receipt_sink`, `required_receipts`, `export_delete_recovery_refs`, and 13 promotion gates. Inherits the binding's never-widens custody-class rule; per-run permission layer that narrows the #2371 manifest modes to concrete sources/scopes/targets. Illustrative non-normative field-custody-map sketch (opaque source refs, example field paths only); no binding change, implements nothing
- [governed-bridge-external-references.md](spec/governed-bridge-external-references.md) - Governed bridge external references (draft spec / external reference model, #2368) — the **observe-only** external-custodian model a governed bridge uses to record that an external system holds authority over a fact (invoice status, registration/ticket/check-in status, external file/consent id) **without** importing the document, processing settlement, or claiming to be its source of truth. Defines the reference record (opaque/hash-bound `external_record_ref`, `external_status`, `observed_at`, `source_hash`, `mapping_version`, `authority_basis`, …), the `ExternalReferenceObservationReceipt` minimum evidence, binding policy + promotion-gate hooks, source-authority/freshness, privacy rendering, and export/delete/recovery distinguishing ICN-reference lifecycle from external-source lifecycle. Observe-only; no settlement/payment/connector; implements nothing
- [governed-bridge-steward-review.md](spec/governed-bridge-steward-review.md) - Governed bridge steward review surface (draft spec / steward review surface, #2369) — the **minimum** steward-facing review surface a governed bridge must present before turning a dry-run proposal into real custody writes; the "Steward review surface (#2369)" that `BridgeReviewDecisionReceipt` and the binding's `steward_review_surface` field point at, modeled as a Steward Cockpit affordance. Defines minimum review inputs (dry-run plan hash, opaque source-record refs, field classes, proposed targets/receipts, policy blocks, gate fields, external observations, export/delete path), required decisions (approve/reject/hold/block/discard/request-re-observation/request-reclassification/request-member-consent-review) each tied to a receipt, role-display-vs-verifiable-authority-proof, per-`(record, field)` reviewed-set binding (never bare field names, no silent gaps), care/accessibility need-to-know boundaries, publication/consent policy-block boundaries, derived-action-card visibility, and the separate future member consent review. Review-surface planning only; implements nothing
- [governed bridge conformance fixtures (review-coverage-v0)](../tools/bridge-conformance/review-coverage-v0/README.md) - First machine-checkable conformance slice for the six governed-bridge specs — a **fake** fixture (binding + dry-run + steward-review + expected-receipts) plus `tools/validate-governed-bridge-conformance.py`, proving the minimum chain `binding → dry-run → steward review → expected receipts`: per-`(record, field)` coverage (no bare field names, no silent gaps, no duplicates), reviewer authority proof required, dry-run writes nothing, `ArtifactReceipt` ≠ `ArtifactRegistrationReceipt`, action cards not write targets, external references observe-only. Fixture/tooling only; no real data, no connector, no runtime import
- [compute-placement-policy.md](spec/compute-placement-policy.md) - Compute placement policy contract (seven placement classes, decision inputs and outputs, fallback behavior, boundary rules, dashboard and member-shell rendering) sitting between ADR-0030 (workload manifest) and ADR-0031 (commons admission and settlement); fixes scope vocabulary (LocalDomain not Coop), execution vs capacity vocabulary (execution budget vs fuel vs capacity vs allocation), and settlement vs payment vocabulary
- [network-anti-entropy-proof-loops.md](spec/network-anti-entropy-proof-loops.md) - Network anti-entropy proof-loop contract (eight-phase institutional evidence loop, thirteen forward-direction proof-artifact identifiers, eighteen divergence classes, ten boundary rules, steward / member surface vocabularies, three fixture-first proof-loop slices) beneath compute placement, storage durability, artifact registry, receipt clearing, and federation settlement finality; anchors against existing icn-gossip + icn-core anti-entropy primitives without redefining them
- [member-shell-v0.md](spec/member-shell-v0.md) - Member shell v0 — the primary participation surface (mobile-first, offline-tolerant, accessibility-first, plain-language-first). Defines five boundary lines, ten design principles, ten-surface information architecture, the ActionCard rendering contract over ADR-0027, the ten-step signing flow with reversibility / privacy / sync warnings, three-tier receipt rendering, offline / draft-intent behavior, ScopedVault opaque-reference affordances, the twelve-category accessibility gate, a closed v0 member-facing status vocabulary, an eighteen-row failure / safety table, and three fixture-first dogfood slices; no new endpoints, no platform decision, no native-app implementation
- [member-shell-i18n-v0.md](spec/member-shell-i18n-v0.md) - Member shell v0 i18n seam — the language-modularity contract for the reference client (icn#2042): locale-keyed message catalog, `t()` fallback order, locale resolution (`?lang=` → `navigator.language` → `en`), `lang`/`dir` + RTL, a pseudo-locale coverage test, and the translation-pending rule. Dependency-free; adding a language is a catalog entry, not a code change. Non-goals: real translations, server-side locale negotiation
- [steward-cockpit-v0.md](spec/steward-cockpit-v0.md) - Steward cockpit v0 — the operator-facing civic-infrastructure surface for node and domain stewards (the operator/steward complement of the member shell). Consumes verbatim the 9-field anti-entropy cockpit surface from #1829 and the 14-field operator/steward dashboard from #1826. Defines six boundary lines (vs member shell, vs node operator civic-role surface, vs public website, vs institution-package skin, vs backend/runtime, vs surveillance/admin-control panel), ten v0 design principles led by stewardship-not-domination, twelve cockpit surfaces, fourteen operator action-card scenarios, per-surface rendering contracts, a closed v0 operator-facing status vocabulary plus a verbatim member-impact summary mapping into the member-shell sync vocabulary, a twenty-row failure / safety table, and three fixture-first dogfood slices; no new endpoints, no frontend technology decision, no surveillance console, no private-data preview, no production-dashboard claim
- [icn-civic-shell-v0.md](spec/icn-civic-shell-v0.md) - ICN Civic Shell v0 — composition contract that ties the public website (truth boundary per ADR-0032 / ADR-0033), the member shell (#1830), the steward cockpit (#1831), the no-CLI organizer / member workflow (#1724 / #1726), the forge / auth-bridge / service-hosting / sovereign-forge models, and current operational reality into a single top-level public-plus-logged-in institutional shell. The first draft used the rejected `ICN Headquarters` metaphor; the v0 name is `ICN Civic Shell`. The Civic Shell composes existing ICN surfaces; it does not supersede the Member Shell, the Steward Cockpit, the public website, the service-hosting model, or the auth-bridge model. Defines public exterior (maturity-banded status / development updates / public roadmap / service-health posture / public forge window), logged-in interior (identity / active domain / active role / authority scope plus dashboards composed from existing surfaces), domain-and-route doctrine (`intercooperative.network` canonical public identity/truth vs `icn.zone` short operational/access/discovery — no `icn.zone` routes claimed live today; `ICN-PRIVATE` / `ICN-EDGE` segmentation framed as current/planned operational context, not ICN product doctrine), a ten-room model, notifications and action-card boundaries (preserves ADR-0027), status / proof labels anchored to #1796, and follow-up issue suggestions (not opened by this PR). No app implementation, no new endpoint, no auth implementation, no Keycloak / Forgejo / Matrix deployment, no n8n workflow build, no DNS / K3s / VLAN / network mutation, no public admin surfaces, no private data in repo, no Phase 2 completion claim, no formal NYCN pilot claim, no live-federation claim, no production-readiness claim, no replacement of existing member-shell or steward-cockpit specs. network-ops was not read locally; operator-provided context only.
- [community-proof-spine-0.1.md](spec/community-proof-spine-0.1.md) - Community Proof Spine 0.1 (icn#2084) — the smallest honest, fixture/dev proof that the RFC-0018 entity-authority spine (`require_entity_access`, #2079 / ADR-0035) can carry a community-shaped civic action end to end (belonging → standing → authority → action → ADR-0026-style blake3 receipt → recompute-verify → plain-language explanation). Models the community as an `icn-entity` `Community` entity; does NOT reconcile the `icn-community` crate. Runtime proof in `icn-gateway` (steward allowed, plain member denied, receipt tamper detected) + a `?mode=demo&set=community` member-shell fixture mirror. Boundary: no new endpoint, no new persisted receipt type, no new `EntityAction` variant, no enforcement change, no kernel change; Meaning Firewall preserved; fixture/dev only

---

## 📖 Reference Documentation

Technical reference materials, APIs, and configuration.

### API Reference (`reference/api/`)

REST API, WebSocket, and SDK documentation:

- [README.md](reference/api/README.md) - API documentation index
- [API_REFERENCE.md](reference/api/API_REFERENCE.md) - Complete API reference
- [api-versioning.md](reference/api/api-versioning.md) - API versioning strategy
- [topic-subscriptions-api.md](reference/api/topic-subscriptions-api.md) - Subscription API
- Also see: [api/OPENAPI.md](api/OPENAPI.md) and [api/README.md](api/README.md)

### Institutional Reference (`reference/`)

- [ccl-charter-templates.md](reference/ccl-charter-templates.md) - Starter charters for federations, cooperatives, communities, and multi-stakeholder co-ops
- [governance-playbook.md](reference/governance-playbook.md) - Common governance decision patterns with API examples
- [entity-topology-patterns.md](reference/entity-topology-patterns.md) - Organizational structure patterns (flat co-op through nested federation)

### Configuration Reference (`reference/config/`)

Configuration files and environment settings:

- [README.md](reference/config/README.md) - Configuration documentation index
- [CONFIGURATION.md](reference/config/CONFIGURATION.md) - Configuration guide
- [identity-backend-configuration.md](reference/config/identity-backend-configuration.md) - Identity backends
- [trust-threshold-configuration.md](reference/config/trust-threshold-configuration.md) - Trust thresholds

### Contract notes (`contracts/`)

- [schema-id-audit.md](contracts/schema-id-audit.md) - Audit-only record of every JSON schema `$id` under `docs/contracts/`, with per-schema keep / migrate / investigate recommendations and migration safety rules
- [preview-review.md](contracts/preview-review.md) - Read-only / read-model contract for the human review boundary between source material and any subsequent action (`urn:icn:contract:preview-review:v1`); not a mutation API
- [pending-publish-summary.md](contracts/pending-publish-summary.md) - Read-only row-level read-model for pending-publish summaries (`urn:icn:contract:pending-publish-summary:v1`); composed body for `preview_kind: pending_publish_summary`, does not replace preview-review; not a mutation API

### Other References

- [glossary.md](glossary.md) - Terminology and definitions
- [examples/policies/README.md](examples/policies/README.md) - Policy examples

### Project Index (`reference/project-index/`)

Show-ready orientation layer — routes outside readers, contributors, and agents to the right canonical doc, source tree, or external URL. Defers to [STATE.md](STATE.md) and [PHASE_PROGRESS.md](PHASE_PROGRESS.md) for current truth.

- [README.md](reference/project-index/README.md) - Project-index orientation and "Start here by role"
- [current-truth-map.md](reference/project-index/current-truth-map.md) - What is real now, what is not, open gates and risks
- [source-tree-map.md](reference/project-index/source-tree-map.md) - Top-level repo surfaces and what each is for
- [rust-workspace-map.md](reference/project-index/rust-workspace-map.md) - The `icn/` Rust workspace grouped by rough layer
- [docs-control-map.md](reference/project-index/docs-control-map.md) - INDEX, registry, validator, truth classes, how to add a doc
- [runtime-surface-map.md](reference/project-index/runtime-surface-map.md) - Real runtime surfaces (member standing, action cards, completion-receipt retrieval)
- [ci-ops-deploy-map.md](reference/project-index/ci-ops-deploy-map.md) - Workflows, deploy paths, K3s smoke runbooks
- [show-readiness-map.md](reference/project-index/show-readiness-map.md) - What can be shown now, what should not be shown as finished, red lines
- [proof-level-taxonomy-capability-matrix.md](reference/project-index/proof-level-taxonomy-capability-matrix.md) - Proof-level taxonomy (L0-L8) and a capability matrix for the organizer-rehearsal path
- [invariants-catalog.md](reference/project-index/invariants-catalog.md) - Source-linked index of the four canonical invariant families (index only; canonical sources authoritative); machine-readable companion `invariants-catalog.toml`

---

## 📚 User & Developer Guides

Practical guides for using and building with ICN.

### Institutional Guides (`guides/`)

- [institution-setup.md](guides/institution-setup.md) - How to create an institution on ICN (federation, cooperative, or community)
- [onboarding-runbook.md](guides/onboarding-runbook.md) - How to bring real humans into an ICN institution

### User Guides (`guides/user/`)

End-user documentation:

- [README.md](guides/user/README.md) - User guide index
- [WHY_ICN_HANDOUT.md](guides/user/WHY_ICN_HANDOUT.md) - ICN value proposition
- [cooperative-setup-guide.md](guides/user/cooperative-setup-guide.md) - Setting up a cooperative

### Developer Guides (`guides/developer/`)

Technical guides for contributors:

- [README.md](guides/developer/README.md) - Developer guide index
- [claude-code-plugin.md](guides/developer/claude-code-plugin.md) - ICN Agent Pack Claude Code plugin (portable MCP launch, skills/agents, validation)
- [agent-context-spine.md](guides/developer/agent-context-spine.md) - Agent Context Spine v0: generated, non-canonical, evidence-grounded repo orientation map (+ `icn_ops_agent_context_spine` MCP tool)
- [agent-context-spine-plan.md](guides/developer/agent-context-spine-plan.md) - Agent Context Spine design, node/edge model, and PR sequence
- [DEV_ENVIRONMENT.md](guides/developer/DEV_ENVIRONMENT.md) - Development environment setup
- [DOCUMENTATION_STYLE.md](guides/developer/DOCUMENTATION_STYLE.md) - Documentation conventions
- [i18n-guide.md](guides/developer/i18n-guide.md) - Internationalization guide

### Operations Guides (`guides/operations/`)

Deployment and operations documentation:

- [README.md](guides/operations/README.md) - Operations guide index
- [operations-guide.md](guides/operations/operations-guide.md) - General operations
- [backup-and-recovery.md](guides/operations/backup-and-recovery.md) - Backup procedures
- [replication-operations.md](guides/operations/replication-operations.md) - Storage replication
- [pilot-smoke.md](guides/operations/pilot-smoke.md) - Deterministic pilot linkage smoke runbook
- [troubleshooting.md](guides/operations/troubleshooting.md) - Common issues and solutions

### Quick References

- [FAQ.md](guides/FAQ.md) - Frequently asked questions
- [QUICK_REFERENCE.md](guides/QUICK_REFERENCE.md) - Command cheat sheet

---

## 🛠️ Development Documentation

Internal development resources, testing, and CI/CD.

### Development Process (`development/`)

- [README.md](development/README.md) - Development documentation index

**Sprints (`development/sprints/`):**
- [README.md](development/sprints/README.md) - Sprint planning and tracking
- Sprint completion records

**Testing (`development/testing/`):**
- [README.md](development/testing/README.md) - Testing documentation index
- Integration and E2E testing guides
- Mobile app testing procedures

### CI/CD & Automation (`ci/`)

Continuous integration and delivery:

- [GATE_RATCHET_PLAN.md](ci/GATE_RATCHET_PLAN.md) - CI check graduation schedule
- CI configuration and status reports

### Performance (`performance/`)

- [README.md](performance/README.md) - Performance documentation index
- Benchmarks and optimization guides

---

## 🔒 Security & Compliance

Security documentation, threat models, and audit reports.

These links include historical reports, current scanning notes, and point-in-time triage records. They do not by themselves establish current production readiness, vulnerability remediation status, deployment approval, or security hardening completion.

### Security Documentation (`security/`)

**Historical security assessments and references:**
- [FINAL_SECURITY_STATUS.md](security/FINAL_SECURITY_STATUS.md) - Historical assessment snapshot; not current readiness evidence
- [COMPREHENSIVE_SECURITY_IMPROVEMENTS.md](security/COMPREHENSIVE_SECURITY_IMPROVEMENTS.md) - Historical improvements report
- [SECURITY_FIXES_2025-12-18.md](security/SECURITY_FIXES_2025-12-18.md) - Historical fix record; current status requires verification
- [SECURITY_TESTING_GUIDE.md](security/SECURITY_TESTING_GUIDE.md) - Testing procedures
- [production-hardening.md](security/production-hardening.md) - Historical hardening reference

**Threat Models & Audits:**
- [threat-model.md](security/threat-model.md) - Comprehensive threat analysis
- [security-roadmap.md](security/security-roadmap.md) - Security roadmap
- [SECURITY_AUDIT_REPORT.md](security/SECURITY_AUDIT_REPORT.md) - Audit findings
- [SECURITY_AUDIT_RESULTS.md](security/SECURITY_AUDIT_RESULTS.md) - Audit results
- [codeql-alert-triage-2026-06-29.md](security/codeql-alert-triage-2026-06-29.md) - Point-in-time static triage of CodeQL alerts #100 and #101
- [codeql-gossip-nonce-triage-2026-06-29.md](security/codeql-gossip-nonce-triage-2026-06-29.md) - Point-in-time static triage of gossip nonce alerts #30–#35
- [codeql-triage-closeout-2026-06-29.md](security/codeql-triage-closeout-2026-06-29.md) - Point-in-time CodeQL inventory and maintainer disposition checklist

**SDIS Security:**
- [SDIS_THREAT_MODEL.md](security/SDIS_THREAT_MODEL.md) - SDIS-specific threats
- [SDIS_CRYPTO_REVIEW.md](security/SDIS_CRYPTO_REVIEW.md) - Cryptographic review
- [SDIS_AUDIT_CHECKLIST.md](security/SDIS_AUDIT_CHECKLIST.md) - SDIS audit checklist

**Specialized Topics:**
- [TOFU_SECURITY_MODEL.md](security/TOFU_SECURITY_MODEL.md) - Trust-On-First-Use model
- [GATEWAY_CSP.md](security/GATEWAY_CSP.md) - Content Security Policy
- [SECRET_MANAGEMENT.md](security/SECRET_MANAGEMENT.md) - Secret management practices
- [EDUCATIONAL_GUIDE_SECURITY_FIXES.md](security/EDUCATIONAL_GUIDE_SECURITY_FIXES.md) - Learning resource

### SDIS Documentation (`sdis/`)

Sovereign Digital Identity System:

- [SDIS_SYSTEM.md](sdis/SDIS_SYSTEM.md) - System overview
- [SDIS_STATUS.md](sdis/SDIS_STATUS.md) - Implementation status
- [SDIS_IMPLEMENTATION_PLAN.md](sdis/SDIS_IMPLEMENTATION_PLAN.md) - Implementation roadmap
- [SDIS_IMPLEMENTATION_COMPLETE.md](sdis/SDIS_IMPLEMENTATION_COMPLETE.md) - Completion report
- [SDIS_QUICK_START.md](sdis/SDIS_QUICK_START.md) - Quick start guide
- [SDIS_USER_GUIDE.md](sdis/SDIS_USER_GUIDE.md) - User guide
- [SDIS_API_GUIDE.md](sdis/SDIS_API_GUIDE.md) - API documentation
- [SDIS_BUILD_PLAN.md](sdis/SDIS_BUILD_PLAN.md) - Build planning
- [SDIS_STEWARD_ROADMAP.md](sdis/SDIS_STEWARD_ROADMAP.md) - Steward network roadmap
- [SDIS_DEPLOYMENT_STATUS.md](sdis/SDIS_DEPLOYMENT_STATUS.md) - Deployment status

---

## 🎯 Specialized Topics

### Onboarding (`onboarding/`)

Contributor onboarding materials:

- [README.md](onboarding/README.md) - Onboarding program overview
- [manual.md](onboarding/manual.md) - Complete onboarding manual
- [syllabus.md](onboarding/syllabus.md) - Learning syllabus
- [reading-map.md](onboarding/reading-map.md) - Documentation reading order
- [patterns.md](onboarding/patterns.md) - Code patterns guide
- Plus modules, tracks, and workshops

### Deployment & Operations

**Deployment Guides (`deployment/`):**
- Deployment configuration and guides

**Operations (`operations/`):**
- [README.md](operations/README.md) - Operations documentation
- Monitoring, runbooks, and operational procedures

**Ops runbooks (canonical: `guides/operations/runbooks/`):**
- [Runbook index](guides/operations/runbooks/README.md) — compatibility stub: [ops/runbooks/README.md](ops/runbooks/README.md)
- Incident response procedures

### Mobile & Observability

**Mobile (`mobile/`):**
- Mobile app documentation

**Observability (`observability/`):**
- Metrics, logging, and tracing

### Pilot Outreach (`pilots/`)

Organizer-facing pilot materials. Honest scope-of-now framing; not pitch decks. Defer to [STATE.md](STATE.md) and [PHASE_PROGRESS.md](PHASE_PROGRESS.md) for current truth.

- [pilot-proposal-template.md](pilots/pilot-proposal-template.md) - Template for approaching potential pilot communities
- [hosted-approach.md](pilots/hosted-approach.md) - Approach for hosted cooperative pilot deployments
- [decision_registry_treasury_vote.md](pilots/decision_registry_treasury_vote.md) - Economic receipt chain implementation in pilot

### Pilot Programs (`internal/pilots/`)

Real-world pilot deployment documentation:

- [pilot-playbook.md](internal/pilots/pilot-playbook.md) - Pilot deployment guide
- [pilot-coordinator-guide.md](internal/pilots/pilot-coordinator-guide.md) - Coordinator handbook
- [pilot-readiness-gaps.md](internal/pilots/pilot-readiness-gaps.md) - Readiness assessment
- [pilot-limitations.md](internal/pilots/pilot-limitations.md) - Known limitations

---

## 🔬 Internal Documentation

Internal planning, status tracking, and project management.

### Internal Overview (`internal/`)

- [README.md](internal/README.md) - Internal documentation index
- [legal-considerations.md](internal/legal-considerations.md) - Legal notes

### Status Tracking (`internal/status/`)

Active project status and gap analyses:

- [GAP_ANALYSIS.md](internal/status/GAP_ANALYSIS.md) - Current gaps
- [gap-analysis.md](internal/status/gap-analysis.md) - Gap tracking
- [strategic-gap-analysis.md](internal/status/strategic-gap-analysis.md) - Strategic gaps
- [vision-implementation-gap.md](internal/status/vision-implementation-gap.md) - Vision gaps
- [multi-device-status.md](internal/status/multi-device-status.md) - Multi-device status

### Planning Documents (`planning/`)

Current project planning and analysis:

- ~~FORWARD_PLAN_2026-03.md~~ — archived to `archive/2026/FORWARD_PLAN_20260312.md`
- [icn-crate-reference.md](planning/icn-crate-reference.md) - 38-crate inventory with roles and modules
- [icn-ecosystem-map.md](planning/icn-ecosystem-map.md) - Entity hierarchy, surfaces, funnels, flywheel
- [icn-vertical-slice-assessment.md](planning/icn-vertical-slice-assessment.md) - Gateway endpoint inventory and gap analysis
- [icn-demo-one-pager.md](planning/icn-demo-one-pager.md) - Demo summary for summit
- [icn-demo-presentation-prompt.md](planning/icn-demo-presentation-prompt.md) - Presentation generation prompt

### Strategy Documents (`strategy/`)

Public introduction materials (June 2026, claims bounded by the evidence map):

- [ICN_FOR_COOPERATIVE_MOVEMENT.md](strategy/ICN_FOR_COOPERATIVE_MOVEMENT.md) - Plain-English ICN introduction for cooperative developers, federation organizers, TA providers, and member-owners; honest tool comparisons, explicit non-claims
- [ICN_FOR_EVERYONE.md](strategy/ICN_FOR_EVERYONE.md) - General-public introduction starting from "what is a cooperative"; receipts as evidence records, not crypto; explicit non-claims
- [ICN_HANDBILL.md](strategy/ICN_HANDBILL.md) - One-page handbill with honest "where it actually stands" framing
- [ICN_HARD_QUESTIONS.md](strategy/ICN_HARD_QUESTIONS.md) - Hard questions answered directly (bad-answer/honest-answer format): production use, fixture vs live vs design-only, capture, surveillance, private data, regulation, bus factor
- [ICN_INTRODUCTION_EVIDENCE_MAP.md](strategy/ICN_INTRODUCTION_EVIDENCE_MAP.md) - Maps every introduction claim to verifiable merged artifacts and states what each does NOT prove

Strategic direction and gap analysis (March 2026):

- [ICN-Gap-Analysis-March-2026.md](strategy/ICN-Gap-Analysis-March-2026.md) - Subsystem-by-subsystem reality check
- [ICN-Sprint-March17.md](strategy/ICN-Sprint-March17.md) - Active 2-week sprint plan
- [ICN-Roadmap-Live.md](strategy/ICN-Roadmap-Live.md) - Grounded 90-day roadmap
- [ICN-Roadmap-Strategy.md](strategy/ICN-Roadmap-Strategy.md) - 18-month strategy
- [ADR-001-What-ICN-Is.md](strategy/ADR-001-What-ICN-Is.md) - Meaning Firewall formal definition
- [ICN-Definition.md](strategy/ICN-Definition.md) - What ICN is (multiple audiences)
- [ICN-Technical-Whitepaper.md](strategy/ICN-Technical-Whitepaper.md) - Architecture whitepaper
- [ICN-Pitch.md](strategy/ICN-Pitch.md) - Cooperative organizer pitch
- [ICN-Scenarios.md](strategy/ICN-Scenarios.md) - Six working scenarios with API calls
- [ICN-Evolution-Arc.md](strategy/ICN-Evolution-Arc.md) - Two-year project history
- [ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md](strategy/ICN_THURSDAY_MEETING_BRIEF_2026-05-21.md) - Meeting-prep truth packet for the 2026-05-21 cooperative-developer / formation-platform / ICN conversation (controls project-state facts and non-claims)
- [COOPERATIVE_CODEBASE_MEETING_BRIEF_2026-06-11.md](strategy/COOPERATIVE_CODEBASE_MEETING_BRIEF_2026-06-11.md) - Meeting brief for the 2026-06-11 Cooperative Codebase (forming Chicago tech co-op) call — learner-first posture, verified logistics, listening agenda, demo policy, pre-call checklist, capture grid; claims bounded by the hard-questions Q&A and evidence map
- [NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md](strategy/NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md) - Project-shaping record: NYCN/Summit as ICN's first institutional laboratory (recurring democratic institution, not an event demo)
- [ICN_NYCN_INSTITUTION_PACKAGE_BOUNDARY.md](strategy/ICN_NYCN_INSTITUTION_PACKAGE_BOUNDARY.md) - Short conceptual clarification: ICN is the substrate, NYCN/Summit is the first reference institution, institution packages are the reusable meaning layer in between. Four-layer table.
- [COOPERATIVE_FORMATION_PLATFORM_MEETING_PREP_2026-05-21.md](strategy/COOPERATIVE_FORMATION_PLATFORM_MEETING_PREP_2026-05-21.md) - **Thursday primary** — learner-first, Launch-first prep packet (public research on Launch / comp.coop / The Worker Place, correct posture, first-five-minutes script, Launch-specific questions, quiet non-claims, walkthrough beats, capture grid)
- [COOPERATIVE_FORMATION_PLATFORM_ONE_PAGE_AID_2026-05-21.md](strategy/COOPERATIVE_FORMATION_PLATFORM_ONE_PAGE_AID_2026-05-21.md) - 10-minute conversation cheat sheet (posture / Launch's shape / the seam / ICN in one breath / five questions / translation table / quiet boundaries / optional follow-up)
- [COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.md](strategy/COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.md) - Thursday quick screen-share deck (Markdown source, 11 slides, speaker notes inline, learner-first / Launch-first)
- [COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.html](strategy/COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.html) - Thursday quick screen-share deck, browser version (self-contained HTML, one slide per viewport, expandable speaker notes, print-friendly, no external assets)
- [COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.pdf](strategy/COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.pdf) - Thursday quick screen-share deck, PDF export of the browser version (11 pages, speaker notes included)
- [COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.pptx](strategy/COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.pptx) - Thursday quick screen-share deck, PowerPoint version (11 slides, speaker notes in the notes pane, minimal calm theme; rebuild with `node scripts/build-launch-deck-pptx.js`)
- [COOPERATIVE_FORMATION_PLATFORM_VISUAL_AID_2026-05-21.html](strategy/COOPERATIVE_FORMATION_PLATFORM_VISUAL_AID_2026-05-21.html) - Earlier in-meeting visual notes page (posture / Launch context / spine / translation / opener / 11-beat walkthrough with Today + ICN lines / branches / questions with note space / claims-not / capture grid). Open in any browser; print-friendly.
- [COOPERATIVE_FORMATION_PLATFORM_VISUAL_AID_2026-05-21.pdf](strategy/COOPERATIVE_FORMATION_PLATFORM_VISUAL_AID_2026-05-21.pdf) - Same visual notes page in PDF. Attachment-ready: email, print, or share.
- [NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md](strategy/NYCN_ORGANIZER_MEETING_PREP_2026-05-21.md) - **Secondary / backup variant** — NYCN/Summit organizer prep packet (formerly Thursday primary; now demoted to backup example surface)
- [NYCN_ORGANIZER_MEETING_ONE_PAGE_AID_2026-05-21.md](strategy/NYCN_ORGANIZER_MEETING_ONE_PAGE_AID_2026-05-21.md) - Secondary variant one-page aid (NYCN/Summit organizer surface)
- [NYCN_ORGANIZER_MEETING_VISUAL_AID_2026-05-21.html](strategy/NYCN_ORGANIZER_MEETING_VISUAL_AID_2026-05-21.html) - Secondary variant HTML visual notes (NYCN/Summit organizer surface)
- [NYCN_ORGANIZER_MEETING_VISUAL_AID_2026-05-21.pdf](strategy/NYCN_ORGANIZER_MEETING_VISUAL_AID_2026-05-21.pdf) - Secondary variant PDF (NYCN/Summit organizer surface)

### Current Status Reports (`status/`)

Live status reports and deployment verification:

- [icn-status-march-2026.md](status/icn-status-march-2026.md) - Current status report
- [DEPLOYMENT_VERIFICATION.md](status/DEPLOYMENT_VERIFICATION.md) - Deployment verification
- [MOBILE_APP_DEMO.md](status/MOBILE_APP_DEMO.md) - Mobile app demo
- Archived to `archive/2025/`: CURRENT_SYSTEM_STATUS, FINAL_CI_RESOLUTION, FINAL_DEMO_SUCCESS, FINAL_SESSION_STATUS, TESTS_FIXED_STATUS, 2025-12-25-sprint-status

### Architecture Decision Records (`adr/`)

Formal architectural decisions:

- [ADR-0010-app-topology.md](adr/ADR-0010-app-topology.md) - App topology decision
- Additional ADRs as numbered documents

### Templates (`templates/`)

Documentation and code templates for consistency.

### Reference Additions

- [reference/institution-package-bootstrap.md](reference/institution-package-bootstrap.md) - Generic institution-package bootstrap manifest, ordering, and current executor reality

### Vision Documents (`vision/`)

High-level vision and strategic direction.

### Session Handoffs & Dev Docs (`dev/`)

Session handoffs, development workflow docs, and worktree guidance:

- [WORKTREES.md](dev/WORKTREES.md) - Git worktree guide for parallel agent work
- [language-guide.md](dev/language-guide.md) - ICN language and terminology guide
- Session handoffs: `handoff-YYYY-MM-DD.md` files (latest is the current execution state)

### AI & Automation (`ai/`)

- [CODEX_WORKFLOW.md](ai/CODEX_WORKFLOW.md) - AI workflow and agent rules

---

## 📦 Archives

Historical documentation, completed phases, and superseded materials.

### Archive Structure (`archive/2025/`)

Organized by year, containing completed work and historical records:

- [README.md](archive/2025/README.md) - Archive index
- [SUMMARY.md](archive/2025/SUMMARY.md) - Year summary

**Key Archived Documents:**
- Phase completion reports (PHASE_18_COMPLETE.md, etc.)
- Historical deployment guides (KUBERNETES_DEPLOYMENT.md, PRODUCTION_DEPLOYMENT_GUIDE.md)
- System status snapshots (PROJECT_STATUS_2025-12-06.md, SYSTEM_GAPS_2025-12-06.md)
- Completed analyses (FOUNDATIONAL_REVIEW_2025-12-16.md, CODE_REVIEW_COMPLETE.md)
- Security audit resolutions (security-audit-resolution-2025-12-18.md, security-hardening-2025-12-18.md)
- Integration reports (GOVERNANCE-LEDGER-INTEGRATION-COMPLETE.md, GOVERNANCE-LEDGER-BUGS-FOUND.md)
- Evolution documents (COMMONS_EVOLUTION_SUMMARY.md, ICN_COMPLETE_ARCHITECTURE_SYNTHESIS.md)
- HSM/TPM documentation (hsm-tpm-roadmap.md, tpm-setup.md, tpm-implementation-plan.md)
- Bug reports and sprint plans
- Plus 20+ additional historical documents

**Migration Notes:** Documents are moved to archives when superseded, completed, or no longer actively referenced. See [REORGANIZATION_2026.md](REORGANIZATION_2026.md) for migration details.

---

## 🗂️ Documentation by Role

### For New Contributors
1. [GETTING_STARTED.md](GETTING_STARTED.md) - Quick start
2. [onboarding/README.md](onboarding/README.md) - Onboarding program
3. [guides/developer/README.md](guides/developer/README.md) - Developer guides
4. [ARCHITECTURE.md](ARCHITECTURE.md) - Architecture overview

### For Core Developers
1. [ARCHITECTURE.md](ARCHITECTURE.md) - Full architecture
2. [architecture/ARCHITECTURE_MAP.md](architecture/ARCHITECTURE_MAP.md) - Visual guide
3. [design/](design/) - Design documents
4. [development/testing/README.md](development/testing/README.md) - Testing guides

### For Operations Engineers
1. [guides/operations/README.md](guides/operations/README.md) - Operations guides
2. [security/FINAL_SECURITY_STATUS.md](security/FINAL_SECURITY_STATUS.md) - Security status
3. [deployment/](deployment/) - Deployment configs
4. [operations/README.md](operations/README.md) - Operational procedures

### For Security Engineers
1. [security/FINAL_SECURITY_STATUS.md](security/FINAL_SECURITY_STATUS.md) - Security overview
2. [security/threat-model.md](security/threat-model.md) - Threat analysis
3. [security/SECURITY_TESTING_GUIDE.md](security/SECURITY_TESTING_GUIDE.md) - Testing
4. [security/production-hardening.md](security/production-hardening.md) - Hardening

### For Product/Community
1. [guides/user/WHY_ICN_HANDOUT.md](guides/user/WHY_ICN_HANDOUT.md) - Value proposition
2. [design/economics/ECONOMIC_VISION.md](design/economics/ECONOMIC_VISION.md) - Economic vision
3. [design/MINIMAL-VIABLE-COOP.md](design/MINIMAL-VIABLE-COOP.md) - MVC specification
4. [internal/pilots/pilot-playbook.md](internal/pilots/pilot-playbook.md) - Pilot guide

---

## 📊 Documentation Statistics

- **Total Markdown Files**: 200+
- **Core Documentation**: 7 files
- **Architecture Docs**: 25+ files
- **Design Documents**: 35+ files
- **Reference Materials**: 15+ files
- **Guides**: 20+ files
- **Security Documents**: 20+ files
- **Internal/Status**: 15+ files
- **Archived Documents**: 30+ files
- **Total Documentation Size**: ~2MB+ of structured knowledge

---

## 🔍 Search Tips

### Find by Topic
- Use your IDE's search (Ctrl/Cmd+Shift+F) across `docs/` directory
- Check specific subdirectories for focused results
- Search [glossary.md](glossary.md) for terminology

### Find by Date
- Recent work: Check `status/` directory
- Historical: Browse `archive/2025/`
- Development timeline: See [PHASE_HISTORY.md](PHASE_HISTORY.md)

### Find by Component
- **Identity**: Search `identity` in architecture/ and design/
- **Trust**: Check `trust` in architecture/ and security/
- **Ledger**: Search `ledger` or `economic` in design/economics/
- **Governance**: Browse design/governance/
- **Federation**: Search `federation` in architecture/
- **SDIS**: Check sdis/ directory

---

## 📞 Getting Help

- **Documentation Issues**: File an issue on GitHub
- **Architecture Questions**: Check architecture/ or ask in dev channels
- **Operations Support**: See guides/operations/troubleshooting.md
- **Contributing**: Read CONTRIBUTING.md in project root
- **Security Issues**: Follow responsible disclosure process

---

## 🔄 Documentation Maintenance

### Keeping This Index Current

This index should be updated when:
- New major documents are added
- Directory structure changes
- Documents are archived
- Significant reorganizations occur

### Last Major Updates
- **2026-02-10**: Phase 2C reorganization (this version)
- **2026-01-17**: Previous organization
- See [REORGANIZATION_2026.md](REORGANIZATION_2026.md) for detailed change history

---

**Navigation**: [Top](#icn-documentation-index) | [Core Docs](#-core-documentation) | [Architecture](#️-architecture--design) | [Reference](#-reference-documentation) | [Guides](#-user--developer-guides) | [Archives](#-archives)
