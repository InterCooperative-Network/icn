---
Status: normative
Authority: spec (defines the design-level rendering contract for the ICN steward cockpit at v0 as the operator-facing complement of the member shell; consumes the 9-field anti-entropy cockpit surface from `docs/spec/network-anti-entropy-proof-loops.md`, the 14-field operator/steward dashboard from `docs/spec/compute-placement-policy.md`, the storage durability policy objects from `docs/spec/storage-durability-policies.md`, and the merged sibling specs; some clauses are explicitly forward-direction)
Canonical: no
Last Reviewed: 2026-05-15
---

# Steward Cockpit v0

> **Status: spec, work-in-progress.** Defines the ICN steward cockpit as the operator-facing civic-infrastructure surface for node and domain stewards at v0. It is the operator/steward complement of `docs/spec/member-shell-v0.md` (merged #1830). It does **not** redefine any merged sibling spec, does **not** implement a dashboard, does **not** define new endpoints, and does **not** specify a frontend technology choice. The shell renders plain participation status; the cockpit renders technical detail. The PR introducing this doc advances `#1795` without closing it.

<!-- truth-class: normative -->

## Purpose

A node steward, a domain steward, and a federation steward show up to keep institutional infrastructure honest. The cockpit is what they see. It answers, in technical detail:

1. **Is this node / domain healthy?**
2. **Are peers reachable?**
3. **Is gossip / federation syncing?**
4. **Are receipts persisting?**
5. **Are artifacts, replicas, backups, and scoped-vault references healthy?**
6. **Are compute jobs authorized, bounded, and reviewable?**
7. **Are accessibility and translation gates passing for member-facing surfaces?**
8. **Are privacy boundaries intact?**
9. **What requires steward action right now?**
10. **What evidence proves each claim?**

The cockpit makes stewardship legible **without turning stewards into rulers**. It surfaces obligations, warnings, receipts, authority basis, and repair paths. Every steward action runs through the same mandate / authority / receipt envelope as every other institutional action.

The cockpit is the consumer side of: the 9-field steward-cockpit surface in `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface" (merged #1829), the 14-field operator/steward dashboard in `docs/spec/compute-placement-policy.md` §"Operator / steward dashboard" (merged #1826), the storage durability policy objects in `docs/spec/storage-durability-policies.md` (merged #1823), the artifact registry / scoped-vault posture in `docs/spec/artifact-registry-and-scoped-vault.md` (merged #1824), the governed-service-binding lifecycle "Observe" state (merged #1822), the CCL policy registry adoption surface (merged #1821), the institutional-domain policy hooks (merged #1820), and the Stage 5 effect-dispatch evidence (merged #1819).

## Scope and non-goals

**In scope:**

- Six hard boundary lines distinguishing the cockpit from neighboring surfaces.
- Ten v0 design principles (stewardship-not-domination at the top).
- Twelve cockpit information-architecture surfaces.
- The Required Actions / Steward Action Card rendering contract for fourteen named operator scenarios.
- Per-surface rendering contracts that consume the merged sibling specs' cockpit / dashboard surfaces verbatim.
- A closed v0 operator-facing status vocabulary plus the member-impact summary surface.
- A failure / safety table covering twenty operator-side concerns.
- Three fixture-first dogfood slices.

**Not in scope (preserved out of this PR):**

- Not a dashboard frontend implementation. No HTML, CSS, JS, React component, Tauri / Electron / native shell, or any other UI technology decision.
- Not a backend endpoint specification. The cockpit consumes existing telemetry, governance, gateway, and proof surfaces; it does not extend them.
- Not a runtime, scheduler, gateway, or storage-backend change.
- Not a redefinition of any merged sibling spec, ADR, or `icn-obs` metric module.
- Not a redefinition of the steward / operator role. Role definitions belong to `DomainPolicy` (per `docs/spec/institutional-domain.md`) and to institution-package skins; the cockpit renders against whatever role the policy adopts.
- Not the node operator civic-role surface in Commons Shell (`#1613`). That is a separate operator-facing surface with its own contract; the boundary is documented below.
- Not a generic admin panel, fintech dashboard, balance / credit / token interface, payment surface, trading interface, surveillance console, or private-data viewer.
- Not a wallet, account-management dashboard, or any account-as-product framing.
- Not a production-dashboard claim. No clause of this spec is a claim that any operator is running an ICN-native cockpit against a real federation today.
- Not a K3s, DNS, Forgejo, deploy-script, or identity-bridge mutation.
- Not a NYCN-specific or named-partner-specific framing. Institution packages localize labels; the generic cockpit stays generic.
- Not closure of `#1795`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's acceptance criteria.

## Boundary lines

### Steward cockpit vs. member shell (`#1830`)

The member shell renders **plain participation status** using the closed seven-string sync vocabulary from `#1829` and the closed seven-string execution-scope vocabulary from `#1826`. The cockpit renders **technical detail** using the operator-facing names: `DivergenceEvidence` class names, peer DIDs, digest forms, `policy_version_id` content hashes, mandate references, replica counts, backup timestamps.

The same divergence event surfaces in both, in different vocabularies. The shell **must not** show cockpit technical detail (per `docs/spec/member-shell-v0.md` Boundary lines). The cockpit **must not** collapse to the shell's plain-language summary alone — every cockpit row shows technical detail under "details" even when the row's headline is plain-language for quick scanning.

A steward who is also a member sees both surfaces in their respective contexts. The cockpit is the steward-context view; the shell is the member-context view of the same institution.

### Steward cockpit vs. node operator civic-role surface (`#1613`)

`#1613`'s node operator civic-role surface in Commons Shell is a **separate** operator surface aimed at infrastructure operators of cooperative-operated nodes who are also performing civic roles inside the cooperative. The cockpit is the **institutional** stewardship surface: domain stewards, federation stewards, and any steward role a `DomainPolicy` adopts. The two surfaces may share underlying telemetry but their audience and authority context are distinct. This spec does not preempt `#1613`.

### Steward cockpit vs. public website

The website is the institution's outward-facing surface. The cockpit is the institution's inward-facing operator surface. The website has no operator authority; the cockpit consumes the steward's standing and mandates to render scoped actions.

### Steward cockpit vs. institution-package skin

Institution packages localize the cockpit with their own role labels ("steward," "treasurer-of-record," "node-keeper," etc.), ceremony markers, and visual identity. The generic v0 spec stays generic: it names the structural concepts (`LocalDomain`, `DomainPolicy`, `Mandate`, `AuthorityClass`, `Standing`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `PlacementDecision`, `BackupPolicy`, etc.), and lets packages add local nouns under "details" without losing the structural concept.

### Steward cockpit vs. backend / runtime

Per `docs/architecture/KERNEL_APP_SEPARATION.md`, the kernel never imports cockpit strings, layouts, or rendering rules. The cockpit is app-side rendering. It consumes existing surfaces (the `icn-obs` metrics modules; the governance proof / receipt surfaces; the gateway endpoints; the merged spec status vocabularies) and renders them. It does not define new wire formats, new endpoints, or new receipt classes.

### Steward cockpit vs. surveillance / admin-control panel

The cockpit is **not** a surveillance console. Operator actions are bounded by authority: every steward action that mutates institutional state requires a covering mandate, produces a receipt, and is challengeable per `DomainPolicy`. The cockpit does **not** surface private vault content. It does **not** offer "see all member activity" views. Member-impact summaries surface affected scope and counts, never identities or message bodies, unless the steward holds an explicit, receipted authority basis to see them.

## Design principles

Ten v0 principles, each load-bearing for what the cockpit is allowed to be:

1. **Stewardship, not domination.** Every cockpit action that mutates institutional state runs through the same mandate / authority / receipt envelope as every member action. The cockpit makes stewardship legible; it does not grant rulers.
2. **Proof before confidence.** The cockpit does not show "everything is fine" without evidence. A surface that lacks a recent matching `PeerSyncReport`, a recent matching `RoutingProof`, a fresh integrity-check receipt, or another freshness witness is rendered as **stale**, not as healthy.
3. **Degraded is visible.** `SyncDegradedStatus`, replica-below-policy, backup-overdue, restore-drill-missing, integrity-violation, and federation-sync-window-stale all render as visibly degraded. No surface renders "healthy" while a Boundary rule in a merged sibling spec is failing.
4. **Privacy posture, not private content.** The cockpit surfaces **posture** — that private overlays exist, that access grants are within policy, that export receipts are landing — without surfacing the **content** of private artifacts. Body bytes of `PrivateEvidence` artifacts (per `docs/spec/artifact-registry-and-scoped-vault.md`) never reach the cockpit rendering layer.
5. **Receipts explain operational state.** Every surface row that asserts a state of the world points to a receipt or an open evidence trail that justifies it. `Receipt` columns are first-class; "no receipt available yet" is itself a renderable state, never silently filled in.
6. **Required actions are explicit.** The Required Actions surface (the cockpit's "Today" view) names exactly which open conditions require steward attention. Authority basis, expected receipt, deadline, and reversibility window are surfaced before the steward acts.
7. **Authority basis is visible.** Every steward action that the cockpit offers names the mandate (per ADR-0014 / ADR-0019), the `DomainPolicy` clause, or the federation agreement that authorizes it. Actions without authority basis are rendered as visibly blocked.
8. **Local / domain / federation / commons scope is visible.** Every surface row carries an explicit scope tag. Cross-scope actions (e.g., a repair plan that involves federation peers) render the scope chain plainly.
9. **Member-impact summary is always present.** Every cockpit row that affects member-facing surfaces carries a one-line member-impact summary using the merged member-shell vocabulary from `docs/spec/member-shell-v0.md`. A steward looking at degraded sync sees both the technical detail and the seven-string member-facing status that members are seeing in their shell. This keeps the operator honest: when the cockpit shows degraded, the shell must already be showing degraded.
10. **No financial-product framing.** The cockpit never uses payment, currency, balance, wallet, token, crypto, blockchain, or timebank framing for ICN-native participation. The vocabulary discipline in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full. Settlement, position, obligation, allocation, receipt, and provenance are the operator-facing terms.

## Cockpit information architecture

The v0 cockpit offers twelve surfaces. The list is the **minimum**; institution-package skins may add, but a cockpit missing any of these is not v0.

| Surface | Purpose | Primary sources consumed |
|---|---|---|
| **Overview / Required Actions** | One-screen-at-a-glance: which conditions require steward action now, with priority. | All sub-surfaces feed this; steward required-action rendering analogs (per §"Required Actions / Steward Action Cards" below) drive the queue, pending a future steward required-action card contract. ADR-0027 member `ActionCard`s are not reused for operator-required-action scenarios. |
| **Node Status** | Daemon health for the local node. | `icn-obs` metrics; `icn-core` supervisor surfaces. |
| **Domain Status** | The `InstitutionalDomain` the steward is operating; policy version; standing read-model health. | `docs/spec/institutional-domain.md`; `/me/standing`; CCL policy registry. |
| **Network / Federation** | Peer reachability, sync state, divergence, repair. | `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface" (the nine fields verbatim, including Escalation status). |
| **Receipt Store** | Receipt persistence and read/write health. | Governance proof module; receipt-store backend (sibling of future ArtifactRegistry); ADR-0026 envelope. |
| **Storage / Artifacts / ScopedVault** | Storage class / custody class posture; artifact and replica health; vault posture without content. | `docs/spec/storage-durability-policies.md`; `docs/spec/artifact-registry-and-scoped-vault.md`. |
| **Governance / Process** | Proposals, decisions, mandates, effect dispatch evidence, policy adoption. | `docs/spec/effect-dispatch-contract.md`; `docs/spec/ccl-policy-registry.md`; `docs/spec/institutional-domain.md`. |
| **Compute / Commons** | Queued / running / completed jobs; placement decisions; admission decisions; settlement receipts. | `docs/spec/compute-placement-policy.md` §"Operator / steward dashboard" (the 14 fields verbatim); ADR-0030; ADR-0031; `docs/spec/federation-settlement-finality.md`. |
| **Participation Access** | Accessibility-gate status and translation / glossary posture for member-facing surfaces in this domain. | `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`; ADR-0028; `#1610`; `#1740`. |
| **Privacy Posture** | Private-overlay loaded / missing, vault posture, access-grant posture, export-receipt queue. | `docs/spec/artifact-registry-and-scoped-vault.md`; `#1792`; `#1767`. |
| **Backup / Export / Recovery** | `BackupPolicy` / `RecoveryPolicy` / `ArchivePolicy` / `IntegrityPolicy` posture; restore-test receipts. | `docs/spec/storage-durability-policies.md`. |
| **Warnings / Incidents / Repair** | Open `DivergenceEvidence`, open `RepairPlan`s, escalations to governance review, key-rotation prompts. | `docs/spec/network-anti-entropy-proof-loops.md`; ADR-0014; ADR-0019; key-rotation surfaces in `icn-gossip`. |

The cockpit may offer more surfaces; a v0-conformant cockpit offers at least these twelve.

## Required Actions / Steward Action Cards

**Schema status (forward-direction).** The member shell consumes ADR-0027's `ActionCard` schema (closed enums: `source_kind ∈ {proposal, meeting, action_item}` + two RFC-gated reserved values; `action_kind ∈ {vote, attend, complete}`). The fourteen operator scenarios named below — repair, backup, restore drill, key rotation, export review, stale peer, etc. — **cannot** be represented by that schema as it currently stands. ADR-0027 was not written to cover operator-required-action surfaces. This spec therefore **does not** claim the existing schema fits; it names the scenarios at design-level and **defers the schema question** to a follow-up `spec(contracts): define steward required-action card contract` that either (a) amends ADR-0027 with an operator-required-action superset, or (b) defines a separate `StewardRequiredActionCard` primitive alongside the member-facing `ActionCard`. Per this PR's non-claims, no schema, ADR, or wire format is introduced here.

Per `#1795` acceptance criteria, the cockpit defines steward-side required-action scenarios for at least the following operator situations. Each scenario is named, with its source class, its authority basis pattern, and its expected `RepairReceipt` or evidence outcome. The **rendering needs** below are spec-level; the **wire-stable record shape** that carries them is forward-direction.

| Operator scenario | Source class | Authority pattern | Expected outcome |
|---|---|---|---|
| Failed receipt write | Receipt-store telemetry | Steward of receipt-store custody | `RepairReceipt` of class "receipt-store recovery"; retry log + read-back verification. |
| Stale peer | `DivergenceEvidence` class "peer behind sync window" | `DomainPolicy` peer-management clause | `RepairReceipt` of class "anti-entropy fetch" once peer catches up; quarantine receipt if peer is dropped. |
| Degraded sync | `SyncDegradedStatus` aggregated | Sync-degradation governance clause | `RepairReceipt` once `QuorumSyncCheck` returns fresh. |
| Missing replica | `DivergenceEvidence` class "replica missing" | `ReplicationPolicy` re-replication authority | `RepairReceipt` recording re-replication outcome. |
| Backup overdue | `IntegrityPolicy` / `BackupPolicy` telemetry | Backup-policy authority clause | Successful `BackupPolicy` cycle receipt. |
| Restore drill due | `RecoveryPolicy` cadence | Recovery-drill authority clause | Restore-test receipt (per `docs/spec/storage-durability-policies.md`). |
| Compute output awaiting review | `PlacementDecision` with `ReviewRequiredActionCard` | Domain ratification clause | Forward `RatificationReceipt`. |
| Accessibility gate failed | Member-facing surface telemetry | Accessibility-gate authority | Failure removed from track; surface re-evaluated against the 12-category gate. |
| Missing translation / glossary review | `#1610` / `#1740` surfaces | Translation-review authority | Translation review receipt; fallback path documented. |
| Private overlay missing | `#1767` posture telemetry | Custody authority | Overlay restored or quarantine receipt. |
| Overbroad access grant | Access-receipt review | Privacy-policy review authority | Grant narrowed; revocation receipt produced. |
| Export receipt awaiting review | Export-receipt queue | Export-review authority | Reviewed export receipt or revocation. |
| Key rotation needed | Key-status telemetry (per `icn-gossip` key rotation module) | `DomainPolicy` key-rotation cadence | Key-rotation evidence + new key adoption receipt. |
| Policy conflict / challenge window open | Open challenge window on a `GovernanceDecisionReceipt` or `EffectDispatchEvidence` | Governance review authority | Decision affirmed, reversed, or amended per policy; corresponding receipt class. |

Each steward required-action row carries a **rendering analog** set — fields the cockpit needs in order to render the row honestly to a steward. These are **not** ADR-0027 `ActionCard` fields; ADR-0027 was written for member participation cards and its `source_kind` / `action_kind` enums do not extend to operator scenarios. The wire-stable record shape is forward-direction (see follow-up `spec(contracts): define steward required-action card contract`); until that lands, the cockpit may surface analogous concepts:

- **Title** — short technical label naming the operator scenario.
- **Summary** — one-line plain-technical description of what's happening.
- **Source** — which cockpit surface raised it (Receipt Store, Network / Federation, Storage / Artifacts / ScopedVault, Compute / Commons, etc.).
- **Authority basis** — the mandate, `DomainPolicy` clause, or federation agreement that authorizes a steward to act here.
- **Scope** — `LocalDomain` / `Federation` / `Commons` / peer-pair, per the corrected scope vocabulary.
- **Risk** — coarse severity (critical / high / medium / low; color-independent per ADR-0028).
- **Deadline** — when the action must be taken, when known; otherwise "no time pressure encoded."
- **Status** — current operator state from the closed v0 vocabulary in §"Status vocabulary."
- **Expected evidence** — the `RepairReceipt` / `EffectDispatchEvidence` / restore-test receipt / etc. that the action will produce on completion.
- **Action path** — the concrete next step (request governance review, run restore drill, re-replicate, rotate key, review export, etc.).
- **Member-impact summary** — verbatim from the member-shell vocabulary mapping (Design principle 9).

These are **rendering analogs**, not a schema definition. The cockpit renders them with operator-facing technical detail rather than member-facing plain language; the member shell does not show them at all (per §"Boundary lines" → "Member shell vs steward cockpit"). The forward-direction follow-up either amends ADR-0027 with an operator-required-action superset or defines a separate `StewardRequiredActionCard` primitive; this PR does neither.

## Node Status surface

Renders the local node's operational posture as observed via existing `icn-obs` metrics and `icn-core` supervisor state. Every field has a freshness timestamp; stale beyond a policy window renders as `stale`.

- **Daemon health** — running / stopped / restarting / crashed; uptime; last-restart cause if not graceful.
- **Version** — daemon version, git SHA if available, build profile.
- **Uptime** — wall-clock since last start.
- **Config status** — config file path; last-load timestamp; validation result (valid / warning / invalid).
- **Key status** — local DID; key state; last rotation timestamp; next rotation window per `DomainPolicy`.
- **Local storage health** — storage class breakdown (canonical, service state, blobs per `icn-kernel-api/src/storage.rs`); disk usage; integrity-policy verification posture.
- **Current mode** — fixture / devnet / K3s / live, labeled honestly. Fixtures and devnet runs are never rendered as live.
- **Evidence timestamp** — when the cockpit last refreshed this surface.

## Domain Status surface

Renders the `InstitutionalDomain` the steward is operating against.

- **InstitutionalDomain id** — the domain DID; the owning entity class (Cooperative / Community / Federation / Individual / other governed class, per `docs/spec/institutional-domain.md`).
- **Domain policy version** — the `policy_version_id` of the adopted `DomainPolicy` per `docs/spec/ccl-policy-registry.md`.
- **Standing read-model health** — last successful `/me/standing` regeneration; any divergence between regenerated standing and stored standing.
- **Active proposals / decisions** — count and link to the Governance / Process surface.
- **Adopted CCL policy versions** — by policy domain, with `policy_version_id` and adoption receipt link.
- **Service bindings** — list of active `GovernedServiceBinding`s (per merged #1822) with their lifecycle state, especially flagging any in "observe" with degradation.
- **Action-card generation posture** — `/me/action-cards` generation health; last generation timestamp; missing-source warnings (e.g., `signal_rule` / `obligation_lifecycle` source paths are inert pending `#1631` / `#1634`).
- **Member-facing readiness summary** — a one-line summary of whether the domain's member-facing surfaces (per `docs/spec/member-shell-v0.md`) currently render `Synced`, `Sync delayed`, `Review required`, or `Sync delayed / degraded` to members in this domain.

## Network / Federation surface

This surface **consumes** the 9-field cockpit surface from `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface" verbatim. Each open `DivergenceEvidence` (per the 18-class taxonomy) renders with:

1. **Affected scope** — `LocalDomain`, `Federation`, `Commons`, or peer-pair.
2. **State class** — one of the nine state classes (governance state, receipts, artifact metadata, scoped vault refs, storage replicas, compute receipts, settlement records, federation membership, CCL policy versions).
3. **Peers** — peer DIDs involved.
4. **Digest mismatch** — Bloom-filter set-difference, Merkle root inequality, vector-clock divergence, etc.
5. **Last successful proof** — when the last matching `PeerSyncReport` for this scope was recorded.
6. **Repair plan** — `RepairPlan` action, authority required, expected `RepairReceipt` class.
7. **Authority required** — the mandate or `DomainPolicy` clause the repair needs.
8. **Receipts / evidence** — links to `DivergenceEvidence`, `RepairPlan`, and any `RepairReceipt` already produced.
9. **Escalation status** — whether the divergence has escalated to governance review (unclassifiable, equivocation, or boundary-rule violations).

Each row carries the closed status terminology from `#1799` / `#1829` (direct / relayed / degraded / partitioned / stale / syncing / verified / unverified) and a member-impact summary using the merged member-shell sync vocabulary.

`QuorumSyncCheck` and `FederationSyncWindow` posture appear here when the domain participates in a federation: which checks are fresh, which are stale beyond the window, and which placement / settlement decisions are currently `RejectedByPolicy` for lack of fresh proof (per `docs/spec/compute-placement-policy.md` Boundary rule 6 and `docs/spec/network-anti-entropy-proof-loops.md` Boundary rule 6).

## Receipt Store surface

Renders persistent receipt posture.

- **Latest receipts** — by ADR-0026 / ADR-0025 receipt class: `GovernanceDecisionReceipt` (Layer 1), `ActionItemCompletionReceipt`, `MeetingAttendanceReceipt`, `ArtifactReceipt` (Layer 2), `InstitutionalEffectRecord` + `EffectDispatchEvidence` (Stage 5), future `RatificationReceipt` (when ADR-0025 / `#1818` follow-ups land it as a top-level class).
- **Failed writes** — receipt writes that did not persist; retry log; cause if known.
- **Read / write health** — read latency, write latency, error rate per `icn-obs` metrics.
- **Verification status** — most recent integrity verification per `IntegrityPolicy`.
- **Opaque receipt store posture** — for receipt stores backed by encrypted storage (`#1767`), posture indicators that do not reveal content.
- **Receipt class summary** — counts per class, retention horizon per `DomainPolicy` §"Receipt retention defaults."
- **Missing receipt warnings** — `DivergenceEvidence` class "missing receipt" rows raised against peers.
- **Evidence envelope references** — links to the Stage 5 `EffectDispatchEvidence` artifacts (per `docs/spec/effect-dispatch-contract.md`) the receipts belong to.
- **Forward-direction proof / evidence artifacts (not receipt classes).** Several merged sibling specs name forward-direction artifact identifiers that travel **inside existing receipt envelopes** (Stage 5 `EffectDispatchEvidence` or Layer 2 `ArtifactReceipt` per ADR-0026), **not** as new ADR-0026 receipt classes. From `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts (forward-direction names)": `RepairReceipt`, `RoutingProof`, `RedundancyProof` (and the broader anti-entropy proof-artifact set). From `docs/spec/compute-placement-policy.md` §"Candidate outputs": `PlacementFallbackReceipt` (an attachment on its parent `PlacementDecision`, not a new top-level receipt class — see also the Compute / Commons surface below for the placement-side rendering). The cockpit surfaces these as attachments on their parent evidence record; they do **not** appear in the "Latest receipts — by class" list above.

## Storage / Artifacts / ScopedVault surface

Consumes `docs/spec/storage-durability-policies.md` and `docs/spec/artifact-registry-and-scoped-vault.md`.

- **Storage class / custody class** — `Canonical`, `ServiceState`, `Blobs` (per `icn-kernel-api/src/storage.rs`); custody class breakdown.
- **Artifact registry health** — registry index health, `content_hash` verification posture, integration-point coverage.
- **Replica health** — replica count per `ReplicationPolicy.target_replicas`; flag any below target.
- **Backup / export / recovery status** — `BackupPolicy` cadence freshness; export-receipt queue; recovery-drill cadence.
- **Scoped vault health without content preview** — vault loaded / missing; `ScopedVault` reference count by privacy class; access-receipt queue. Body bytes never surface here.
- **Access / export receipt warnings** — access grants exceeding policy scope; export receipts awaiting review.
- **Private object reference health** — divergence class 16 ("private object reference mismatch without content disclosure") rows.
- **Redaction / evidence packet status** — `EvidencePacket` and `PrivateEvidence` artifact-class counts (per `docs/spec/artifact-registry-and-scoped-vault.md` artifact-class taxonomy); redaction posture without revealing content.

## Governance / Process surface

Consumes `docs/spec/effect-dispatch-contract.md`, `docs/spec/institutional-domain.md`, `docs/spec/ccl-policy-registry.md`.

- **Proposals** — open proposals scoped to the domain; deadline; current standing; vote tally if applicable.
- **Decisions** — recent `GovernanceDecisionReceipt`s; dispute-window status for each per `docs/spec/federation-settlement-finality.md` analogue.
- **Mandate / effect status** — open mandates per ADR-0014 / ADR-0019; `EffectManifest` and Stage 5 `EffectDispatchEvidence` per `docs/spec/effect-dispatch-contract.md`.
- **Effect dispatch evidence** — links to the Stage 5 evidence artifacts.
- **CCL policy registry versions** — adopted, draft, superseded versions per `docs/spec/ccl-policy-registry.md`.
- **Process-transition receipt classes where present** — coverage check against `docs/spec/effect-dispatch-contract.md` receipt-class summary.
- **Challenge / reversal windows** — open challenge windows on receipts that support disputes; deadlines; required authority class to act.

## Compute / Commons surface

This surface **consumes** the 14-field operator/steward dashboard from `docs/spec/compute-placement-policy.md` §"Operator / steward dashboard" verbatim:

1. **Placement decision** — one of the seven placement classes (`LocalOnly`, `DomainLocalPreferred`, `LocalDomainBound`, `FederationBound`, `CommonsEligible`, `ExternalCustodianRequired`, `RejectedByPolicy`).
2. **Runner / executor class** — one of the seven runtime classes from `docs/spec/governed-service-binding.md`.
3. **Scope** — `LocalDomain` and any wider scope the placement authorizes.
4. **Privacy class** — per the workload manifest (current code variants `Public` / `Member` / `NeedToKnow`; ADR-0030 names `Public` / `Encrypted` / `Sealed` — reconciliation tracked in `#1792`).
5. **Determinism class** — governance-grade or advisory.
6. **Resource envelope** — declared CPU / memory / storage / network shape.
7. **Execution budget** — the policy-facing name for `fuel_limit`. Compatibility note per `docs/spec/compute-placement-policy.md` §"Vocabulary boundaries": `FuelLimit` is the runtime field; `execution budget` is the cockpit-facing term; `capacity` is reserved for executor / node resource availability.
8. **Capacity fit** — whether the selected executor has capacity for the workload's resource envelope at the moment of placement.
9. **Allocation decision** — the governed permission to consume capacity, with the mandate ref.
10. **Federation / commons agreement ref** — when applicable.
11. **Output artifact ref** — when the workload has produced an output artifact (per `docs/spec/artifact-registry-and-scoped-vault.md`).
12. **Settlement receipt** — settlement receipt ref per ADR-0031 (not "payment receipt").
13. **Review requirement** — whether the output is awaiting human ratification.
14. **Failure / fallback reason** — when the placement fell back or was `RejectedByPolicy`, the reason and authority basis.

Rejected and fallback rows show the original preferred class, the chosen class (or rejection), and the policy clause invoked. `PlacementFallbackReceipt`s (per `docs/spec/compute-placement-policy.md` §"Candidate outputs") render as evidence attachments on their parent `PlacementDecision`. The Stage 5 envelope that carries the attachment is the `EffectDispatchEvidence` from `docs/spec/effect-dispatch-contract.md`.

## Participation Access surface

Consumes `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` and ADR-0028.

- **Accessibility gate results** — pass / fail per the twelve categories, per member-facing surface in this domain.
- **Missing translations** — surfaces or strings missing translation per the institution's adopted languages.
- **Glossary gaps** — terms with no glossary entry, per `#1610`.
- **Member-shell readiness** — overall readiness summary for `docs/spec/member-shell-v0.md` v0 conformance.
- **Low-bandwidth / offline readiness** — last successful low-bandwidth audit; cache freshness; offline-mode behavior verification posture.
- **Screen-reader / keyboard / color-independent readiness** — per the relevant categories from ADR-0028.

A failing accessibility gate row produces a Required Action card per the table above.

## Privacy Posture surface

Consumes merged `docs/spec/artifact-registry-and-scoped-vault.md` and forward-direction `#1792` / `#1767`.

- **Private overlay loaded / missing** — per `#1767` (encrypted distributed private-overlay storage).
- **Public package clean** — no private artifacts have leaked into the public artifact-registry index.
- **Vault health** — `ScopedVault` loaded; access-key custody healthy; integrity checks fresh.
- **Overbroad grants** — access grants that exceed `DomainPolicy` scope; flagged for review.
- **Export receipts awaiting review** — export receipts that produced data movement outside the domain; review-window deadlines.
- **Redaction / evidence status** — `EvidencePacket` and `PrivateEvidence` posture without surfacing content.
- **Hard rule:** no raw private content in cockpit. Body bytes of private artifacts never reach the rendering layer.

## Backup / Export / Recovery surface

Consumes `docs/spec/storage-durability-policies.md`.

- **Backup policy status** — `BackupPolicy` cadence freshness per spec; missed backups flagged.
- **Restore-test receipt status** — last successful restore-test receipt; cadence freshness per `RecoveryPolicy`.
- **Archive verification** — `ArchivePolicy` verification cadence; integrity-check posture.
- **Export authority** — open export-receipt queue; authority basis per export.
- **Recovery drill cadence** — when the next drill is due per `DomainPolicy`.
- **Locality / disclosure inheritance warnings** — any backup, replica, archive, or export that would broaden `DataLocality` or `privacy_class` is flagged before it can run (per `docs/spec/storage-durability-policies.md` §"Locality and privacy inheritance" and `docs/spec/network-anti-entropy-proof-loops.md` Boundary rule 4).

## Warnings / Incidents / Repair surface

Aggregates open warnings, active incidents, and repair flows across all surfaces. Distinct from the Overview / Required Actions surface: that one orders by required steward action; this one orders by warning / incident severity and life-cycle, and is the place to drill into the underlying evidence chain.

Each row renders:

- **Warning / incident id** — stable id derived from the source evidence (e.g., the `DivergenceEvidence` id, the failing `IntegrityPolicy` check id, the missing-restore-drill key). Not a member-facing id; technical detail.
- **Severity** — one of `critical` / `high` / `medium` / `low`, derived from the source class + the affected scope + the policy's tolerance window. Color-independent (per ADR-0028 category 4): glyph + label always present, color reinforces.
- **Affected scope** — `LocalDomain`, `Federation`, `Commons`, or peer-pair; same scope vocabulary as the Network / Federation surface.
- **Source surface** — which of the other eleven cockpit surfaces raised this row (Network / Federation, Storage / Artifacts / ScopedVault, Compute / Commons, etc.). One-click navigation to the originating surface.
- **Authority basis** — the mandate, `DomainPolicy` clause, or federation agreement that authorizes a steward to act on this row. If no current authority basis exists, the row renders as "review required" with a path to request the missing authority.
- **Member-impact summary** — verbatim from the member-shell vocabulary mapping (e.g., "Members see: Sync delayed / degraded"). Required for every row that has member-facing impact (Design principle 9; the v0-violation row in the failure / safety table enforces this).
- **Evidence / receipt refs** — links to the underlying `DivergenceEvidence`, `EffectDispatchEvidence`, `IntegrityPolicy` failure record, or other Stage 5 evidence per `docs/spec/effect-dispatch-contract.md`. Receipts (not raw bodies) are the path; private-artifact bodies never appear here.
- **Repair plan ref** — when a `RepairPlan` (per `docs/spec/network-anti-entropy-proof-loops.md`) has been produced; otherwise blank with "no plan yet — escalate."
- **Current state** — one of the closed v0 operator states (`healthy`, `degraded`, `syncing`, `stale`, `partitioned`, `relayed`, `verification pending`, `repair planned`, `repair applied`, `review required`, `blocked by policy`, `private content restricted`).
- **Steward action required** — the specific operator action this row needs (request governance review, run restore drill, re-replicate, rotate key, review export, etc.). When the required-action shape lands per the forward-direction follow-up named in §"Required Actions / Steward Action Cards," the row links to it; until then the cockpit renders the action as a plain technical instruction with authority basis.
- **Escalation / challenge path** — when the policy provides one (challenge window, governance review queue, federation arbiter). Deadline shown when applicable.
- **Last updated** — freshness timestamp. Stale beyond a policy window renders the row as `stale` regardless of the underlying state class.

The surface obeys the same boundary rules as the rest of the cockpit:

- No private content preview. Body bytes of `PrivateEvidence` artifacts never reach this surface; existence + scope + access path only.
- No surveillance aggregation. The surface aggregates by scope and severity, not by member identity. A row may name affected scope and counts; it does not enumerate member DIDs unless the steward holds an explicit receipted authority basis to see them and the policy has authorized that surfacing.
- Degraded state must match member-shell status. Per Design principle 9, the Member-impact summary on every row must be honest; a cockpit row that says "degraded" while the member shell is showing "Synced" is a v0 violation per the failure / safety table.
- Every repair action needs an authority basis and an evidence path. The row's Authority basis and Evidence / receipt refs columns are mandatory; an action lacking either is rendered as visibly blocked.

## Status vocabulary

The cockpit uses operator-facing technical terms. Each operator term carries a member-impact summary that maps to the closed seven-string member-shell sync vocabulary from `docs/spec/network-anti-entropy-proof-loops.md` and the closed seven-string member-shell execution-scope vocabulary from `docs/spec/compute-placement-policy.md`.

### Operator states (closed v0 set)

- **healthy** — fresh evidence; all relevant freshness windows satisfied.
- **degraded** — partial evidence; some freshness windows stale or some metrics outside policy bounds.
- **syncing** — anti-entropy probe in flight; pending convergence.
- **stale** — last successful evidence is beyond freshness window.
- **partitioned** — peer unreachable; no fresh `PeerSyncReport`.
- **relayed** — direct connection unavailable; relayed path in use; relay is not authority.
- **verification pending** — `DivergenceEvidence` open; classification result pending.
- **repair planned** — `RepairPlan` produced; awaiting authorization or execution.
- **repair applied** — `RepairReceipt` recorded with `EffectOutcome::Applied`.
- **review required** — escalation to governance review (unclassifiable, equivocation, boundary-rule violation, or advisory output awaiting ratification).
- **blocked by policy** — action available in principle but blocked by current `DomainPolicy` (e.g., placement `RejectedByPolicy`, federation agreement absent).
- **private content restricted** — surface intentionally opaque due to privacy / custody rule; existence + scope + access path are shown, body is not.

### Member-impact summary mapping

Every operator-state surface row that has member impact carries a one-line member-impact line using the merged member-shell sync vocabulary:

| Operator state | Member-impact summary (verbatim from `#1829` member shell) |
|---|---|
| healthy | "Members see: Synced." |
| degraded | "Members see: Sync delayed." |
| syncing | "Members see: Some records are being verified." |
| stale (within grace) | "Members see: Sync delayed." |
| stale (beyond grace) | "Members see: Sync delayed / degraded." |
| partitioned | "Members see: Some records are being verified." |
| repair planned | "Members see: Action paused until records sync." |
| repair applied | "Members see: Receipt available." |
| review required | "Members see: Review required." |
| blocked by policy | "Members see: Action paused until records sync." |

This mapping is **honest**: when the cockpit shows `degraded`, the member shell must already be showing `Sync delayed`. The "dashboard says healthy while member shell says degraded" row in the failure / safety table below is a v0 violation.

## Failure and safety table

| Failure | Where it surfaces | Disposition |
|---|---|---|
| Receipt store unavailable | Receipt Store + Required Actions | Required Action card "failed receipt write." Member-impact: "Members see: Sync delayed / degraded." Cockpit renders the underlying error class and the retry plan. |
| Receipt write failed | Receipt Store + Required Actions | Required Action card per the table above; underlying class + retry log surfaced. |
| Anti-entropy divergence | Network / Federation | `DivergenceEvidence` row rendered with the 8 cockpit fields per `#1829`; member-impact summary attached. |
| Stale peer | Network / Federation + Required Actions | Required Action card "stale peer"; `DivergenceEvidence` class "peer behind sync window." |
| Partition / rejoin incomplete | Network / Federation | Status: `partitioned` then `syncing`; rejoin progress visible; member-impact summary attached. |
| Replica below policy | Storage / Artifacts / ScopedVault + Required Actions | Required Action card "missing replica"; `DivergenceEvidence` class "replica missing" or "replica lag." |
| Backup overdue | Backup / Export / Recovery + Required Actions | Required Action card "backup overdue"; `BackupPolicy` cadence telemetry surfaced. |
| Restore drill failed | Backup / Export / Recovery + Required Actions | Required Action card "restore drill due"; failure detail; escalation per `RecoveryPolicy`. |
| Private overlay missing | Privacy Posture + Required Actions | Required Action card "private overlay missing"; surfaces existence of the gap without exposing what is gated behind it. |
| ScopedVault content exposed in cockpit | Any surface | v0 violation. Bodies of private artifacts never reach the rendering layer. If detected, drop and log a safety failure. |
| Export without receipt | Backup / Export / Recovery + Privacy Posture | Hard rule: export requires a receipt. An export attempt without a receipt is rejected; cockpit logs the rejection and surfaces a Required Action. |
| Compute job lacks authority basis | Compute / Commons + Required Actions | Per Boundary rule 3 in `docs/spec/compute-placement-policy.md`: `RejectedByPolicy`. Cockpit surfaces the missing mandate. |
| Compute output not reviewed | Compute / Commons + Required Actions | Required Action card "compute output awaiting review"; advisory output renders `Review required` member-impact. |
| Federation sync window expired | Network / Federation + Compute / Commons | Per `docs/spec/network-anti-entropy-proof-loops.md` Boundary rule 6 and `docs/spec/compute-placement-policy.md` Boundary rule 6: federation / commons placement renders `RejectedByPolicy` until `QuorumSyncCheck` refreshes. |
| Settlement finality claimed without proof | Compute / Commons | Per `docs/spec/network-anti-entropy-proof-loops.md` Boundary rule 7: finality is not claimed; dispute window restarts; cockpit surfaces the missing proof. |
| Accessibility gate failed | Participation Access + Required Actions | Required Action card "accessibility gate failed"; the failing category named; surface removed from member-facing track. |
| Translation missing | Participation Access + Required Actions | Required Action card "missing translation"; fallback language surfaced; review path offered. |
| Dashboard says healthy while member shell says degraded | Any surface | v0 violation per Design principle 9 ("member-impact summary is always present") + the member-impact mapping above. If detected, the cockpit row is marked stale until the member-impact summary can be reconciled. |
| Steward action lacks authority basis | Required Actions | Action rendered as visibly blocked; the missing mandate / clause / agreement is named; no confirm affordance until the basis is supplied. |
| Fintech vocabulary appears (payment / wallet / balance / currency / token / crypto / blockchain / timebank) | Any surface | v0 violation. Replace with settlement / position / obligation / allocation / receipt / provenance. Forbidden as positive ICN-native framing in operator copy. |

## First safe proof-loop / dogfood slice

Per `#1795` acceptance criteria and the session pattern from #1829 / #1830, the spec names docs- and fixture-first slices that exercise the cockpit contract without touching real operators, real partner federations, or a real backend.

### Slice A (preferred): Read-only receipt-store + anti-entropy degraded / repair cockpit fixture

- **Fixture peers.** Three peer fixtures (DIDs assigned, signing keys local) per `docs/spec/network-anti-entropy-proof-loops.md` Slice A.
- **Fictional institution / domain.** A fixture `LocalDomain` (e.g., `did:icn:demo:exampledomain`).
- **One open `DivergenceEvidence`.** Class "missing receipt"; affected scope the fixture domain; peers named; digest mismatch surfaced as Bloom-filter set-difference.
- **One `RepairPlan`.** Action "fetch missing receipt"; authority basis fixture clause; expected `RepairReceipt` class.
- **One `RepairReceipt`.** `EffectOutcome::Applied`; before / after digests; signed by fixture steward.
- **Cockpit fields rendered.** All nine fields from `#1829` §"Steward cockpit surface" (Affected scope, State class, Peers, Digest mismatch, Last successful proof, Repair plan, Authority required, Receipts/evidence, Escalation status). Member-impact summary attached: `Members see: Sync delayed → Receipt available`.
- **Accessibility checklist applied.** The twelve-category gate from ADR-0028 / `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` evaluated on the fixture rendering. Operator surfaces are accessible: keyboard reachable, screen-reader navigable, color-independent status, plain technical detail without surveillance-style aggregation.

### Slice B: Storage replica / backup overdue / restore-test receipt fixture

- Same fixture stack.
- Fixture artifacts with `ReplicationPolicy.target_replicas = 3` held by only two of three fixture peers.
- `DivergenceEvidence` class "replica missing"; `RepairPlan` of action "re-replicate within scope"; `RepairReceipt` with `EffectOutcome::Applied`.
- Backup cadence intentionally set such that the most recent backup is overdue; Required Action card "backup overdue" surfaces.
- Restore-test receipt fixture: one successful, one failed; cockpit surfaces both with member-impact summaries.

### Slice C: Compute placement review-required fixture

- Same fixture stack.
- Fixture `PlacementDecision` for an advisory workload that produces a `ReviewRequiredActionCard`.
- The fourteen `#1826` operator-dashboard fields rendered for the placement, including the fallback path if applicable.
- A fixture `ExecutorAdmissionDecision` and an optional `PlacementFallbackReceipt` exercise the post-placement artifact rendering.
- Member-impact summary: `Members see: Review required`.

All three slices are fixture-only. None implements the cockpit. None touches a real network, a real federation, real artifacts, or real members.

## Relationship to sibling work

| Concern | Where it lives |
|---|---|
| Steward cockpit (this spec) | `#1795` |
| Member shell v0 | `#1818` (merged #1830) |
| Network anti-entropy proof loops | `#1799` (merged #1829) — supplies the 8 cockpit fields |
| Compute placement policy | `#1801` (merged #1826) — supplies the 14 operator/steward dashboard fields |
| Storage durability policies | `#1816` (merged #1823) — supplies BackupPolicy / ReplicationPolicy / RecoveryPolicy / ArchivePolicy / IntegrityPolicy posture |
| ArtifactRegistry / ScopedVault | `#1798` (merged #1824) — supplies artifact-class taxonomy and scoped-vault posture |
| Governed service binding | `#1815` (merged #1822) — supplies the binding "Observe" lifecycle state |
| CCL policy registry | `#1817` (merged #1821) — supplies policy_version_id and adoption posture |
| Institutional domain | `#1794` (merged #1820) — supplies DomainPolicy hooks |
| Effect dispatch | `#1797` (merged #1819) — supplies Stage 5 evidence envelope |
| Entity-scope vocabulary boundary | `#1825` (merged) — supplies the corrected scope vocabulary |
| Integrated cooperative operating model | `#1793` (merged #1814) — supplies the spine that names the cockpit and shell together |
| Federation settlement finality | `docs/spec/federation-settlement-finality.md` — supplies dispute window / clearing receipt context |
| Node operator civic-role surface | `#1613` — separate operator surface; boundary documented above |
| Legibility dashboards | `#1012` — related concern; not in scope |
| Private data disclosure boundary | `#1792` — forward-direction; cockpit observes posture, never content |
| Encrypted distributed private-overlay storage | `#1767` — forward-direction; cockpit surfaces overlay loaded / missing |
| Language / glossary model | `#1610` — forward-direction; cockpit consumes glossary status |
| Multilingual / inclusive-access | `#1740` — forward-direction; cockpit surfaces translation posture |
| AuthorityClass / TypedScope / Mandate | ADR-0014 |
| Authority grant minting seam | ADR-0019 |
| Bootstrap activation + standing read model | ADR-0020 |
| Institutional Effect Record canonical schema | ADR-0025 |
| Receipt and provenance proof envelope | ADR-0026 |
| ActionCard contract | ADR-0027 + `docs/contracts/institution-package/action-card.schema.json` |
| Accessibility baseline | ADR-0028 + `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` |
| Compute workload manifest | ADR-0030 |
| Commons compute admission and settlement | ADR-0031 |
| Kernel / app separation | `docs/architecture/KERNEL_APP_SEPARATION.md` |

## Non-claims (repeat block for grep clarity)

- This spec does not implement a steward cockpit. No code lands here.
- This spec does not specify a frontend technology, a native shell, a web framework, or any platform choice.
- This spec does not define new endpoints. It consumes existing `icn-obs` metrics, governance proof / receipt surfaces, `/me/standing` (per ADR-0020), `/me/action-cards` (per `#1608` / `#1646`), and the merged sibling specs' status surfaces.
- This spec does not redefine any merged sibling spec, ADR, or `icn-obs` metric module.
- This spec does not introduce new receipt classes. It surfaces existing classes (per ADR-0026 / ADR-0025 / merged sibling specs) in operator-facing detail.
- This spec does not preempt `#1613` (node operator civic-role surface in Commons Shell). That is a separate operator surface.
- This spec does not redefine the steward / operator role. Role definitions belong to `DomainPolicy` and to institution-package skins.
- This spec does not authorize a generic admin panel, a surveillance console, a private-data viewer, or any account-as-product framing.
- This spec does not claim production readiness, a live partner federation, a formal NYCN pilot, or operation under this contract by any real institution today.
- This spec does not move, expose, preview, or cache private vault contents. Body bytes of `PrivateEvidence` artifacts never reach the rendering layer.
- This spec does not use wallet, payment, balance, currency, token, crypto, blockchain, or timebank framing for ICN-native participation. All such terms appear in this doc only in explicit negation context (Design principles, Failure / safety table, Non-claims) or as references to existing legacy code identifiers preserved without endorsement.
- This spec does not authorize any change to the runtime, gateway, SDK, website, deploy scripts, K3s, DNS, Forgejo, or any deployed infrastructure.
- This spec does not close `#1795`. The PR uses `Refs: #1795`; closure is a user-driven decision against the issue's acceptance criteria.
