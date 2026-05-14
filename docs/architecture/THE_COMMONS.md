---
Status: doctrine
Canonical: no
Authority: architecture (complements KERNEL_APP_SEPARATION.md and INSTITUTION_PACKAGE_BOUNDARY.md)
Last Reviewed: 2026-04-24
Owner: Matt Faherty
---

# The Commons

> **ICN is infrastructure for The Commons. ICN is not The Commons.**
>
> The Commons is what communities, cooperatives, and federations build and live inside. ICN is the substrate that makes The Commons executable, verifiable, portable, federated, and resilient against capture.

This document is the canonical doctrine on what the Capital-C **Commons** means in the context of ICN. It sits above `ARCHITECTURE.md` (which describes the substrate) and above `KERNEL_APP_SEPARATION.md` / `INSTITUTION_PACKAGE_BOUNDARY.md` (which describe the meaning firewall and institution packaging). It is the north star those docs serve. It is consulted when deciding whether a proposed feature belongs in:

- the **ICN kernel** (generic primitive mechanics),
- an **ICN platform crate** (generic institutional shapes), or
- an **institution package** (institution-specific vocabulary), or
- a **Commons shell** (member-facing surface), or
- nowhere in ICN at all.

It does not specify any new kernel behavior. It names the thing ICN exists to enable so that future design choices do not drift.

---

## Purpose

ICN exists to make The Commons operable.

**The Commons** is the shared substrate of social, economic, institutional, cultural, and civic life that a community holds and tends in common. It is older than ICN by centuries — the shared grazing land, the mutual aid society, the credit union, the cooperative, the church, the school, the library, the neighborhood assembly, the volunteer fire brigade, the commons of knowledge, the commons of language. These are Commons institutions. They are not platforms. They are not products. They are not one-way services. They are organized forms of people choosing to hold something together.

The Commons is plural and nested. A household is a Commons. A cooperative is a Commons. A federation of cooperatives is a Commons. A regional coordination network is a Commons. A global Commons of practice (a discipline, a language, a body of shared knowledge) is a Commons. Capital-C **The Commons** is the overarching word for all of them — the shared human substrate that modern platforms, corporations, and states have systematically enclosed, privatized, or neglected.

ICN's purpose is **not** to be The Commons. The Commons is made of people, their relationships, their agreements, their labor, and their memory. ICN is digital public infrastructure that makes those forms:

- **Executable** — agreements, decisions, and roles are enforceable by mechanism, not only by goodwill.
- **Verifiable** — every state change leaves a receipt; every authority has a provenance.
- **Portable** — members own their keys and their data; they can always exit and take what is theirs.
- **Federated** — entities retain sovereignty; coordination happens between them, not above them.
- **Resilient** — no central operator, no platform landlord, no single point of capture or removal.

ICN is the *substrate* layer. The Commons is what **lives on it**.

Everything ICN does — identity, membership, governance, ledger, gossip, receipts, federation, compute — exists to give Commons institutions real operational capacity without making them tenants of someone else's platform.

---

## Non-Goals

To keep doctrine precise, The Commons is **not** any of the following. ICN must never be designed as if it were.

- **Not a marketplace.** ICN has no matching engine, no price discovery, no auction layer. Communities may build marketplaces as apps, but the substrate never becomes an extractive intermediary. See `ARCHITECTURE.md §2`.
- **Not a platform.** A platform owns its users; The Commons belongs to its members. ICN has no operator who can deplatform, throttle, monetize attention, or sell behavior data.
- **Not a state.** The Commons is not a replacement for public institutions, and ICN has no monopoly on legitimate force, taxation, or judicial finality. Some Commons institutions may coexist with, petition, or contract with states; ICN does not pretend to be one.
- **Not a church.** ICN encodes no ethical, religious, or spiritual doctrine. Different Commons institutions carry different traditions of meaning, care, and ritual; ICN must not bake any single tradition into kernel mechanics.
- **Not a charity layer.** ICN is not a donation platform, nonprofit administration tool, or grant-making rail. Mutual aid is a first-class Commons pattern, but "charity" (one-way giving from rich to poor mediated by a platform) is not the model.
- **Not a blockchain or token economy.** No global ledger, no native token, no speculative instruments, no tokenomic incentives. Mutual credit is bilateral; receipts are evidentiary, not tradable assets. See `docs/genesis.md` and `ARCHITECTURE.md §2`.
- **Not one global admin hierarchy.** The Commons is plural. There is no top. There is no single federation that governs all federations. ICN must never contain a privileged operator, root key, or "platform" that other participants serve.
- **Not one social doctrine in the kernel.** This is the hardest non-goal to hold. Different Commons traditions disagree — about consensus vs. majority, about one-member-one-vote vs. weighted stakes, about property, about membership criteria, about governance scope. Those disagreements belong in apps, CCL charters, and institution packages. The kernel must remain semantically blind.

This last point ties The Commons doctrine directly to the **Meaning Firewall**. See `docs/architecture/KERNEL_APP_SEPARATION.md` (Invariants 1–5) and `docs/genesis.md` ("The Meaning Firewall Boundary"). The firewall is not only an architecture discipline — it is how ICN stays usable by Commons institutions with different values.

> If ICN encoded one social ideology in its kernel, it would cease to be infrastructure and become a platform for that ideology. The kernel enforces *that agreements exist and are kept*. Apps decide *what agreements mean*.

---

## First Principles

Before naming primitives, name needs. ICN's primitives exist because Commons institutions need them. Start from humans and institutions; derive the technical shape from there.

### Human and Institutional Needs

Any durable Commons institution has to make room for:

1. **Belonging** — knowing who is in, on what terms, and under what obligations.
2. **Agency** — a member can act, propose, object, vote, contribute, withdraw.
3. **Care** — mutual aid, sickness, hardship, welcoming, conflict repair.
4. **Memory** — decisions, agreements, promises, debts, and gratitude survive turnover.
5. **Recognition** — labor, contribution, and expertise are visible and accountable.
6. **Purpose** — the institution has stated reasons for existing and can be held to them.
7. **Access** — participation does not require elite credentials, expensive hardware, or secret knowledge.
8. **Voice** — members can speak, dissent, amend, and publicly record objections.
9. **Repair** — wrongs can be named, addressed, and durably corrected.
10. **Continuity** — the institution survives individuals; roles, records, and authority transfer cleanly.
11. **Shared resources** — land, tools, funds, knowledge, infrastructure, labor pools.
12. **Transparent rules** — everyone can see what is allowed, what is required, what is forbidden.
13. **Bounded authority** — power is scoped, time-limited, revocable, and answerable to those it governs.
14. **Conflict handling** — disputes have legitimate channels that do not rely on the patience of the wronged.
15. **Knowledge transfer** — elders, coordinators, and stewards can pass on institutional know-how.
16. **Economic coordination** — labor, credit, surplus, and obligation flow without external financialization.
17. **Intergenerational memory** — institutions remember across decades, not just sprints.

### Translation to ICN Requirements

These needs translate into concrete substrate requirements:

| Human/Institutional Need | ICN Requirement |
|--------------------------|-----------------|
| Belonging | Know who belongs to what entity, in what role, with what capabilities. |
| Agency | A member can act under cryptographic identity without asking a central server. |
| Care | Mutual aid, dispute, and care flows are first-class, not grafted onto finance. |
| Memory | Every state transition emits an immutable receipt (`docs/genesis.md` invariant 10). |
| Recognition | Contributions (labor, decisions, stewardship) are attributable and durable. |
| Purpose | Each entity has a charter; charters are the constitutional layer (CCL). |
| Access | Works on ordinary phones, browsers, cheap Linux boxes, and public terminals. |
| Voice | Proposals, votes, discussion, amendment, and appeal are primitives. |
| Repair | Disputes, amendments, and appeals are typed — not reduced to "file a ticket". |
| Continuity | Roles, mandates, and authority grants have provenance, scope, and expiry. |
| Shared resources | Treasury, mutual credit, commons pools are entity-held, not platform-held. |
| Transparent rules | Constraints are published and enforced mechanically by the kernel. |
| Bounded authority | Capabilities are typed, scoped, delegable, revocable (see `AuthorityGrant`, ADR-0014). |
| Conflict handling | Appeal, amendment, veto, and force-close are proposal variants. |
| Knowledge transfer | Institutional documents, meeting minutes, and program cycles persist. |
| Economic coordination | Mutual credit, double-entry ledger, zero-sum invariant. |
| Intergenerational memory | Signed receipts, content-addressed history, portable by member export. |

**None of this requires ICN to hold opinions about what a cooperative, community, or federation *should* be.** ICN requires only that authority, membership, decisions, resources, and receipts exist as typed, verifiable, portable objects.

---

## ICN Primitive Mapping

This section maps Commons concepts to the real crates, modules, and types in the ICN repo as of 2026-04-24. Types that exist are cited with their actual paths. Types that are planned or spec-only are explicitly marked `[future design]`.

| Commons Concept | ICN Primitive | Status | Location |
|-----------------|---------------|--------|----------|
| Person / human participant | `Did` (newtype around `String`; 32-byte identifier encoded in DID form, representing either an Ed25519 public key or an SDIS anchor id) | ✅ shipped | `icn/crates/icn-identity/src/lib.rs` (Ed25519 path) and `icn/crates/icn-identity/src/anchor.rs` (`Did::from_anchor_id`) |
| Cryptographic identity bundle | `IdentityBundle` (Ed25519 + X25519, age-encrypted keystore) | ✅ shipped | `icn/crates/icn-identity/src/bundle.rs` |
| Unified entity handle | `EntityId` (format `entity:icn:<type>:<slug>`) | ✅ shipped | `icn/crates/icn-entity/src/entity.rs` |
| Sovereign entity kinds | `Individual`, `Cooperative(CooperativeProfile)`, `Community(CommunityProfile)`, `Federation(FederationProfile)` | ✅ shipped | `icn/crates/icn-entity/src/entity.rs` |
| Membership | `Membership { member_id, parent_id, role, status, shares, capabilities }` | ✅ shipped | `icn/crates/icn-entity/src/membership.rs` |
| Membership role | `MembershipRole` enum (`Founder`, `Member`, `Worker`, `Consumer`, `Producer`, `BoardMember`, `Officer{title}`, `FederatedMember`, `AssociateMember`, `ObserverMember`, `ProvisionalMember`, `Custom{name}`) | ✅ shipped | `icn/crates/icn-entity/src/membership.rs` |
| Per-membership capability grants | `MembershipCapability` enum (`Vote`, `Propose`, `TreasuryAccess`, `Invite`, `ManageSubEntities`, `Sign`, `Configure`, `ViewSensitive`, `Custom(String)`) | ✅ shipped | `icn/crates/icn-entity/src/membership.rs` |
| Non-sovereign internal unit | `Structure { kind: Committee | WorkingGroup | Team | Office }` | ✅ shipped | `icn/crates/icn-governance/src/structure.rs` |
| Role inside a structure | `RoleAssignment { id, structure_id, person_did, role, authority_scope: Vec<String>, start_date: Timestamp, end_date: Option<Timestamp>, assigned_by_decision }` | ✅ shipped | `icn/crates/icn-governance/src/structure.rs` |
| Time-bounded work | `Activity { kind: Event | Program | Project | Initiative }` | ✅ shipped | `icn/crates/icn-governance/src/activity.rs` |
| Program / cycle container | `Program { ... }`, `Milestone { ... }` | ✅ shipped (PR #1548) | `icn/crates/icn-governance/src/program.rs` |
| Polymorphic parent attachment for operational objects | `InstitutionalParent` enum | ✅ shipped | `icn/crates/icn-governance/src/parent.rs` |
| Governance domain (scope of proposals/votes) | `GovernanceDomain` | ✅ shipped | `icn/crates/icn-governance/src/domain.rs` |
| Proposal | `Proposal`, `ProposalPayload` (`Text`, `Budget`, `Allocation`, `Membership`, `ConfigChange`, `FreezeMember`, `VetoProposal`, `ForceCloseProposal`) | ✅ shipped | `icn/crates/icn-governance/src/proposal.rs` |
| Vote | `Vote` | ✅ shipped | `icn/crates/icn-governance/src/vote.rs` |
| Delegation (individual → individual) | `Delegation` (1717 LOC; domain-scoped, expiring, revocable) | ✅ shipped | `icn/crates/icn-governance/src/delegation.rs` |
| Typed authority grant (entity → grantee, scoped, time-bounded, revocable) | `AuthorityGrant { class, grantor, grantee, scope: TypedScope, valid_from, valid_until, revoked_at }` | ✅ type landed; typed-scope enforcement is additive migration per ADR-0014 | `icn/crates/icn-governance/src/authority.rs` |
| Mandate (execution obligation linked to a decision) | `Mandate { id, decision: DecisionProvenance, payload_hash, grants, executor, deadline, status, issued_at }` | ✅ shipped | `icn/crates/icn-governance/src/mandate.rs` |
| Meeting | `Meeting` (schedule, agenda, attendance, minutes) | ✅ shipped (PR #1543) | `icn/crates/icn-governance/src/meeting.rs` |
| Action item | `ActionItem`, `ActionItemSpec` (decision-to-action bridge, provenance-linked to proposals) | ✅ shipped | `icn/crates/icn-governance/src/action_item.rs` |
| Charter (constitutional doc for an entity) | `Charter`, `CharterStore` | ✅ shipped | `icn/crates/icn-governance/src/charter.rs`, `charter_store.rs` |
| Cooperative contract language | CCL — AST, interpreter, fuel-metered | ✅ shipped | `icn/crates/icn-ccl/` |
| Governance receipts / proof | `icn-governance::proof` | ✅ shipped | `icn/crates/icn-governance/src/proof.rs` |
| Treasury + mutual credit + receipts | Double-entry journal, zero-sum invariant, `LedgerPolicyOracle` | ✅ shipped | `icn/crates/icn-ledger/` |
| Trust graph + trust-gated rate limits | `TrustGraph`, `TrustPolicyOracle` | ✅ shipped | `icn/crates/icn-trust/`, `icn/apps/governance/` (oracle) |
| PolicyOracle (meaning firewall) | `PolicyOracle` trait, `ConstraintSet`, `PolicyDecision` | ✅ shipped | `icn/crates/icn-kernel-api/` |
| Capability token / bearer | `Capability`, `CapabilitySet` (genesis bootstrap) | ✅ shipped | `icn/crates/icn-kernel-api/src/bootstrap.rs` |
| Frozen-core invariants | 10 invariants defined as `InvariantId` | ✅ shipped | `icn/crates/icn-kernel-api/src/invariants.rs` |
| Federation membership list | `FederationProfile.member_entities` | ✅ shipped | `icn/crates/icn-entity/src/entity.rs` |
| Naming / DID resolution | `icn-naming` | ✅ shipped | `icn/crates/icn-naming/` |
| Scope-aware identity across devices | `MultiDevice`, `Capability` enum | ✅ shipped | `icn/crates/icn-identity/src/multi_device.rs` |
| Node / daemon | `icnd` | ✅ shipped | `icn/bins/icnd/` |
| Gateway (REST + WebSocket, JWT, per-DID rate limit) | `icn-gateway`, pilot-ui surfaces | ✅ shipped | `icn/crates/icn-gateway/`, `web/pilot-ui/` |
| Institution package (e.g., NYCN) | in-monorepo staging at `institutions/nycn/`; target: external `nycn-icn` repo | ⚠️ boundary doc pinned; package thin | `institutions/nycn/`, `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` |
| "My scopes" surface | `GET /gov/me/scopes` (role assignments for caller DID) | ✅ shipped (PR #1552) | `icn/apps/governance/src/http/handlers.rs` |
| "My work" surface | `GET /gov/me/work` (assigned action items for caller DID) | ✅ shipped (PR #1552) | `icn/apps/governance/src/http/handlers.rs` |
| **Commons Shell** (unified member-facing doorway across entities) | concept; partial predecessors in `web/pilot-ui/` and `sdk/react-native/examples/CoopWallet/` | `[future design]` | — |
| `GET /me/standing` (roles + memberships + mandates + active scopes) | shipped — `icn-gateway` member-standing read model | `[shipped, #1627]` | — |
| `GET /me/action-cards` (unified call-to-action surface) | shipped — three of five source paths emit proof-bearing receipts (proposal/vote, action_item/complete, meeting/attend); two RFC-gated under #1646 (signal_rule on #1631, obligation_lifecycle on #1634) | `[shipped, #1659]` | — |
| `RepresentativeVote` / `EntityAction` (action whose subject is the represented entity, not the signer) | `AuthorityClass::Representation` exists in ADR-0014; no dedicated vote-wrapper type yet | `[future design]` | — |
| Active-scope / session-selected-scope model (every action bound to an explicit chosen scope) | scope is currently inferred from JWT `coop_id` + scopes; no explicit active-scope primitive | `[future design]` | — |
| Commons OS / node appliance / kiosk mode | aspirational; see §11 | `[future design]` | — |

Where this doc says `[future design]`, it means: *the concept is named here, it has not been built, and this doc does not authorize its implementation.* A dedicated design doc and issue are required before code lands.

---

## Entity and Membership Model

A member's DID is the cryptographic root of their participation. It is **not** a login, **not** a wallet address in the crypto sense, and **not** a platform account. It is a key pair the member holds, under their control, from which all participation derives.

### Layers

1. **Individual DID → Individual Entity.** Every person in the system starts as a `did:icn:<base58-ed25519-public-key>`. `EntityId::from_did(did)` produces the matching individual entity (format `entity:icn:individual:<identifier>`). There is no "user account" between the key pair and the entity.

2. **Membership** is the typed link between a member entity and a parent entity. Individuals hold membership in communities and cooperatives. Entities (cooperatives, communities, other federations) hold membership in federations. Membership is recursive: a cooperative's members can themselves be cooperatives.

3. **Membership confers standing.** A `Membership` record carries:
   - `role: MembershipRole` — what kind of member (`Founder`, `Member`, `Worker`, `BoardMember`, `Officer{title}`, `FederatedMember`, `AssociateMember`, `ObserverMember`, `ProvisionalMember`, `Custom{name}`, …).
   - `status: MembershipStatus` — whether the role is currently active.
   - `shares: u64` — voting weight per charter (0 means observer).
   - `capabilities: Vec<MembershipCapability>` — default grants (`Vote`, `Propose`, `TreasuryAccess`, `Invite`, `ManageSubEntities`, `Sign`, `Configure`).

4. **RoleAssignment** layers further capability onto structures. A member of `nycn-organizers` may also hold a `RoleAssignment` in the `nycn-finance` structure with `authority_scope` strings like `"approve-budget-<=5000"`. The role inside a structure is bounded by the structure's delegated authority.

5. **AuthorityGrant / Mandate** (ADR-0014) is the typed, scoped, revocable layer of authority above capability strings. A `Mandate` binds an executor to a specific decision's obligation. An `AuthorityGrant` binds a grantee to a typed scope, time-bounded, with explicit grantor provenance.

### Illustrative Flow

```
did:icn:alice
  → Individual Entity               (entity:icn:individual:alice-pubkey)
  → Membership in greenstar-coop    (role: Worker, capabilities: [Vote, Propose])
  → Membership in brooklyn-coop-net (role: Member via greenstar delegate)
  → RoleAssignment in               (structure: nycn-finance,
    nycn-finance committee             role: "coordinator",
                                       authority_scope: ["approve-budget-<=5000"])
  → Mandate to represent            (class: Representation,
    greenstar in NYCN federation       grantor: greenstar,
                                       grantee: did:icn:alice,
                                       scope: nycn-federation-gov proposals)
```

**This illustrative layering is not a single primitive today.** Each layer exists in the repo; they compose. Alice's action surface at any given moment is the *union* of:

- capabilities from her direct memberships,
- capabilities from her role assignments in structures,
- representation mandates granted to her by other entities,
- operator-level rights on any node she runs.

A single API returns "everything Alice is standing in right now" — the `GET /me/standing` surface, shipped in #1627 (see §9).

> **Illustrative only.** The flow above composes real types but is pseudocode. Do not treat it as a single struct or a committed API contract.

---

## Communities, Cooperatives, and Federations

The three non-individual sovereign entity kinds each embody a different Commons function. They share a substrate but not a purpose.

### Community

A **community** (`Entity::Community(CommunityProfile)`) is a civic/social Commons. Its primary function is collective life: care, ritual, learning, mutual aid, local memory, public life, conflict repair, welcoming. Communities may have treasury and governance, but their economic function is secondary to their social one. A neighborhood assembly, a care circle, a faith community, a study group, a maker-space circle, a regional potluck network — these are communities.

Communities tend to operate at human scale, with low formality relative to their depth. Their membership criteria are often affiliative rather than contractual.

### Cooperative

A **cooperative** (`Entity::Cooperative(CooperativeProfile)`) is an economic Commons. Its primary function is production, labor, services, treasury, mutual credit, and surplus allocation. A worker co-op, a housing co-op, a consumer co-op, a multi-stakeholder co-op, a producer co-op — these are cooperatives.

Cooperatives carry more formal governance than most communities because they coordinate around shared economic risk. Their membership criteria are contractual: a member accepts the charter and the corresponding economic obligations.

Communities and cooperatives are **not hierarchical** relative to each other. A community does not "become" a cooperative when it matures; a cooperative is not a "more serious" community. They are different kinds of Commons optimized for different kinds of holding-in-common.

### Federation

A **federation** (`Entity::Federation(FederationProfile)`) is a coordination layer between sovereign entities. Federations do not govern their members' internal affairs. Federations coordinate shared initiatives, maintain shared infrastructure, admit new member entities, and express joint positions. Members join federations voluntarily and retain their sovereignty.

> **Crucial rule**: a federation must not govern the internals of a member entity unless the member entity's own charter explicitly grants the federation scoped authority over that internal matter. The default is subsidiarity.

NYCN is a federation (institutional spec lives in the partner NYCN repo). GreenStar Cooperative is one of its sovereign members. NYCN does not decide how GreenStar organizes labor; GreenStar decides how it participates in NYCN.

### Subsidiarity as Doctrine

> **Decisions happen at the smallest competent scope.**

A decision that affects only a working group should be made by the working group, not escalated to the committee. A decision that affects only a cooperative should be made by the cooperative, not escalated to the federation. Authority moves *upward* only when the smaller scope is structurally incompetent to decide — for example, admitting a new federation member, or amending a shared charter.

Subsidiarity is not a runtime check. It is a design discipline for charters and for CCL rules. ICN enforces the charter; the charter decides where each class of decision lives.

---

## Delegation vs Representation

These two concepts are often conflated and are *not* the same. Keeping them separate matters for every feature that touches voting, authority, and receipts.

### Delegation (individual → individual)

**Delegation** is an individual DID handing their own voting power to another individual DID, inside a governance domain. The represented party is still *themselves* — they have chosen to let someone else cast their vote. Delegation is the repo's existing `icn-governance::delegation` primitive (1717 LOC, mature).

Current delegation semantics (as shipped — see `icn/crates/icn-governance/src/delegation.rs`):

- Delegation is **domain-scoped** — a delegation in one governance domain does not apply to another.
- Delegation can be **blanket, domain-scoped, proposal-scoped, or topic-scoped**, per the delegation type.
- Delegation is **revocable** by the delegator at any time.
- Delegation carries an **expiry**.
- Delegation cycles and self-delegation are rejected by the validation path.
- **Direct vote overrides delegated vote**: if Alice delegates to Bob on proposal P, and then Alice casts her own vote on P, Alice's direct vote wins.

Delegation does **not** change who is voting's *subject*. Alice has not become Bob. Bob is casting Alice's vote as her delegate. The receipt shows: voter = Alice, signed by = Bob (as delegate), authority = delegation record `D`.

### Representation (entity → human)

**Representation** is categorically different. A cooperative, a community, or a federation is an entity. Entities do not have fingers. They cannot literally sign. When a federation casts a vote, some human has to actually press the button with their key. That human is a **representative**, not a delegate.

The human's signature is the cryptographic vehicle. The entity is the subject of the action. The receipt must reflect both.

Authority for representation comes from:

- a `RoleAssignment` with `authority_scope` strings (the shipped capability-string surface),
- an `AuthorityGrant { class: AuthorityClass::Representation, grantor: <entity>, grantee: <person DID>, scope: TypedScope, … }` (the ADR-0014 typed layer),
- a specific `Mandate` tied to a named decision (the execution-obligation layer), or
- a charter clause that directly appoints a role holder.

Every representative action must be **receipt-backed**: the receipt records the represented entity, the human signer, the authority source, the decision or action, the scope / domain, the timestamp, and the resulting state change. Without this provenance, entity-level authority is indistinguishable from personal preference.

> A `RepresentativeVote` / `EntityAction` type — a single object that binds *(represented entity, human signer, authority source, action payload, scope, provenance, resulting receipt)* — does not yet exist as a first-class primitive. Today, the relationship is reconstructed from a combination of JWT `coop_id` scope, `RoleAssignment`, and the proposal/vote record. Formalizing this is `[future design]`. See §14.

### Worked example: GreenStar votes in NYCN

> GreenStar Cooperative is a member of NYCN Federation. Alice DID is authorized by GreenStar to cast GreenStar's NYCN federation vote. Alice is not voting as Alice. Alice is voting as GreenStar.
>
> The receipt must show:
>
> - **represented entity**: `entity:icn:cooperative:greenstar`
> - **human signer**: `did:icn:alice-pubkey`
> - **authority source**: `AuthorityGrant{ id, class: Representation, grantor: greenstar, grantee: alice, scope: nycn-federation-gov, valid_from, valid_until }` — plus optionally a specific `Mandate` if the vote was decided in a GreenStar internal proposal
> - **action**: `CastVote { proposal: <id>, choice: <…> }`
> - **scope / domain**: `nycn-federation-gov`
> - **timestamp**: issued at
> - **resulting state change**: vote tallied; `Vote` record with `voter: greenstar`, `signed_by: alice`, `authority: <grant_id>`
>
> If Alice ceases to be a GreenStar delegate, a *future* Alice-signed vote on NYCN proposals must no longer tally for GreenStar. The `valid_until` and `revoked_at` fields on `AuthorityGrant` are the enforcement surface.

The doctrine carried by this example: **a person can be authorized to act for an entity, but the identity of the acting subject is the entity**. UX must reflect that. Receipts must reflect that. Tally semantics must reflect that.

---

## The Commons Shell

A **Commons Shell** is the member-facing doorway into The Commons. It is not a dashboard. It is not a wallet. It is not an admin panel. It is not a social feed. It is the piece of software a person opens to participate in the institutions they belong to.

Today, ICN's member-facing surfaces are split across:

- `web/pilot-ui/` (browser PWA served by the gateway),
- `sdk/react-native/examples/CoopWallet/` (mobile wallet, 21 screens),
- `icnctl` CLI (power user + operator),
- `icn-console` (TUI),
- a variety of SDIS / steward / anchor pages.

None of these are a full Commons Shell today. The pilot UI is cooperative-scoped and auth-centric. CoopWallet leans toward payment/identity primitives. `icnctl` assumes operator fluency. A Commons Shell is the member-facing thing these could converge toward.

The Commons Shell answers:

- **Where do I belong?** Which entities list me as a member? Which structures have me assigned? Which federations list entities I represent?
- **What scope am I acting in right now?** Self? Member of community X? Officer of cooperative Y? Representative of federation Z? Operator of node N?
- **What can I do here?** Given my active scope, what capabilities do I hold, and therefore what actions are available?
- **What needs my attention?** Open votes, pending action items, overdue obligations, membership requests, expiring mandates, meeting attendance, budget approvals.
- **What decisions are open?** Proposals under deliberation in domains I vote in.
- **What work is assigned or available?** Action items assigned to me; action items unassigned but in scopes I can claim.
- **What help is needed, requested, or offered?** Care requests, mutual aid, coordination needs.
- **What resources are shared?** Treasury positions, commons pools, shared tools.
- **What meetings, events, or rituals are coming?** Calendar view across my scopes.
- **What knowledge is being passed on?** Institutional documents, meeting minutes, program milestone handoffs.
- **What was decided?** History of accepted proposals, with receipts, in scopes I belong to.
- **What receipts prove it?** Direct receipt viewing for any decision, action, settlement, or mandate.
- **What mandates do I hold?** Active representation authority: which entities, which domains, which expirations.
- **What can I verify?** Steward proofs, trust edges, identity proofs for others.

### Design Obligations

A Commons Shell must respect:

- **Accessibility is not optional.** Screen reader, high-contrast, low-literacy, non-English — these are first-class. See the Visual System (`docs/design/ICN_VISUAL_SYSTEM.md`).
- **Low-end device reach.** A Commons Shell must run on inexpensive phones, shared devices, kiosk terminals, library workstations, and older laptops. If it requires a flagship device, it is not a Commons tool.
- **Offline-first member flows.** Vote queues, payment queues, trust attestations, and receipt views work without connectivity and sync on reconnect. The CoopWallet offline queue is the existing pattern.
- **Public-terminal / library / community-center access.** Short sessions, no persistent state on the device beyond what the member explicitly saves, explicit sign-out, no silent lingering.
- **Scope-aware UX.** Every screen knows which scope the member is currently acting in. Scope-switching is a first-class gesture, not a hidden flag. Mis-scoping is the most common form of institutional confusion; the UI must make it visible.
- **Receipt visibility.** A member can ask "why did this happen?" on any entry and reach the signed provenance in at most a few taps. Receipts are not admin-only.
- **Action cards as the primary surface.** Home is a list of things that need this person's attention, ranked by urgency and scope (see §10).

> A capitalist app asks how to keep you scrolling. A Commons shell asks how to help you participate in shared life.

The mobile spec (`docs/mobile/icn-mobile-ux-spec-v1.md`) and the archived human-interface notes (`docs/archive/2026/plans-scratch/human-interface.md`) contain raw material for promoting this concept into current design docs. That promotion is `[future design]`.

---

## Standing and Active Scope

Two surfaces are named here because they shape every member flow that follows. The first two below are shipped; the active-scope primitive remains `[future design]`.

### `GET /me/standing` `[shipped, #1627]`

A single authenticated query that returns a member's full participatory position across ICN. Suggested shape (non-binding):

```text
GET /me/standing
Authorization: Bearer <JWT>

→ {
    self: {
      did: "did:icn:alice-pubkey",
      individual_entity_id: "entity:icn:individual:alice-pubkey",
      devices: [...],
    },
    memberships: [
      { entity_id: "entity:icn:cooperative:greenstar", role, status, shares, capabilities, joined_at },
      ...
    ],
    role_assignments: [
      { structure_id, parent_entity_id, role, authority_scope: [...], valid_from, valid_until },
      ...
    ],
    mandates: [
      { mandate_id, represented_entity, decision: <provenance>, grants: [...], deadline, status },
      ...
    ],
    authority_grants_held: [
      { grant_id, class: Representation | Execution | Attestation, grantor, scope, valid_until, revoked_at },
      ...
    ],
    representable_entities: [
      { entity_id, via_grant: <grant_id>, domain_scopes: [...] },
      ...
    ],
    available_active_scopes: [
      { kind: "self", label: "Acting as self" },
      { kind: "member", entity_id: "...", label: "Member of GreenStar" },
      { kind: "role",   structure_id: "...", label: "Finance Coordinator, NYCN" },
      { kind: "representative", represented_entity: "...", via_grant: "...", label: "Representing GreenStar in NYCN" },
      { kind: "operator", node_id: "...", label: "Node operator: home-01" },
    ],
    capabilities_by_scope: { <scope_key>: [capability, ...], ... },
    pending_expirations: [
      { kind: "role_assignment" | "mandate" | "grant", id, expires_at },
      ...
    ],
    pending_revocations: [ ... ],
  }
```

This does **not** authorize any implementation. It names the target so that when the pieces exist (`/gov/me/scopes` already does; memberships and authority grants can be joined) the unified query has a shape to aim at.

### Active Scope

Every action a member takes should be bound to an **explicitly selected active scope**. Scope options include:

- **Self** — `scope: { kind: "self" }`. Actions are the member's own (personal trust attestations, their own ledger position, their own key operations).
- **Member** — `scope: { kind: "member", entity_id: <id> }`. Actions taken as an ordinary member of an entity (vote in a community proposal, propose in a cooperative, log hours).
- **Role** — `scope: { kind: "role", structure_id: <id> }`. Actions taken as a role-holder inside a structure (officer, coordinator, committee member).
- **Representative** — `scope: { kind: "representative", represented_entity: <id>, via_grant: <grant_id> }`. Actions whose subject is a represented entity, not the signer.
- **Operator** — `scope: { kind: "operator", node_id: <id> }`. Actions taken as a node operator (maintenance, federation peering, trust seeding).

**Hidden authority is capture.** If a member can take an action and there is no way to answer "under what scope?", the Commons Shell has failed and the receipt has failed. Every receipt must name the active scope used. Every authority check must evaluate capability against the *selected* scope, not the union of all scopes the member could have chosen.

An explicit active-scope / session-selected-scope primitive does not yet exist in the codebase. Today the approximation lives in JWT `coop_id` + `scopes` claims and is not uniformly applied across the ledger, governance, and trust surfaces. Formalizing this is `[future design]` (see §14).

---

## Action Cards

An **action card** is a generated call-to-action item surfaced in the Commons Shell. It is not a notification, not a feed post, not a marketing message. It is a scoped, typed, dismissible prompt that says: *"This is a thing in the Commons that needs you."*

`GET /me/action-cards` `[shipped, #1659]` — a unified query that returns the ranked list of action cards for the authenticated member across all their scopes. Three of five source paths currently emit proof-bearing receipts (`proposal/vote` → `GovernanceDecisionReceipt`; `action_item/complete` → `ActionItemCompletionReceipt`; `meeting/attend` → `MeetingAttendanceReceipt`). Two source paths remain RFC-gated under umbrella issue #1646: `signal_rule` (gated on #1631) and `obligation_lifecycle` (gated on #1634). Completion-receipt retrieval (`GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`) shipped in #1675.

### Sources

An action card may be generated from:

- **Open proposals** in a governance domain where the member has `Vote` capability.
- **Pending votes** (the member has not yet cast on a proposal they can vote on).
- **Assigned action items** (`ActionItem` records where `assignee == caller`, as already surfaced by `/gov/me/work`).
- **Upcoming meetings** the member is invited to.
- **Role or mandate expirations** (a `RoleAssignment.valid_until` or `AuthorityGrant.valid_until` approaching).
- **Membership requests** where the member has `Invite` or `ManageSubEntities` capability in the inviting entity.
- **Budget approvals** where the member is in the approval chain.
- **Care requests** (mutual aid, dispute support, accessibility help).
- **Program milestone prompts** (a program cycle entering a new phase, a check the member is accountable for).
- **Receipt verification requests** (another member has asked the member to verify an identity proof, a trust attestation, a steward enrollment).
- **Node maintenance items** (if the member is also a node operator).
- **Appeals or amendments** the member is qualified to respond to.

### Doctrine

> A capitalist app asks how to keep you scrolling. A Commons shell asks how to help you participate in shared life.

Action cards are designed for closure, not engagement. A card that stays open forever is a failure. A card that gets dismissed, acted on, delegated, or timed out and archived is working.

Action cards are **scoped**. Each card is generated in the context of one of the member's active scopes; rendering must show which scope the card belongs to so the member never acts in the wrong hat.

A card generation model — predicates, ranking, dismissal, expiry, cross-scope deduplication — does not exist in the codebase today. The existing `/gov/me/work` and the notification digest work (PR #1547) are partial predecessors.

---

## Physical Commons and Nodes

ICN runs on existing, ordinary technology first. The first operational target is never a custom appliance. The first operational target is whatever the community already has.

### Current Reach

- Modest **phones** (iOS + Android) via `sdk/react-native/examples/CoopWallet/`.
- **Browsers** (desktop + mobile) via `web/pilot-ui/` served by the gateway.
- Ordinary **Linux boxes** (home servers, office workstations) running `icnd`.
- **Raspberry Pi / single-board computers** for community nodes.
- **Library and community-center terminals** (browser-based access, no install).
- **Homelab servers** running containerized `icnd` under K3s or Docker Compose (see `deploy/README.md`, `docs/HOMELAB_DEPLOYMENT.md`).
- **Community nodes** run by volunteer operators — the current live K3s cluster is the canonical example.
- **Cheap laptops** for organizers and stewards.
- **Event kiosks** (forthcoming, via the Commons Shell concept).
- **Offline-first local networks** — mDNS discovery and gossip already work on isolated LANs.

**ICN turns existing hardware into Commons infrastructure.** The binary target is unremarkable hardware in the hands of cooperatives, communities, and federations.

### Commons OS `[future design]`

A longer-horizon concept: a **Commons OS** distribution — a node appliance, kiosk mode, mobile shell, desktop environment, or app runtime tuned specifically for Commons participation. Possible shapes include:

- An `icnd`-first Linux image for community nodes (plug in, enroll, peer).
- A kiosk-mode browser build of the Commons Shell for libraries and community centers.
- A mobile-first shell distribution where the phone itself is the primary member device.
- A local-first archive and content-addressed shared-memory layer across community nodes.
- A distributed storage/compute pool across homelab operators.
- A package manager for Commons apps and institution packages (CCL templates, oracles, UI overlays).

None of these are on any roadmap as of 2026-04-24. Naming them here is a hedge against feature creep into the current ICN kernel: Commons OS ambitions live **outside** the kernel/app boundary, not inside it.

> **First, ICN turns existing hardware into Commons infrastructure. Only later — if ever — does a dedicated Commons OS make sense.**

---

## Institution Transformation Examples

The Commons transforms inherited institutions by decomposing them into human functions and rebuilding from primitives. The list below is illustrative, not exhaustive. Each transformation names the existing ICN primitives that would compose it. None of these is an endorsement of a specific institution design; each is a *sketch* of how the primitives carry the weight.

Use the composition shorthand: **E** = Entity, **S** = Structure, **A** = Activity, **P** = Program, **M** = Meeting, **Pr** = Proposal, **R** = Role / RoleAssignment, **C** = Charter, **L** = Ledger / Treasury, **Rc** = Receipt / Provenance, **F** = Federation relationship, **N** = Node / operator layer.

| Inherited institution | Commons transformation | Primitive composition |
|-----------------------|------------------------|------------------------|
| **Church** | Ritual / Care / Meaning Commons — a community holds a tradition of ritual, care, and meaning without a corporate hierarchy above it. Elders are role-holders, not owners. | E (community) + C (charter of tradition) + S (care circle, liturgy working group) + A (annual cycle of observances) + M (gatherings) + R (elder, steward, care coordinator) + Rc (minutes, commitments). |
| **Corporation** | Production Commons — a worker-owned cooperative holds productive capacity directly, with federated coordination across similar cooperatives. | E (cooperative) + C (charter) + S (committees: finance, operations, stewarding) + A (annual plan) + P (campaigns, product cycles) + Pr (budget, hiring, strategy proposals) + L (treasury, mutual credit) + F (industry or regional federation) + Rc (decision & settlement receipts). |
| **Bank** | Mutual Credit Commons — credit is bilateral and cooperative, not extracted. No custodial float. No external settlement rail. | E (cooperative or federation) + C (credit charter) + L (double-entry ledger, zero-sum invariant) + Pr (credit-policy changes, credit-ceiling adjustments) + Rc (every settlement). Regulatory framing: units, positions, settlement — never currency, balance, payment. |
| **School / University** | Learning / Knowledge Commons — curriculum, stewardship of knowledge, peer recognition, intergenerational memory held by a community rather than sold by a platform. | E (community or federation of learning circles) + C (learning charter) + S (subject circles) + A (cohorts) + P (multi-year programs with milestones) + M (seminars, defenses) + R (teacher, steward, peer) + Rc (recognition receipts, credential provenance). |
| **Social media** | Communication Commons — topic-scoped federated gossip, no engagement engine, no algorithmic feed, moderation is local and accountable. | ICN gossip primitives + E (community) + C (community norms charter) + S (moderation circle) + Pr (norms amendments) + Rc (moderation receipts). The kernel already provides the gossip; what is missing is member-facing shells. |
| **Insurance** | Risk Commons — mutual insurance pool, not investor-owned. Claims are governed by members. | E (cooperative) + C (pool charter) + L (pool treasury + contribution flow) + Pr (claim decisions, pool parameter changes) + Rc (claim settlement receipts). |
| **Hospital / clinic network** | Care Commons — patient-owned or practitioner-owned cooperative care network, federated regionally. | E (cooperative of clinics) + S (specialty circles, ethics committee) + A (service cycles) + R (practitioner, patient-representative, steward) + L (solidarity fund, contribution flow) + F (regional care federation) + Rc (care provenance; never medical-record oversharing). |
| **Local government functions** | Civic Commons — participatory budgeting, neighborhood assembly, dispute resolution at the smallest competent scope. | E (community) + C (civic charter) + Pr (Allocation for participatory budgeting, Text for resolutions) + M (assembly meetings) + R (rotating coordinator roles) + Rc (decisions published and verifiable). |
| **Supply chain** | Federated Production / Distribution Commons — producer, processor, distributor cooperatives federated with published terms and mutual credit clearing. | Multiple E (each coop) + F (federation) + L (inter-coop mutual credit, settlement) + Pr (shared-initiative proposals) + Rc (cross-coop settlement receipts). |
| **Public library** | Physical Commons node — the library is itself a community node: a place with terminals, stewards, shared knowledge, access for members who have no other point of entry. | E (community) + N (library node running `icnd`) + R (librarian-steward) + Commons Shell on local terminals + Rc (lending, access, membership receipts). |

These transformations are not "apps" to be built by the ICN core team. They are the *use-cases* Commons institutions — with their own apps, charters, and institution packages — build on top of ICN. The core team's obligation is to keep the substrate general enough that none of these requires bespoke kernel support.

---

## Boundary Rules

The Commons doctrine above is worthless unless it constrains engineering decisions. The following rules are binding.

1. **Generic institutional shapes live in ICN core crates.** `Entity`, `Membership`, `Structure`, `RoleAssignment`, `Activity`, `Program`, `Milestone`, `Meeting`, `Proposal`, `Vote`, `Delegation`, `Mandate`, `AuthorityGrant`, `ActionItem`, `InstitutionalParent`, `Charter`, ledger entries, receipts, invariants — these are generic and belong in `icn-governance`, `icn-entity`, `icn-identity`, `icn-ledger`, `icn-kernel-api`.

2. **Institution-specific vocabulary lives in institution packages.** Sponsor tiers, conference sessions, summit tracks, committee nickname conventions, jurisdiction-specific registration fields, donor levels, volunteer shift patterns — these belong in `institutions/<name>/` (currently `institutions/nycn/`, to be split out as an external repo). See `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`.

3. **NYCN- and Summit-specific semantics must not leak into core.** No `ProgramKind::AnnualSummit`, no `MilestoneType::VenueLocked` in `icn-governance`. Named institution milestones live in CCL contract bodies; institution-kind escapes use `Custom(String)` on generic enums. The authoritative routing correction in `INSTITUTION_PACKAGE_BOUNDARY.md` is the enforcement surface.

4. **Commons doctrine must not weaken the Meaning Firewall.** Everything in this doc that sounds like social meaning (trust, care, standing, representation, scope) is app-layer and/or shell-layer. The kernel continues to see only `ConstraintSet`, `PolicyDecision`, `Capability`, `Did`, `Timestamp`, `Hash`, and the typed invariant attestations. See `docs/architecture/KERNEL_APP_SEPARATION.md` (Invariants 1–5).

5. **The Commons Shell may render social meaning; the kernel must enforce constraints without knowing that meaning.** A shell may show "Alice is voting as GreenStar's representative in NYCN". The kernel sees a signed vote message whose constraints passed a `PolicyOracle` check; it does not know that "NYCN" is a federation or that "GreenStar" is a worker cooperative.

6. **Receipts and provenance remain central.** Every state transition — vote, settlement, role assignment, grant, mandate, admission, expulsion, amendment — emits a receipt. Commons legitimacy rests on verifiable memory. This is frozen-core invariant 10 (Universal Receipt Generation). See `docs/genesis.md`.

7. **Accessibility is not optional.** Any member-facing surface that does not work on a screen reader, on a low-end phone, in a second language, or from a public terminal fails the Commons Shell design test. This is a blocker-level concern, not a polish concern.

8. **Mobile / member interface is the main interaction point.** For most Commons members, `CoopWallet`-class mobile and browser PWA surfaces are the *primary* ICN experience. `icnctl` and `icn-console` are operator surfaces. Architectural decisions that assume the member is an operator are a category error.

9. **Bounded authority.** No wildcard capabilities, no unbounded grants, no eternal mandates (frozen-core invariants 5 and 7). Every authority has scope and time bounds; every grant has a revocation path.

10. **Exit is non-negotiable.** A member can always leave an entity and export their keys, their receipts, and their authored state (frozen-core invariant 1; `icnctl id export`, `icnctl backup`). This is what makes The Commons a Commons and not a platform.

---

## Open Questions / Future Work

The following items are *not* authorized for implementation here. They are explicitly enumerated so that future design docs, issues, and PRs can pick them up without re-deriving the gap analysis.

1. **Define `GET /me/standing` query surface.** Unified standing query (memberships + role assignments + mandates + authority grants + available active scopes + capabilities per scope + pending expirations). Successor to, not replacement for, `/gov/me/scopes` and `/gov/me/work`. Needs a dedicated design doc.

2. **Define the active-scope / session model.** Every member action should carry an explicitly selected scope; the scope must appear in receipts. Today's JWT `coop_id`-based approximation is not uniform across the surfaces. Needs a model doc and a migration plan.

3. **Define the representative / entity-vote model.** A first-class type that binds `(represented_entity, human_signer, authority_source, action_payload, scope, provenance, receipt)`. `AuthorityClass::Representation` in ADR-0014 is the anchor; the vote-wrapper and the tally semantics need a spec. Tally semantics in federations is a pre-requisite.

4. **Define the Action Card generation model.** Predicates, ranking, dismissal, expiry, cross-scope deduplication, scope rendering. The existing notification digest and `/gov/me/work` are partial predecessors.

5. **Promote or rewrite `docs/archive/2026/plans-scratch/human-interface.md`.** Move it into current design docs (`docs/design/` or `docs/mobile/`) as the Commons Shell specification, aligned with `docs/mobile/icn-mobile-ux-spec-v1.md` and the pilot-ui / CoopWallet inventories.

6. **Connect Commons Shell concepts to pilot-ui / mobile / wallet surfaces.** Define the convergence path from today's split surfaces (pilot-ui PWA + CoopWallet + icnctl) to a single Commons Shell concept with shared scope semantics and shared action-card generation.

7. **Add NYCN worked examples.** Show the full person-community-cooperative-federation relationship graph in the NYCN topology, including representation flows and receipt shapes. The `docs/strategy/NYCN-*` docs are close but do not yet render the member-level standing and representation flows.

8. **Clarify how mandates expire, revoke, and appear in receipts.** `Mandate.status`, `Mandate.deadline`, and the decision-bridge path already exist; the member-facing view (what do I see when a mandate I hold approaches expiry? what do I see when an entity revokes my grant?) is not specified.

9. **Clarify entity-level voting eligibility and tally semantics in federations.** When GreenStar votes in NYCN, what weight does GreenStar carry? How do shares work at the entity-member level vs. the individual level? What happens when two humans authorized by GreenStar both attempt to vote on the same federation proposal? Needs a tally-semantics spec.

10. **Clarify node operator civic role and ordinary-operator dashboard needs.** Operators run infrastructure; they are also members of their communities. What does an operator-scope surface look like separate from, but integrated with, their member-scope surfaces? Should node operation itself be a charterable responsibility?

---

## References

**Canonical anchors (consulted while drafting):**

- `docs/genesis.md` — Constitutional Genesis: frozen-core invariants, bootstrap phases, hard invariants.
- `docs/ARCHITECTURE.md` — ICN Architecture Reference: what ICN is, what it isn't, terminology discipline.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — Meaning Firewall, PolicyOracle pattern, kernel/app invariants.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — Authoritative routing between ICN platform and institution packages.
- `docs/architecture/CELLS_AND_SCOPES.md` — Scope primitives at the substrate layer.
- `docs/design/ICN_VISUAL_SYSTEM.md` — Visual doctrine; human participation, scope-aware membership, proof-backed coordination.
- Partner NYCN repo (`InterCooperative-Network/nycn`) — NYCN-as-native-institution design and repo-shaped institutional spec (with the 2026-04-15 layered-ontology correction).
- `docs/strategy/ADR-001-What-ICN-Is.md` — ADR on ICN's definition.
- `docs/adr/ADR-0014-constitutional-object-model.md` — Typed authority model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`).
- `docs/mobile/icn-mobile-ux-spec-v1.md` — Current mobile UX spec.
- `docs/archive/2026/plans-scratch/human-interface.md` — Raw material for the Commons Shell concept (archived; pending promotion).
- `docs/STATE.md` — Current project state.
- `AGENTS.md` — Agent coding instructions and app topology rule.

**Code anchors (types referenced by name):**

- `icn/crates/icn-identity/src/lib.rs` — `Did`.
- `icn/crates/icn-identity/src/bundle.rs` — `IdentityBundle`.
- `icn/crates/icn-identity/src/multi_device.rs` — `Capability`.
- `icn/crates/icn-entity/src/entity.rs` — `EntityId`, `Individual`, `Cooperative(CooperativeProfile)`, `Community(CommunityProfile)`, `Federation(FederationProfile)`.
- `icn/crates/icn-entity/src/membership.rs` — `Membership`, `MembershipRole`, `MembershipCapability`.
- `icn/crates/icn-governance/src/structure.rs` — `Structure`, `RoleAssignment`.
- `icn/crates/icn-governance/src/activity.rs` — `Activity`.
- `icn/crates/icn-governance/src/program.rs` — `Program`, `Milestone`.
- `icn/crates/icn-governance/src/parent.rs` — `InstitutionalParent`.
- `icn/crates/icn-governance/src/domain.rs` — `GovernanceDomain`.
- `icn/crates/icn-governance/src/proposal.rs` — `Proposal`.
- `icn/crates/icn-governance/src/vote.rs` — `Vote`.
- `icn/crates/icn-governance/src/delegation.rs` — `Delegation`.
- `icn/crates/icn-governance/src/authority.rs` — `AuthorityGrant`.
- `icn/crates/icn-governance/src/mandate.rs` — `Mandate`.
- `icn/crates/icn-governance/src/meeting.rs` — `Meeting`.
- `icn/crates/icn-governance/src/action_item.rs` — `ActionItem`, `ActionItemSpec`.
- `icn/crates/icn-governance/src/charter.rs` — `Charter`.
- `icn/crates/icn-kernel-api/src/invariants.rs` — `InvariantId` (10 frozen-core invariants).
- `icn/crates/icn-kernel-api/src/bootstrap.rs` — `Capability`, `CapabilitySet`.
- `icn/apps/governance/src/http/handlers.rs` — `GET /gov/me/scopes`, `GET /gov/me/work`.

---

*This doc is canonical doctrine, not a plan. No feature in this document is authorized for implementation on the basis of this document alone. Each `[future design]` item requires its own design doc, issue, and review.*
