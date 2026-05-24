---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-20
---

# Formation → Federation: where Launch and ICN actually fit together

> A synthesis from three sources: (a) ICN's existing substrate — concrete file and crate references, not vibes; (b) the 2024 NY Cooperative Summit attendee feedback — actual practitioner words; (c) the public surface of Launch.coop and Worker Place. The hypothesis is that Launch and ICN are two halves of a pipeline that does not exist anywhere today: from "a group decides to become a cooperative" through "that cooperative federates with others over years and survives every handoff that follows."

## 1. The hypothesis in one paragraph

Launch.coop makes cooperative formation **doable** — the wizard, the document portal, the team and advisor surface, the multilingual UX. ICN, beneath that, holds the **state machine and durable institutional record** that turns formation artifacts into a cooperative that can survive every handoff that follows — founder rotation, advisor turnover, fiscal years, member exits, dissolution, and eventually federation with other coops. Launch is the obstetrician. ICN is the birth certificate, the membership register, the internal capital account ledger, the labor patronage record, and the inter-cooperative clearing protocol that survive the obstetrician going home.

Neither alone is sufficient. Launch without ICN produces a formed coop whose ongoing records live in vendor SaaS and fragile spreadsheets. ICN without Launch produces a substrate no founding team would ever bootstrap onto unaided.

## 2. What the field actually said it needs

From the 2024 Summit attendee evaluation (35 returns surveyed):

> "No central registry of guests / attendees and their contact info, so we can stay in touch or meet folks we didn't have time for."
> — **Arda Ungun** (co-op developer, software engineer)

> "Curated opportunities throughout the year to collaborate with other cooperatives and co-op practitioners."
> — repeated in ~75% of returns as the single most-requested next step

> "Educational opportunities about co-ops throughout the year."
> — second most-repeated request

> "Could be more sessions for co-op members to discuss everyday challenges and network / crowdsource solutions and or ideas."

> "Deep dive strategy session w TA providers across the state to learn, deepen practice."

> "Stronger network to share resources and learning."

> "Would love us to build toward a statewide trade association or state USFWC chapter."

> "Site visits and tours of each others ecosystems."

> "Story-sharing events to build camaraderie."

> "Discord server."

The dominant theme is not "we want a better Summit." It is "the Summit is a once-a-year ping; we need a between-event coordination substrate." That substrate is what every attendee is reaching for, in different vocabulary.

From the NY Cooperative Summit Lessons Learned (Dec 2025), organizer-side:

- *"There's a bit of a waterfall of tasks… Marketing and fundraising depended on content and logistics teams to lock things in before we could start our process."* — Survey response
- *"Perhaps we could use our fundraising ability to hire part-time staff?"* — Mike Zak
- *"I think it would be great if the backbone group, which is heavy on developers, were able to work toward securing money for stipends/annual paid positions for members of co-ops that may want to be involved with organizing the summit, but have a harder time stepping away from the work that pays them."* — Survey response

The organizers themselves see the institutional-memory and patronage-allocation gap.

## 3. What Launch.coop appears to do (from public material)

Inferred surface, not claimed:

- Guided onboarding and decision-tree intake for prospective co-ops.
- Co-founder and team workspace, with task assignment and comments.
- Document portal with change history and secure handoff.
- Advisor / TA provider / lawyer / CPA engagement surface.
- Mobile, print, and multilingual access (English + Spanish, expandable).
- "Make the ask feel doable" — the explicit framing.

What Launch is *not* claiming to do, from the same public surface:

- Durable post-formation member ledger.
- Patronage allocation across years.
- Internal capital account record-keeping.
- Inter-cooperative coordination.
- Federation-level credit settlement.

That's the seam.

## 4. What ICN actually has — concrete, with file references

Everything below is real code in the repo, not architecture decks.

### 4.1 Cooperative formation lifecycle

`icn/crates/icn-coop/src/lifecycle.rs` models a complete cooperative lifecycle as enumerated state transitions:

```
Forming
  → CharterSigned (per-founder, signatures accumulate)
  → CharterRatified (all founders signed)
  → Activated (charter ratified + min_members met)
  → (optional) Suspended → Resumed
  → DissolutionStarted (proposal-linked)
  → AssetsDistributed (per AssetDistributionPlan)
  → Dissolved
```

Every transition is a `LifecycleEvent` with structured fields. Treasury DID is derived deterministically from coop ID at activation:

`derive_treasury_did(coop_id) → "did:icn:treasury:<base58-anchor>"`

A governance domain (`coop.{id}`) is auto-created at formation. The cooperative carries its own `CharterId`, `domain_id`, and a list of `StoredFounderSignature` (Ed25519 over charter content + timestamp + optional role like "initiator" or "steward").

### 4.2 Formation request as a typed artifact

`icn/crates/icn-coop/src/types.rs::FormationRequest` carries the exact fields a formation platform would need to hand off:

- `name`, `coop_type`, `min_members` (default 3), `founding_members: Vec<Did>`
- `charter_document_hash: Option<[u8; 32]>` — off-chain charter document hash
- `description`, `currency` (default `"hours"`)

This is the API surface Launch could write into, today.

### 4.3 Charter as code, not as PDF

`icn/apps/charter/src/lib.rs` and `oracle.rs` implement a `CharterPolicyOracle` that takes a CCL document plus context and produces kernel-enforceable constraints:

```
YAML charter doc → CclDocument → charter_to_constraints() → ConstraintSet
                                  ↑ MEANING FIREWALL BOUNDARY
```

Multiple charters can be deployed per oracle. The cooperative's charter becomes the policy oracle that gates every future decision the substrate evaluates. This is the mechanism by which a coop's bylaws are not just a PDF in someone's Drive but an enforced policy in the running system.

### 4.4 Membership management, unified across entity types

`icn/apps/membership/` consolidates Cooperative / Community / Federation / Individual membership under a single app:

- `coop_core/` — Cooperative-specific lifecycle (formation, treasury, members, application, change tracking)
- `community_core/` — Community lifecycle (members, resource pools)
- `entity_core/` — Cross-cutting EntityRegistry, EntityRelationship, lifecycle, labor_exchange
- `entity_core/labor_exchange.rs` — labor patronage across coops (see §4.5)

A unified `MembershipManager` lets you `add_member(member, parent, role, weight)` against any entity type. `MembershipClass` and `MembershipCriteria` are CCL-evaluable, so charter rules ("must complete trial period," "must reach 0.3 patronage threshold," etc.) are enforced by the kernel.

### 4.5 Labor exchange with credit routing — the patronage substrate

`icn/apps/membership/src/entity_core/labor_exchange.rs` is the most important piece for McKenzie's stated unlock. It models:

- **`LaborAssignment`**: worker, home_coop, host_coop, assignment_type (Project / Seasonal / Trial / DualMembership), start/end dates, status.
- **`CreditRouting`**: how credits earned at the host flow back. `ThroughHomeCoop`, `DirectToWorker`, or `Split { worker_pct }`.
- **`admin_fee_bps`**: the home cooperative's coordination fee in basis points (0–10,000).
- **`share_contribution`**: whether host contributes to worker's home-coop shares.
- **`AssignmentApprovals`**: governance proposal IDs from both home and host coops — assignments require dual-side approval.
- **`LaborPool` / `PoolRegistration`**: a worker registers availability; cooperatives match against `LaborNeed`.

This is the mechanism by which a worker's patronage at a host coop becomes a credit allocation routed to (and tracked at) the home coop. The substrate to track it exists; what's missing is the formation-platform handoff that gets a coop onto the substrate in the first place. The legal framing — how a given jurisdiction wants those records held — belongs to the cooperative's attorney, not to ICN.

### 4.6 Dissolution + asset distribution

`icn/crates/icn-coop/src/types.rs::AssetDistributionPlan`:

- **`positive_balance_action`**: `ReturnToMember`, etc.
- **`negative_balance_action`**: `WriteOff`, etc.
- **`capital_return: CapitalReturnMethod::ProRata`** — direct mapping to ICA capital return on dissolution.
- **`residual_recipient`**: optional federation treasury.

The default plan is "ReturnToMember (positive) + WriteOff (negative) + ProRata capital return." A real cooperative would override per its charter.

### 4.7 Federation: cross-coop discovery, trust, clearing

`icn/crates/icn-federation/` implements the inter-cooperative coordination layer that the Summit attendees were unknowingly asking for:

- **`registry.rs`** — `CooperativeRegistry`: gossip-discovered registry of cooperatives in the federation.
- **`attestation.rs`** — `FederatedTrustAttestation` with `EvidenceSummary` and `TrustContext`. One coop attests to another's standing without a central authority.
- **`clearing.rs` + `clearing_manager.rs`** — `BilateralClearingAgreement`, `ClearingPosition`, `CrossCoopTransfer`, `SettlementInterval`, `SettlementReport`. Bilateral credit clearing between coops.
- **`netting.rs`** — `NettingEngine`, `DebtCycle`, multilateral netting across N coops. The thing that turns "A owes B owes C owes A" into "everyone is square."
- **`receipt_clearing.rs`** — cross-scope receipt batching with `BatchClearingConfig`, `ClearingReceipt`, `FlushReport`.
- **`agreement/`** — `InterCooperativeAgreement` types with manager + gossip + store. The federated-treaty primitive.
- **`router.rs` + `channel.rs`** — scoped gossip with cooperative-level visibility classes.
- **`resolver.rs`** — federated DID resolution across cooperative boundaries.

This is "Discord server, but it's the coordination substrate, not a chat app" with credit clearing built in. A network of coops can discover each other, attest to each other's standing, clear credits across boundaries, and net debt cycles — without a central platform.

### 4.8 The institution-package model

`docs/strategy/NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md` already names the layer between substrate and any single cooperative or ecosystem:

- ICN core stays domain-agnostic (Entity, Standing, Role, Authority, Mandate, Obligation, Action, Receipt, Evidence class, Privacy class, Review status, Handoff state).
- **Institution packages** carry the meaning (Summit, Committee, Session, Speaker, PublicProgram, FeedbackTheme, PlanningObligation, AccessibilityCommitment, FollowUpInvitation, EvidencePacket, HandoffPacket).
- A formation-platform package would be the obvious next one: `Formation`, `FoundingTeam`, `AdvisorRelationship`, `FormationMilestone`, `LegalArtifact`, `OperatingTransition`.

This is the mechanism by which a Launch-shaped formation flow becomes a forkable, governable artifact other formation platforms (or co-op developers) can compose against. Not a Launch-replacement; a Launch-compatibility-shape that other formation work can reuse.

## 5. The seam: where Launch hands to ICN, where ICN hands back to Launch

The seam is bidirectional. The naive read is "Launch is the front-end and ICN is the back-end." The truer read is that the two are interlocking on multiple ongoing axes.

### 5.1 Launch → ICN (formation artifacts become institutional state)

| Launch produces | ICN absorbs as |
|---|---|
| Founding team roster | `FormationRequest.founding_members: Vec<Did>` |
| Charter draft (Launch document portal) | `charter_document_hash` + ratification via `StoredFounderSignature` |
| Co-op type / model selection | `CoopType` in `FormationRequest` |
| Minimum-members commitment | `min_members` field |
| Operating currency convention | `currency` field |
| Founder signatures collected | `LifecycleEvent::CharterSigned` events |
| Final ratification | `LifecycleEvent::CharterRatified` |
| Activation milestone | `LifecycleEvent::Activated` + treasury DID issued |

After activation, the cooperative is **a verifiable institutional fact**: the charter is enforced as kernel constraints, the founders are signed and time-stamped, the treasury DID exists, and the governance domain is live. Nothing about that depends on Launch continuing to exist.

### 5.2 ICN → Launch (durable records flow back into Launch's surface)

| ICN tracks | Launch can render |
|---|---|
| Ongoing membership changes | Member directory, pending applications |
| Role transitions | Org chart, board / steward views |
| Internal capital account balances | Member equity view (read-only) |
| Patronage allocation events | Annual statement view |
| Labor assignments to / from other coops | Inter-coop coordination view |
| Federation discovery + attestations | "Coops in our network" view |
| Pending obligations | Action items / dashboard |
| Receipts of past decisions | Audit log, governance history |

Launch becomes a renderer of state ICN holds, not the system of record. If Launch goes away, the records survive intact and can be rendered by any other surface — a self-hosted dashboard, a steward console, an auditor's portal, a member's mobile app.

### 5.3 The advisor / TA-provider loop

This is the third seam, the one that maps cleanly onto Summit attendees' "deep dive strategy session w TA providers across the state to learn, deepen practice."

| Step | Launch's role | ICN's role |
|---|---|---|
| Co-op engages TA provider | Provider relationship record | Cross-entity standing record (TA provider has visibility scope into the coop) |
| TA provider issues guidance / draft | Document in Launch portal | Provenance record: who drafted, under what authority, for what purpose |
| Co-op adopts / amends guidance | Comment / approval in Launch | Receipt: under whose authority, against what charter clause |
| TA provider observes outcome | Optional post-mortem note | Receipt visible to TA provider per agreed scope, no real personal data |
| TA provider publishes pattern (anonymized) | Published to TA provider's own surface | Federated attestation: "this pattern worked for N coops in our network" |

This is the substrate that turns the Summit's annual TA-provider gathering into a year-round, evidence-backed practice network — without making any TA provider build the network themselves.

## 6. The bigger frame: formation → federation

The pipeline this proposes has six stages. Most are visible to the founding team; one or two are invisible until they matter.

1. **Pre-formation discovery.** Launch territory: decision tree, model selection, basic education. ICN doesn't need to be present yet.
2. **Founding-team assembly + charter drafting.** Launch territory, with the option to hash-anchor the charter into ICN so the document version that gets ratified is the document version that becomes the policy oracle.
3. **Ratification + activation.** Hand-off moment. Launch collects founder signatures; ICN ratifies the lifecycle event and issues the treasury DID.
4. **Operating coop.** Launch optionally remains the member-facing surface. ICN holds the ledger, membership, capital account, patronage, governance domain. Other surfaces can also read.
5. **Inter-cooperative work.** Labor exchange across coops, federated trust attestations, bilateral credit clearing, multilateral netting. Launch may or may not render this; ICN is the substrate either way.
6. **Dissolution or transition.** Asset distribution plan executes per the coop's own charter. Receipts survive forever; the institution can be cleanly closed without losing the record.

The Summit attendees are reaching for stages 4 and 5. Launch (publicly) covers 1 through 3. ICN's gap is the human-facing surface to 4 and 5. Together: the formation-to-federation pipeline.

## 7. The smallest joint rehearsal worth doing

A fictional five-member worker cooperative in NY State.

- **Year 0** — formation: charter drafted in Launch, founding team rostered, ratification ceremony produces founder signatures. ICN absorbs as a `FormationRequest` → `Activated` lifecycle, treasury DID issued.
- **Quarter 1** — operations: members log hours through whatever surface (Launch or otherwise). Patronage credits accrue. Internal capital accounts begin holding non-zero balances.
- **Quarter 2** — first labor exchange: one member spends three weeks at a host coop. `LaborAssignment` created with `CreditRouting::ThroughHomeCoop`. Both governance proposals approved. Credits flow to home coop, member gets paid, ICA balance updates.
- **Quarter 3** — first member exit: one of five leaves. Charter dictates ICA balance returned per capital-return method. Receipts survive showing the exit was clean.
- **Quarter 4** — annual statement: each member sees their year's patronage allocation. The CPA can verify the underlying ledger. The new board sees the receipts of every governance decision the founders made.
- **Year 2** — federation: this coop joins three other worker coops in a federation. `FederatedTrustAttestation` issued. Bilateral clearing agreements established. The four-coop ecosystem can now route labor between each other and clear credits monthly without a central platform.

The rehearsal needs no real data, no live cooperative, no production claims. It needs: a sanitized scenario, a concrete charter, a known-good lifecycle path, and the ability to walk it end-to-end through ICN's existing primitives. The output is either "yes, the seams hold" or "here are the specific gaps."

## 8. Risks and what would kill it

- **Bus factor.** One maintainer. The thesis dies if the maintainer can't be supported through Phase 19–25.
- **UX gap.** Launch is human-facing. ICN is not yet. The rehearsal works only if the formation-platform side is willing to be the human surface for the joint pilot.
- **Regulatory drift.** The substrate's vocabulary discipline (settlement, obligation, allocation, position, receipt, provenance, evidence) reflects a deliberate choice to avoid claiming financial-product semantics. Whether that posture is sufficient for any specific jurisdiction is an attorney question, not an architecture one.
- **CCL maturity.** Charter-as-code only works if the CCL language is expressive enough for real worker-coop bylaws. Today it can encode the structural pieces (membership tiers, thresholds, capital return methods); it cannot yet encode every nuance a real cooperative attorney would write. The gap is closeable; today it's open.
- **The "we want one platform" attractor.** Both Launch and ICN may be pulled toward becoming "the one cooperative platform." That kills the synergy. The discipline is: Launch is one possible formation front-end; ICN is one possible substrate; institution packages are forkable. The pipeline only works if both sides hold the line on not absorbing each other's territory.

## 9. Non-claims

- This is not a partnership ask, a pilot proposal, or a commitment from either side.
- Nothing in this document obligates Launch.coop, Worker Place, comp.coop, NYCN, Alchemical Nursery, or any cooperative referenced.
- ICN is research-stage substrate, not production software. Single maintainer.
- Attendee quotes are from the public 2024 Summit evaluation; no private NYCN data appears here.
- Nothing in this document is legal advice. Any reference to legal mechanisms or regulatory categories is shorthand for "an attorney would need to determine this," not a claim the author has verified.

## 10. Read together

- ICN primitives, by crate:
  - `icn-coop/lifecycle.rs` — formation state machine
  - `icn-coop/types.rs` — `FormationRequest`, `AssetDistributionPlan`, member types
  - `apps/charter/` — charter-as-code oracle
  - `apps/membership/` — unified membership across entity types
  - `apps/membership/src/entity_core/labor_exchange.rs` — patronage substrate
  - `icn-federation/` — inter-cooperative coordination
- Institution-package thesis: `docs/strategy/NYCN_SUMMIT_REFERENCE_INSTITUTION_STRATEGY.md`
- Summit field signal: `docs/strategy/NY_Cooperative_Summit_Lessons_Learned` (Drive) + 2024 Attendee Eval responses (Drive)
- Doctrine on receipts vs. legitimacy: `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`
- Meaning firewall: `docs/architecture/KERNEL_APP_SEPARATION.md`
