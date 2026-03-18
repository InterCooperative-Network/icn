# ICN Mobile Member UX Spec v1

**Status:** Draft
**Date:** 2026-03-18
**Author:** Matt Faherty
**Anchored to:** ICN repo at commit `72a8b7a3` (post feat/demo-federation-system merge)
**Purpose:** Build-facing spec for the ICN mobile member layer, anchored to real crates, gateway endpoints, and October 2026 demo flows.

**Governing sentence:**
ICN mobile is a member-centered semantic layer that projects real protocol objects into a portable, scope-aware participation experience across many cooperative and community contexts.

**Cross-references (convergence verified 2026-03-18):**
- `CLAUDE.md` — project status, identity, ICN framing rules ("Digital Public Infrastructure" / "Coordination OS")
- `BOOT.md` — current priorities (Coordination OS Phase 1, ICN integration, homelab hardening)
- `TASKS.md` — governance demo passed end-to-end Mar 17 (19 steps, exit 0); Phase 2 not started
- `icn-crate-reference.md` — 38 crates, gateway endpoints, React Native SDK components
- `icn-ecosystem-map.md` — 5 front-end surfaces + demo UI; pilot-ui NOT wired to real icnd yet
- `icn-forward-plan.md` — Phase 2 timeline (Mar 17–Apr 17); Track D = real demo calls
- `icn-vertical-slice-assessment.md` — 70+ endpoints, zero stubs; auth flow documented
- `icn-status-march-2026.md` — Phase 0+1 complete, ~75% overall
- `docs/strategy/ICN-Gap-Analysis-March-2026.md` (IN REPO) — subsystem-by-subsystem reality check; only governance flow proven end-to-end; federation settlement undefined; Mana descoped
- `docs/strategy/ICN-Sprint-March17.md` (IN REPO) — 2-week sprint (Mar 17-30) implementing forward plan at 70% capacity
- `docs/strategy/ICN-Roadmap-Live.md` (IN REPO) — grounded 90-day roadmap; NOW/NEXT/LATER tracks
- `docs/strategy/ADR-001-What-ICN-Is.md` (IN REPO) — Meaning Firewall formal definition; kernel/app boundary
- `docs/mobile/` (IN REPO) — 5 historical mobile docs from Dec 2024; SUPERSEDED by this spec

---

## 1. Member Shell

The app is organized around a persistent member, not an organization.

A member is a person who:
- holds a self-sovereign identity on their device (Ed25519 keys, `did:icn:<base58pubkey>`)
- belongs to zero or more cooperatives, communities, and federations simultaneously
- acts under different roles and charters depending on active scope
- carries their identity, local state, and signing authority across the network

The device is the member's:
- **Identity anchor** — keys stored locally under age encryption (`.ICN_DATA_DIR/identity/keypair.age`)
- **Signing agent** — all actions signed locally before network submission
- **Local cache** — recent state cached for offline operation
- **Participation surface** — where the member sees what they can do and acts on it
- **Continuity layer** — preserves member identity and context across scope transitions

### What follows the member across the network (portable)

- Core identity (DID, keys, device metadata)
- Membership list (all entities the member belongs to)
- Personal participation history (cross-scope activity view)
- Recovery state (backup phrases, recovery contacts)
- Notification preferences
- Saved filters and views
- Cross-scope action queue (Home)

### What stays scoped to an entity

- Charter rules and governance configuration
- Role powers and permissions
- Local obligations and their states
- Local positions (per-unit resource state)
- Local trust relationships (per-dimension, per-scope)
- Scope-specific activity records
- Scope-specific governance context

### Onboarding flow (member-first, not org-first)

1. Establish or import identity on device (`icnctl id create` equivalent)
2. Secure device (biometric unlock for signing, recovery setup)
3. Enroll into one or more memberships (via invite, QR, or manual)
4. Sync accessible scopes
5. Land on member Home (cross-scope action queue), not an org dashboard

---

## 2. Navigation Contract

Five tabs. Mobile bottom nav. No more.

| Tab | Job | Contains | Does NOT contain |
|-----|-----|----------|------------------|
| **Home** | What needs me now, across all memberships | Cross-scope action queue, aggregated by urgency and scope | Dashboards, metrics, vanity stats |
| **Groups** | My memberships and scopes | Entity cards (coop, community, federation), role indicators, charter badges, scope switcher | Org-centric "home pages" |
| **Participate** | What I can do in the active scope | Proposals, obligations, flow-relevant actions, unified under one surface | Separate "Governance" / "Work" / "Exchange" tabs |
| **Activity** | What happened, with proof | Proof-backed event stream, filterable by scope and type | Infinite social feeds, algorithmic ranking, engagement tricks |
| **Profile** | Who I am, across the network | Identity, devices, recovery, trust inspection, member-level settings | Org-specific settings (those live in Groups) |

### Rules

- **Participate** is the core working surface. It reunifies governance + obligations + flows into one participation experience. The kernel separates these concerns architecturally; the UX reunifies them for the member.
- **Home** aggregates across memberships. A member should not have to visit each coop separately to discover pending actions.
- **Groups** is the scope/membership manager. Switching scope here changes the context for Participate and Activity.
- **Activity** is not notifications. It is the human-readable view into proof-backed events, status changes, dispute states, and cross-scope changes.
- **Profile** includes the honest state of device management (v1 limitations included).

---

## 3. Gateway-to-View Projection Map

Every mobile view renders projections of real ICN types. Cards are rendering primitives, not data ontologies.

### 3.1 Proposal → Action Card (Home + Participate)

**Source:** `GET /v1/gov/proposals/{id}` + `GET /v1/gov/proposals/{id}/tally`

| Card field | Source field | Notes |
|-----------|-------------|-------|
| Title | `proposal.title` | |
| Type badge | `proposal.payload_type` | `text`, `budget`, `membership`, `config_change` |
| Scope chip | `proposal.domain_id` → entity lookup | Shows entity name + type icon |
| Status | `proposal.status` | `Draft`, `Open`, `Accepted`, `Rejected`, `NoQuorum` |
| Summary | `proposal.description` (truncated) | First 2-3 sentences |
| Impact line | Derived from payload type | Budget: "Allocate X units to Y". Membership: "Add/remove Z". Config: "Change K to V" |
| Threshold info | From governance domain | "Requires 2/3 approval, 5 of 7 quorum" (charter-derived) |
| Deadline | `proposal.voting_period_days` + created timestamp | "Closes in 3 days" |
| Primary action | Based on status + member role | `Vote` (if Open + eligible), `Review` (if Draft + author), `View Result` (if closed) |
| Proof affordance | Link to `/v1/gov/proposals/{id}/proof` | "View proof" button on closed proposals |

**Appears in:** Home (if vote needed), Participate (always in active scope)

### 3.2 Obligation → Action Card (Home + Participate)

**Source:** Obligation state machine (issue #1306 — `Issued → Accepted → Settled → Defaulted → Disputed`)

| Card field | Source field | Notes |
|-----------|-------------|-------|
| Title | Obligation summary | Derived from originating proposal/allocation |
| Type badge | `obligation` | |
| Scope chip | Entity context | |
| Status | Obligation state | Color-coded: Issued (blue), Accepted (green), Disputed (red) |
| Provenance link | `ProvenanceRef` on originating `JournalEntry` | "Authorized by Proposal #X" |
| Primary action | Based on state | `Accept` (if Issued to me), `Confirm Settlement` (if Accepted), `Dispute` (if concerned) |

**Appears in:** Home (if action needed), Participate (in active scope)

### 3.3 GovernanceProof → Detail Panel (Participate + Activity)

**Source:** `GET /v1/gov/proposals/{id}/proof`

Renders the provenance chain for a closed proposal:
- Decision hash
- Voter attestations (each signed with Ed25519)
- Verification status (the gateway verifies proofs before serving them — invalid proofs return 404)
- Link to related ExecutionReceipt if one exists

**Appears in:** Participate (detail view on closed proposals), Activity (as proof-backed event)

### 3.4 ExecutionReceipt → Provenance Link (Participate + Activity)

**Source:** `icnctl audit receipt-chain` equivalent via gateway

Shows the chain: Proposal → GovernanceProof → AllocationReceipt → ExecutionReceipt

**Appears in:** Detail views on obligations and allocations, Activity stream

### 3.5 Trust Dimensions → Trust Panel (Profile + Groups)

**Source:** `icn-trust` crate — three independent graphs

| Dimension | Scoring | Member-facing label |
|-----------|---------|---------------------|
| Social | 60% direct + 40% transitive | "Known through" |
| Economic | 80% direct + 20% transitive | "Relied on for" |
| Technical | Configurable | "Operates" |

Each dimension displays:
- Trust class: `isolated` / `known` / `partner` / `federated`
- Evidence basis (endorsements, history, attestations)
- Scope context (trusted in which entity)

Never collapse into a single score. "Trusted economically but unknown socially" is meaningful information.

**Appears in:** Profile (own trust standing), Groups (trust context for entities), Participate (trust badges on counterparties)

### 3.6 Entity → Membership Card (Groups)

**Source:** `GET /v1/coops/{id}` + entity model (`entity:icn:<type>:<identifier>`)

| Card field | Source field | Notes |
|-----------|-------------|-------|
| Name | `entity.name` | |
| Type | `entity.type` | `cooperative`, `community`, `federation` |
| Role | Member's role in this entity | `member`, `steward`, `delegate` |
| Charter badge | Charter type if known | "Worker Coop", "Community Land Trust", etc |
| Pending actions | Count of open items in this scope | "3 votes pending" |
| Switch action | Tap to set as active scope | Updates Participate + Activity context |

**Appears in:** Groups

### 3.7 Activity Event → Event Card (Activity)

**Source:** Gateway WebSocket event stream + cached activity records

| Card field | Source | Notes |
|-----------|--------|-------|
| Event type | Enum | `proposal_created`, `vote_cast`, `obligation_issued`, `settlement_confirmed`, etc |
| Summary | Human-readable description | "Proposal 'Roof Repair' accepted (5 for, 1 against)" |
| Scope | Entity context | |
| Timestamp | Event timestamp | |
| Proof link | If proof-backed event | "View proof" |
| Status indicators | Sync state | `local_pending`, `synced`, `confirmed`, `disputed` |

**Appears in:** Activity (primary), Home (important recent events)

---

## 4. Vocabulary Contract

The ICN codebase enforces regulatory-safe language via CI linter (issue #1312). The mobile UX must match.

### Required substitutions

| Never use | Always use | Why |
|-----------|-----------|-----|
| payment | settlement | ICN does not route payments |
| currency | unit | ICN tracks units of account, not currencies |
| balance | position | ICN does not hold balances |
| transaction | flow record / activity record | Not financial transactions |
| wallet | identity / device | Not a financial wallet |
| debt | obligation | Obligations have lifecycle states, not debt semantics |
| transfer | resource movement / settlement | Not money transfer |
| account | position record | Not a financial account |
| ledger | activity record / flow history | Avoid "ledger" in member-facing UX |
| execution receipt | action verification / completion record | Codebase type is `ExecutionReceipt`; member-facing label should be human-readable |
| allocation receipt | authorization record | Codebase type is `AllocationReceipt`; member sees "authorized by [proposal]" |
| governance proof | decision proof / verification | Codebase type is `GovernanceProof`; member sees "verified decision" |

### Seven Invariants (must be reflected in UX behavior)

1. **User-signed transitions only** — no action happens without the member's cryptographic signature
2. **No hosted balances** — positions are computed views, not custodial holdings
3. **No operator routing of value** — the network does not move resources; members do
4. **Derived views are not authoritative** — the proof chain is authoritative; summaries are convenience
5. **No embedded convertibility** — units are not automatically exchangeable
6. **Matching/market features are opt-in** — and governance-authorized
7. **Execution receipts close the governance loop** — every resource action traces back to a governance decision

### UX copy principles

- Use "position" not "balance" in summary views
- Use "flow history" not "transaction history"
- Use "activity record" not "ledger entry"
- Use "settlement" not "payment" when describing resource movement
- Use "obligation" not "debt" for tracked commitments
- When showing provenance: "Authorized by [proposal name]" not "Created by [system]"
- When showing sync state: "Saved on your device" → "Sent to network" → "Confirmed by network"
- When showing trust: "Known through [evidence]" not "Trust score: 82"

---

## 5. Provenance Rendering Contract

The actual provenance chain in the codebase:

```
Proposal
  → GovernanceProof (blake3 hash, voter attestations, Ed25519 signatures)
    → AllocationReceipt (links governance decision to resource allocation)
      → ExecutionReceipt (PR #1327, Invariant 7 enforcement)
        → Obligation state change (Issued → Accepted → Settled)
          → JournalEntry (Merkle-DAG linked, required ProvenanceRef)
```

### How this renders on mobile

**Every important object should expose a "Why this exists" affordance.**

#### On a Proposal detail view:

- **What this changes** — derived from payload type
- **Who authorized** — governance domain + quorum rules
- **Current proof** — link to GovernanceProof (if closed)
- **What followed** — link to any AllocationReceipt or ExecutionReceipt

#### On an Obligation card:

- **Authorized by** — link to originating Proposal
- **Proof** — link to GovernanceProof that authorized it
- **Execution** — link to ExecutionReceipt if settled

#### On an Activity event:

- **Source** — what governance decision or obligation triggered this
- **Proof** — cryptographic proof reference (if proof-backed)
- **Chain position** — where in the Merkle-DAG this sits

### Receipt verification happens on device

Per ICN-Scenarios.md (Scenario 6), receipt verification works locally on the member's device via the React Native SDK. The member does NOT need to contact the issuing organization's daemon to verify a receipt. This is a critical security and offline property — verification is self-contained once the receipt data is cached.

### Progressive disclosure pattern

1. **Card view:** One-line provenance summary ("Authorized by Proposal: Roof Repair")
2. **Tap to expand:** Full provenance chain with links to each step
3. **Deep inspection:** Raw proof data, hashes, attestation signatures

### CLI audit equivalents on mobile

| CLI command | Mobile equivalent |
|------------|-------------------|
| `icnctl audit receipt-chain --receipt <hash>` | Tap "View full chain" on any proof-backed object |
| `icnctl audit allocation-receipt --hash <hash>` | Tap allocation link in provenance chain |
| `icnctl audit settlement-intent --hash <hash>` | Tap settlement link in provenance chain |

---

## 6. Scope + Role Switching Spec

### The scope model

A member's active context is defined by:

```
ActiveContext:
  entity_id: EntityId          # entity:icn:<type>:<identifier>
  entity_type: EntityType      # cooperative | community | federation
  role: MemberRole             # member | steward | delegate | (charter-defined)
  charter: CharterRef          # which governance rules apply
  permissions: Vec<Scope>      # governance:read, governance:write, coop:write, etc
```

### Switching behavior

When scope changes:
- **Participate** reloads with the new entity's proposals, obligations, and flow records
- **Activity** filters to the new scope (with option to view cross-scope)
- **Available actions change** based on role + charter + permissions
- **Vocabulary may change** (charter-specific terminology)
- **Threshold displays change** (different charters have different quorum/approval rules)

### Scope switcher UI

Persistent element accessible from any tab (likely top bar or Groups tab):

```
[Scope indicator: Entity name + Role badge]
  ↓ tap to open
[Membership list]
  - Rochester Food Coop (Steward)     [worker-coop charter]
  - Harbor Homes (Member)              [community-land charter]
  - Finger Lakes CDN (Delegate)        [federation charter]
  - Self (no active entity scope)
```

### Entity type indicators

| Entity type | Icon concept | Scoping behavior |
|------------|-------------|------------------|
| Cooperative | Economic/production symbol | Shows obligations, positions, patronage |
| Community | Civic/solidarity symbol | Shows governance, mutual aid, shared resources |
| Federation | Network/bridge symbol | Shows cross-entity coordination, federation proposals |

### Cooperatives and Communities are co-equal

The scope switcher must NOT present these as a hierarchy:
- ❌ Individual → Cooperative → Community → Federation (ladder)
- ✅ Individual can belong to cooperatives, communities, and federations independently

A person can be a steward in a cooperative and a member of a community simultaneously. These are parallel memberships, not nested containers.

---

## 7. Charter-Adaptive Rendering Rules

When the active scope changes, the charter determines what the member sees and can do.

### What the charter controls (from `icn-governance` crate)

```toml
# Example: contracts/templates/worker-coop.yaml
[membership]
admission = "application_plus_vote"
quorum = "two_thirds"
voting_method = "consent"

[treasury]
allocation_method = "governance_vote"
patronage_distribution = "ny_corp_law_93"
reserve_requirement = 0.10

[governance]
proposal_types = ["budget", "membership", "policy", "dissolution"]
decision_method = "consensus"
fallback = "simple_majority"
receipt_required = true
```

### What changes in the UI per charter

| UI element | Charter field | Example variation |
|-----------|--------------|-------------------|
| Available proposal types | `governance.proposal_types` | Worker coop shows budget+membership+policy+dissolution. Event planning coop might only show budget+policy |
| Voting interface | `membership.voting_method` | Consent: approve/object/stand-aside. Simple majority: for/against/abstain. Consensus: support/block |
| Threshold displays | `membership.quorum` + `governance.decision_method` | "Requires consensus (fallback: simple majority)" vs "Requires 2/3 approval" |
| Membership flow | `membership.admission` | Application+vote vs invitation vs open enrollment |
| Economic context | `treasury.*` | Patronage tracking visible for worker coops. Reserve requirements shown where configured |
| Receipt requirement | `governance.receipt_required` | If true, every allocation shows execution receipt status |

### Rendering strategy

The mobile app does NOT hardcode governance semantics. It reads charter configuration and adapts:

1. **Proposal creation** — only shows proposal types enabled by the charter
2. **Voting UI** — renders the correct voting method controls
3. **Threshold display** — computes and shows the rules from charter config
4. **Role labels** — uses charter-appropriate terminology
5. **Action availability** — only offers actions the member's role permits under this charter

### Charter fallback

**Charter templates are deployed as of Phase 1 complete (2026-03-18).** Five templates exist at `contracts/templates/` (worker-coop, consumer-coop, housing-coop, community-org, federation). The ratification flow (`ProposalPayload::Charter`, PRs #1336+#1337) is merged. If a cooperative has not yet ratified a charter, the app falls back to a default governance profile:
- Simple majority voting
- Standard proposal types (text, budget, membership, config_change)
- No patronage tracking
- No charter badge on scope switcher

---

## 8. Demo-Flow Mapping

The four October 2026 demo flows map to specific mobile screens and transitions.

**Dependency:** This mapping is designed against the gateway API endpoints (which exist with real handler logic — 70+ endpoints, zero stubs per the vertical slice assessment). However, the demo scripts currently run simulated responses, not real gateway calls. Phase 2 Track D converts them to real `icnctl`/gateway calls. The governance demo (Flow 1) was validated end-to-end via CLI on March 17 (19 steps, exit 0, cryptographic receipt generated), but this was the CLI path, not the gateway-mediated path the mobile app will consume. Mobile implementation should proceed against the gateway API spec with the understanding that end-to-end integration testing requires Track D completion.

**Gap Analysis Reality (from `docs/strategy/ICN-Gap-Analysis-March-2026.md`):**
- **Flow 1 (Governance):** PROVEN end-to-end. This is the v1 mobile anchor.
- **Flow 2 (Patronage):** Merged but unverified at runtime. PatronageTracker exists (PR #1325) but hasn't been demo-tested.
- **Flow 3 (Federation):** Basic federation works, but settlement finality is undefined. The demo runs four coops on the same K3s cluster — not real cross-network federation. Mobile should not claim federation is production-ready.
- **Flow 4 (Reporting):** Merged but unverified. Depends on receipt chain completion (#1311).

**Priority:** Build mobile v1 around Flow 1 (governance). Add other flows as they're verified by the Sprint (Mar 17-30).

### Flow 1: Governance ("Did the vote that authorized this action actually happen?")

**Demo script:** `demo/scripts/flow-1-governance.sh`
**Persona:** Harbor Homes Housing Cooperative

| Step | Gateway call | Mobile screen | User action |
|------|-------------|---------------|-------------|
| Create proposal | `POST /v1/gov/proposals` (budget: $12K spend) | Participate → New Proposal | Fill title, amount, recipient, purpose |
| Open for voting | `POST /v1/gov/proposals/{id}/open` | Proposal detail | Tap "Open for voting" (steward action) |
| Cast votes | `POST /v1/gov/proposals/{id}/vote` × N | Proposal detail → Vote sheet | Select for/against/abstain + optional comment |
| View tally | `GET /v1/gov/proposals/{id}/tally` | Proposal detail (updated) | See vote counts |
| Close voting | `POST /v1/gov/proposals/{id}/close` | Proposal detail | Tap "Close voting" (steward action) |
| View proof | `GET /v1/gov/proposals/{id}/proof` | Provenance panel | Tap "View proof" → see GovernanceProof with attestations |

**Key mobile moment:** The proof panel. Member taps "View proof" and sees every voter's Ed25519 attestation, the decision hash, and verification status. This is where ICN's value becomes tangible.

### Flow 2: Patronage ("Why did this member get this distribution?")

**Demo script:** `demo/scripts/flow-2-patronage.sh`
**Persona:** BrightWorks Worker Cooperative

| Step | Mobile screen | Shows |
|------|--------------|-------|
| View patronage allocation | Participate → Allocation detail | Amount, recipient, basis (NY Corp Law §93 via PatronageTracker) |
| Trace provenance | Provenance panel | Proposal → GovernanceProof → AllocationReceipt |
| Verify | Deep inspection | Full chain: governance decision → allocation → distribution formula |

**Key mobile moment:** Tapping "Why this amount?" on a patronage allocation and seeing the entire chain from governance vote to formula application.

### Flow 3: Federation ("How do independent co-ops coordinate without a bureaucracy?")

**Demo script:** `demo/scripts/flow-3-federation.sh`
**Persona:** Finger Lakes CDN (federation of Harbor Homes, BrightWorks, River City Tool Library)

| Step | Mobile screen | Shows |
|------|--------------|-------|
| View federation members | Groups → Federation entity | Member cooperatives, attestations, vouches |
| Federation proposal | Participate (federation scope) | Cross-entity proposal (join/leave/vouch/policy) |
| Vote as delegate | Proposal detail → Vote | Delegate action on behalf of member cooperative |
| Cross-entity attestation | Activity | Federation attestation event with proof |

**Key mobile moment:** Scope switching from "Harbor Homes (Member)" to "Finger Lakes CDN (Delegate)" and seeing the governance context change entirely — different proposal types, different threshold rules, different scope of authority.

### Flow 4: Reporting ("Can this produce trustworthy reporting without admin overhead?")

**Demo script:** `demo/scripts/flow-4-reporting.sh`

| Step | Mobile screen | Shows |
|------|--------------|-------|
| View activity stream | Activity (scope: cooperative) | All proof-backed events in chronological order |
| Filter by type | Activity filter | Proposals only, settlements only, etc |
| Drill into record | Activity → Detail | Full provenance chain for any event |
| Cross-scope view | Activity (scope: all) | Events across all memberships |

**Key mobile moment:** The Activity stream itself — showing that every record is proof-backed, every flow is traceable, and the audit trail is self-generating without manual overhead.

---

## 9. Identity and Device Reality Spec

### What v1 honestly supports

| Feature | State | UX treatment |
|---------|-------|-------------|
| Local key generation | ✅ Working | `icnctl id create` → Ed25519 keypair + DID |
| Key storage | ✅ Working | age encryption in `.ICN_DATA_DIR/identity/keypair.age` |
| Challenge-response auth | ✅ Working | Sign nonce → JWT token (1hr expiry) |
| DID resolution | ✅ Working | `GET /v1/identity/resolve/{did}` |
| Export/import | ✅ Working (awkward) | `icnctl id export/import` — manual file transfer |
| Multi-device | ⚠️ Partial | `icnctl device [add|list|remove]` exists but delegation protocol not implemented |
| Social recovery | ⚠️ Designed | SDIS recovery flow exists in pilot-ui but not fully integrated |
| Key rotation | ⚠️ Partial | Rotation protocol v1 exists; v2 delegated keys planned |
| Biometric unlock | ❌ Not built | Must be added for mobile (Android Keystore / iOS Secure Enclave) |

### Honest mobile UX for identity

**Onboarding:** Show the reality. "Your identity is stored on this device. If you lose this device, you will need your recovery phrase or recovery contacts to regain access."

**Device management:** Show current device, with clear indication that multi-device is limited. "To use ICN on another device, you'll need to export your identity from this device."

**Recovery:** Surface recovery setup prominently. Do not bury it. "Set up recovery now" should be a persistent prompt until completed.

**Key operations:** All signing happens locally via biometric unlock (when built) or device authentication. Never transmit private keys.

### What the member sees in Profile → Identity

```
Identity
  DID: did:icn:z6MkhaXg...  [copy]
  Created: 2026-03-15
  Key type: Ed25519

Device
  This device: Pixel 9 Pro (primary)
  Status: Active
  Last synced: 2 minutes ago

Recovery
  Status: ⚠️ Not configured
  [Set up recovery]

Export
  [Export identity to another device]
```

---

## 10. QR Schema v1

### Practical constraints

- Maximum payload: 2000 bytes (for reliable scanning across devices, per `icnctl` implementation using `qrcode` crate)
- Encoding: JSON, optionally signed
- Must work in low-light, at distance, on cheap screens

### QR payload format

```json
{
  "v": 1,
  "type": "entity" | "proposal" | "obligation" | "identity" | "action",
  "id": "<entity:icn:... or proposal ID or DID>",
  "scope": "<optional entity context>",
  "action": "<optional action hint: vote, confirm, inspect>",
  "exp": "<optional expiry timestamp for dynamic codes>",
  "sig": "<optional Ed25519 signature over payload minus sig field>"
}
```

### Use cases (v1 only — no NFC, no AR)

| Use case | QR type | Payload | Mobile behavior |
|----------|---------|---------|----------------|
| Identify an entity | `entity` | Entity ID | Opens Groups → entity detail |
| Open a proposal | `proposal` | Proposal ID + scope | Opens Participate → proposal detail |
| Confirm an obligation | `obligation` | Obligation ID + action hint | Opens Participate → obligation with confirm action |
| Identity exchange | `identity` | DID + optional scope | Opens Profile → trust panel for that identity |
| Summit workshop QR | `action` | Scope + action hint | Opens relevant flow (demo-specific) |

### Scan flow

1. User opens scan mode (persistent camera, auto-detect, no "press to scan" button)
2. QR detected → instant transition to relevant view
3. If signed payload: verify signature before acting
4. If expired: show "This code has expired" with option to proceed to the referenced object anyway
5. If scope mismatch: prompt scope switch before opening

### Dynamic QR (for person-to-person interactions)

For actions requiring counterparty confirmation (e.g., exchange settlement):

1. Person A generates dynamic QR (rotating, short-lived, signed)
2. Person B scans
3. Both devices now share context via the signed payload
4. Action confirmation screen appears on both devices
5. Both confirm → operations queued and synced

### Non-QR fallbacks (for devices without cameras or in low-light)

- **Manual entry:** User can type an entity ID or DID directly
- **Nearby devices (future):** Bluetooth proximity discovery
- **Share link:** Deep links that open the same views as QR scans

---

## Appendix A: Existing Surfaces and Their Relationship to Mobile

The ecosystem map (icn-ecosystem-map.md) defines five front-end surfaces plus supporting tools. The mobile member app is a sixth surface that must converge with, not duplicate, these.

| Surface | Audience | State | Relationship to mobile |
|---------|---------|-------|----------------------|
| **pilot-ui** (web/pilot-ui/) | Members | Feature-complete PWA, 288KB. SDIS enrollment/proofs/recovery, steward dashboard, offline support, i18n (EN+ES) | Identity/wallet flows should inform mobile design. **Critical caveat:** pilot-ui is NOT yet wired to real `icnd` — the `anchor-manager.js` component is a stub. Phase 2 Track D wires it up. Pilot-ui flows are validated against local/mocked backends, not the live gateway. Port the UX patterns, but do not assume the API integration is proven. |
| **React Native SDK** (sdk/react-native/) | Members | Components: VotingScreen, BudgetManager, CooperativeManager, NotificationCenter, RecurringPaymentSetup | Starting point for mobile. Extend with scope switching, action queue, provenance views. |
| **TypeScript SDK** (sdk/typescript/) | Developers | `@icn/client` v0.1.0, OpenAPI-generated types | Gateway API client for mobile use. Shared with Demo UI — both consume the same projection model. |
| **Demo UI** (not yet built) | Summit attendees | Single-page React + TS SDK app. Presentation instrument, NOT a product. Four keyboard-navigable scenes. | Shares the same gateway API and TypeScript SDK as mobile. The Demo UI and mobile app must use a shared projection model for proposals, proofs, and governance flows to avoid incompatible front-end representations at the Summit. Demo UI is built post Phase 2 Track D. |
| **NYCN Platform** (ny-coop-net) | Organizers | FastAPI + React/TS, 85% prod-ready. Missing: ICN receipt views, `icn_service.py`. | Not directly related to mobile member app, but shares the pattern of anchoring decisions to ICN receipts. Receipt rendering should be consistent across NYCN and mobile. |
| **TUI** (icn-console) | Operators/devs | ratatui + crossterm. Dashboard, Members, Ledger, Governance, Trust tabs | Remains the serious operator surface. Mobile does not replace it. |
| **Demo scripts** (demo/scripts/) | Presenters | 4 flows, presenter mode, beat markers | Mobile v1 should support these flows for October demo. **Note:** demo scripts currently run simulated responses. Phase 2 Track D converts them to real `icnctl` calls. The governance demo (Flow 1) was tested end-to-end via CLI on Mar 17 (19 steps, exit 0, cryptographic receipt verified) — but this was the CLI flow, not the gateway-mediated flow the mobile app would consume. |
| **ICN Website** (icn-website) | Developers + Organizers | Astro, deployed to intercooperative.network. Live but stale stats. | Summit attendees will scan QR codes pointing here. QR schema (Section 10) should support website deep links. |

## Appendix B: Gateway Endpoints Relevant to Mobile v1

### Auth (required for all protected calls)

```
POST /v1/auth/challenge  →  { nonce, expires_in }
POST /v1/auth/verify     →  { token (JWT), expires_in }
```

### Core governance (the demo-critical path)

```
POST   /v1/gov/domains                        Create governance domain
GET    /v1/gov/domains                         List domains
POST   /v1/gov/proposals                       Create proposal
GET    /v1/gov/proposals                       List proposals (paginated, filterable)
GET    /v1/gov/proposals/{id}                  Get proposal detail
POST   /v1/gov/proposals/{id}/open             Open for voting
POST   /v1/gov/proposals/{id}/vote             Cast vote
POST   /v1/gov/proposals/{id}/close            Close and tally
GET    /v1/gov/proposals/{id}/tally            Vote counts
GET    /v1/gov/proposals/{id}/proof            Cryptographic proof
GET    /v1/gov/proposals/{id}/discussion       Discussion thread
POST   /v1/gov/proposals/{id}/discussion/comments  Add comment
POST   /v1/gov/delegations                     Delegate vote
```

### Cooperative management

```
POST   /v1/coops                    Create cooperative
GET    /v1/coops/{id}               Get cooperative
POST   /v1/coops/{id}/members       Add member
DELETE /v1/coops/{id}/members       Remove member
PUT    /v1/coops/{id}/members/role  Update role
```

### Federation

```
GET    /v1/federation/status                    Federation status
POST   /v1/federation/init                      Initialize federation
GET    /v1/federation/coops                     List federated coops
POST   /v1/federation/coops/{id}/vouch          Vouch for cooperative
POST   /v1/federation/attestations              Issue attestation
```

### Identity

```
GET    /v1/identity/resolve/{did}    DID resolution
GET    /v1/members/{coop_id}/{did}   Member profile
```

### WebSocket (real-time events)

Gateway supports WebSocket event streaming for live activity updates.

## Appendix C: Sync State Model

Every operation on mobile moves through:

```
local_pending → syncing → network_confirmed → (optional: disputed | superseded)
```

### Member-facing state labels

| Internal state | Member sees | Color |
|---------------|-------------|-------|
| `local_pending` | "Saved on your device" | Gray |
| `syncing` | "Sending to network..." | Blue/animated |
| `network_confirmed` | "Confirmed by network" | Green |
| `disputed` | "Flagged for review" | Red |
| `superseded` | "Updated — review required" | Orange |

### Offline behavior

| Action | Offline? | Behavior |
|--------|---------|----------|
| View cached proposals | ✅ | From local cache |
| Cast vote | ✅ | Signed locally, queued for sync |
| Create proposal | ✅ | Stored locally, synced when online |
| View proofs | ⚠️ | Only if previously cached |
| Scope switching | ✅ | From cached membership data |
| View real-time activity | ❌ | Requires WebSocket connection |

### Conflict handling

If state changes before sync (e.g., proposal amended while offline vote is pending):

1. UI shows: "This proposal changed before your vote was confirmed"
2. Member must re-review the updated proposal
3. Member re-submits vote against current version
4. Original local vote is discarded with explanation

---

*Spec generated 2026-03-18. Updated 2026-03-18: charter templates shipped (Phase 1 complete, PRs #1336+#1337). Next update triggers: gateway scope expansion (treasury:read/write), mobile implementation reveals missing assumptions.*
