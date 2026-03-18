# Agent Teams Launch Prompt: ICN Point A → Point B

> Paste this as first message in a new Claude Code chat with Agent Teams enabled.

---

We are using Claude Code Agent Teams. Create a team of 6 agents and run a coordinated "Point A → Point B" engineering campaign for the ICN repo.

## Repo Context

**Repo**: `~/projects/icn`
**Branch**: `main` (clean)
**Workspace**: Cargo workspace in `icn/` subdirectory. 31 library crates + 3 binaries + 2 app crates. 414K LOC Rust. ~5,060 tests. Builds clean on rustc 1.88.0.

## What ICN Is

ICN is a civilizational consensus technology. Not a blockchain. Not a federation server. It's a constraint enforcement substrate where:

- **Apps** (Policy Oracles) translate domain semantics (trust scores, governance rules, membership criteria) into generic constraints
- **The kernel** enforces constraints blindly without understanding meaning (the **Meaning Firewall**)
- **CCL** (Cooperative Contract Language) is the constitutional layer where cooperatives, communities, and federations write enforceable policies and inter-org contracts
- **Cooperatives, communities, and federations** are the primary organizational units — each with their own policies, governance, economics, and dispute resolution
- **Federations** can be any shape/size — overlapping, temporary, regional, sectoral — and they operate via cooperative contracts (treaties), not centralized authority
- **Individuals** participate through wallet/mobile as their primary interface, with rights that persist beyond any single membership
- **Developers/operators** run services on nodes for their orgs or as commons/public services

The purpose is to defang state and capital by making legitimacy computable, coordination auditable, and participation non-capturable.

## Current State (Point A) — VERIFIED FROM CODE

### What's Built and Real (90%+ substrate)

**Identity** (icn-identity, 15K LOC): CommonsHolderRecord with affiliations/rights/personhood levels. PersonhoodAnchor with POPLevel (Weak/Strong/Verified). MembershipCapability (Vote, Propose, Transact, HoldOffice, AccessPrivate, Sponsor). JurisdictionId format `<type>:<id>`. VUI (Verifiable Unique Identifier) with threshold PRF. Age-encrypted keystore. Social recovery primitives.

**Trust** (icn-trust, 13K LOC): Three orthogonal dimensions (Social, Economic, Technical). TrustScore [0,1] with class thresholds (Isolated/Known/Partner/Federated). ScopeId for scope-bounded trust. Multi-graph management. Sybil detection. Evidence validation. Trust-gated rate limiting per trust class.

**Scope/Jurisdiction** (icn-kernel-api): ScopeLevel enum: Local(0) < Cell(1) < Org(2) < Federation(3) < Commons(4). CellId with deterministic hashing. 100% complete, fully in kernel.

**Entity Model** (icn-entity + icn-coop + icn-community): EntityId with type prefixes. 7 CoopTypes (Worker, Consumer, Producer, MultiStakeholder, Platform, Housing, Credit). 4 CommunityTypes. Full lifecycle (Forming→Active→Suspended→Dissolving→Dissolved→Merged→Split). Recursive membership (Individual OR Cooperative as member).

**Governance** (icn-governance, 25K LOC): 13+ ProposalPayload variants (Text, Budget, Membership, ConfigChange, Treasury, SurplusAllocation, Federation, ResourceAccess...). Weighted voting with delegation chains. Amendments with ratification. Full appeal system (7 AppealGrounds, 6 AppealRemedy types). GovernanceDecisionReceipt V1+V2 with canonical hashing. Treasury operations (CreateBudget, Withdraw, Spend, TransferBetweenBudgets).

**CCL** (icn-ccl, 16K LOC): Full AST (Contract, Rule, Stmt, Expr, Value). Capability system (ReadLedger, WriteLedger, ReadTrust, ExecuteCompute, MintToken). Fuel metering, deterministic. ContractRegistry with deploy/revoke/deprecate, semantic versioning, visibility controls (Private/Coop/Public), gossip-based distribution. Entity/governance schema (Community, Cooperative, Federation types with governance bodies, economics, agreements). DisputeResolutionSystem with re-execution, mediator pool, penalties, gossip callbacks.

**Receipts** (icn-kernel-api): CanonicalReceipt trait with blake3 canonical hashing. Chain: GovernanceDecisionReceipt → AllocationReceipt → SettlementIntent(s) → LedgerEntry. ProvenanceAnchors linking receipts. ReceiptStore with sled persistence. 6 REST endpoints for receipt and ledger provenance queries.

**Disputes**: CCL disputes (dispute.rs): Dispute types (IncorrectResult, FuelLimitExceeded, etc.), DisputeEvidence, DisputeResolutionSystem with auto re-execution + mediation + penalties. Governance appeals (appeal.rs): AppealType (Revocation, Suspension, GovernanceDecision, PolicyViolation), AppealGrounds (7 variants), AppealRemedy (Reverse, Reinstate, Modify, Compensation), full Filed→UnderReview→Hearing→Resolved workflow.

**Federation** (icn-federation, 16K LOC): 6 AgreementTypes (Trade, Credit, ResourceSharing, FederationMembership, LaborReciprocity, SharedServices). Agreement lifecycle (Draft→Proposed→Signed→Active→Expired/Terminated/Disputed). Amendment system. Bilateral clearing agreements with settlement intervals. Netting engine for circular obligation detection. Receipt clearing for cross-scope settlement. Federated DID resolution with caching. Federated trust attestations.

**Ledger** (icn-ledger, 29K LOC): Double-entry mutual credit. Merkle-DAG integrity. Fork detection.

**Compute** (icn-compute, 26K LOC): Trust-gated execution. WASM registry. Commons pool with sybil resistance. Scheduler.

**Network** (icn-net 22K + icn-gossip 16K): QUIC/TLS with DID-TLS binding. Signed envelopes + replay guard. Topic-based gossip with vector clocks + Bloom filters + anti-entropy. mDNS discovery.

**Gateway** (icn-gateway, 74K LOC): 38 route modules. DID challenge-response auth → JWT. QR login sessions. Trust-gated rate limiting. WebSocket with 20+ event types. Middleware: metrics, CORS, security headers, tracing, compression.

**Service Discovery**: ScopedDiscovery/NamingService/Discovery traits. ServiceType {namespace, name}, ServiceEndpoint, ServiceAnnouncement. Gossip-based with TTL. No DNS integration.

**SDIS/Steward** (icn-steward, 7K LOC): VuiRegistry with Bloom filters. StewardProfile with bonds/jurisdiction/stats. Enrollment + recovery ceremonies. Gossip-based steward network.

### What Exists at the Human Interface

**pilot-ui**: Vanilla JS PWA. Screens: Login (CLI token), QR login, Dashboard, Members, Ledger/Transactions, Governance (proposals+voting+discussion), Treasury (partial), Listings, Settings, Notifications. 11 languages. Offline-capable. **Gap**: Flat single-coop view. No scope awareness. No receipt provenance. No multi-org. No "what law applies here?"

**icnctl**: 30+ command groups. Power-user tool. No regular-member path.

**TypeScript SDK**: Published. Full API coverage. No built-in key management (requires external SignatureProvider).

**React Native CoopWallet**: 20+ screens (Login, Home, Transactions, Payment, Governance, Identity, Contacts, Steward Dashboard, Verify, Settings). Ed25519 + post-quantum crypto. QR scanning. Offline queue. **Gap**: Not deployed. Not connected to scope/affiliation model.

**OpenAPI**: Current, generated from utoipa. 38 modules documented.

### Open Issues (58)
- **#1147** EPIC: Vertical Slice (Identity→Standing→Governance→Allocations→Workloads→Receipts→Audit→Future Constraints). Tracks A-E with dependency graph.
- **#1012** Legibility Dashboards UX Spec: "Constraints with Receipts" pattern — specifies Credit/Trust/Governance dashboards where users see What (constraint value), Why (policy), Who (authority), When (timestamp).
- ~12-15 issues should be closed as done/superseded by merged PRs.
- Core issues to keep: kernel/app separation (A1-A4), treasury/economics (B1-B3), operations (C1-C3), compute (E4, E7), naming primitive (#862), federation agreements (#863), attestation model (#1009), commons compute (#925, #947, #948).

## Vision (Point B)

ICN is "online" when:

1. A **person** can create identity, prove personhood, and hold inalienable commons rights that persist beyond any org membership
2. A **cooperative** can form, write governance policies in CCL, manage membership, operate a treasury, and run services — all under its own enforceable rules
3. A **community** can govern shared resources, resolve disputes, and coordinate members without external authority
4. A **federation** can form via cooperative contracts (CCL treaties), clear credit across member orgs, resolve cross-org disputes, and govern shared resources — with the federation itself governed by its member orgs
5. All of the above operates through **scope-aware interfaces** where users always know: "Where am I?" (jurisdiction), "What law applies?" (policies/contracts), "What can I do?" (capabilities), "What happened and why?" (receipt chain)
6. **Wallet/mobile** is the citizen interface for identity, rights, affiliations, signing, and daily participation — with offline capability
7. Every significant action produces **inspectable receipts** (decision→effect→settlement→audit)
8. **Disputes** have a procedural pathway (evidence→mediation→arbitration→appeal) with enforced outcomes and receipt provenance
9. **Services** are how orgs "exist" — discoverable, named, capability-gated, scoped
10. The system **resists capture** by state/capital through: plural jurisdictions, bounded power, anti-sybil, transparent finance, hard exits (forking is procedural, not war)

## Non-Negotiables

1. **Meaning Firewall**: kernel stays domain-agnostic. Policy semantics stay in apps/oracles. Never import domain crates into kernel crates.
2. **Sequential phases only**: do not invent new tracking systems. Use the existing phase numbering.
3. **Federations are contracts**: can overlap, vary in shape/size. Not empires.
4. **Rights are durable**: cooperatives can revoke membership; they cannot revoke personhood/commons rights.
5. **Receipts for everything**: if it's a decision, allocation, settlement, or enforcement — it produces an inspectable receipt.
6. **CCL is living law**: orgs author policies that compile to enforceable constraints. Humans must be able to read what governs them.
7. **Wallet-first UX** for regular humans; CLI remains for power users.
8. **Node modes are explicit**: wallet/individual, org-operator, dev/service — each with clear "must do / may do / must never be required to do."

## Team Roles

1. **CONSTITUTION / RIGHTS**: Map current rights/jurisdiction/scope model (CommonsHolderRecord, Affiliations, ScopeLevel, MembershipCapability). Identify what's missing for a real constitutional layer: legible rights rendering, jurisdiction boundaries in UX, "personhood vs membership" clarity, commons baseline guarantees.

2. **GOVERNANCE / RECEIPTS**: Map the proposal→decision→effect→receipt→audit chain. Current state: GovernanceDecisionReceipt→AllocationReceipt→SettlementIntent→LedgerEntry. Identify gaps: treasury spend execution, receipt rendering for humans, "Constraints with Receipts" pattern (#1012), amendment/appeal UX.

3. **ECONOMICS / TREASURY**: Map mutual credit + treasury + asset types + allocation/settlement. Current state: icn-ledger double-entry, SettlementIntent with 6 AssetTypes, federation clearing + netting engine. Identify gaps: treasury-coop integration (#1139), surplus distribution, cross-org credit clearing UX, "why did this fail?" explainer.

4. **CCL + DISPUTES**: Map CCL as living constitutional layer. Current state: ContractRegistry with deploy/revoke/deprecate, DisputeResolutionSystem with mediation/penalties, governance Appeals. Identify gaps: policy registry browsing, human-readable policy rendering, dispute filing UX, evidence chain-of-custody, cross-org dispute escalation, federation agreement authoring.

5. **HUMAN INTERFACE**: Define citizen/member/steward/operator journeys across scope. Map existing surfaces (pilot-ui, CoopWallet, icnctl) to what's needed. Design scope navigation, affiliations home screen, receipts provenance view, "what law applies here?" surface. Account for node modes: wallet (signing+affiliations), org-operator (services+monitoring), dev (build+deploy under policy).

6. **SECURITY / OPS / RESILIENCE**: Threat model (capture, sybil, coercion, censorship, dependency attacks). NAT traversal plan. Naming resilience (no single DNS choke). Key management + recovery. Operational playbooks. Packaging/distribution for nodes + wallet. Observability mapping to scope.

## Mission

Produce a single "State → Vision → Gap → Phase Plan" document that engineers ICN from Point A to Point B:

### Required Outputs

**A) Repo State Summary** (per subsystem, cite file paths and types)

**B) Vision Requirements** (operational, not aspirational — what users can DO)

**C) Gap Map** (per layer: Constitution, Governance, Economics, Disputes, Interface, Ops)

**D) Phased Engineering Roadmap** (sequential, concrete deliverables):

- **Phase 0**: Legibility + receipts surface + non-dev onboarding. Web-first (pilot-ui evolution). Auth without CLI JWT. Scope context even if single-org. Receipt chain viewer. LAN-accessible.
- **Phase 1**: Scope-aware governance + treasury + membership end-to-end. Decision→treasury spend→receipt→audit loop complete and visible. Multi-org affiliations experience begins.
- **Phase 2**: CCL as living law — policy registry browsing, human-readable rendering, "what governs this?" surfaces. Dispute filing MVP (claims, evidence, mediation, appeals) integrated with receipts.
- **Phase 3**: Federation as contract — agreement authoring in CCL, cross-org credit clearing UX, federation-scoped governance. Naming/discovery primitive for human navigation.
- **Phase 4**: Wallet evolution — signer → lightweight node → optional commons participation. Mobile-first identity + offline queue + affiliations.
- **Phase 5**: Resilience hardening — NAT traversal, naming without DNS dependence, anti-censorship, packaging/distribution.

**E) Phase 0 Execution Plan**: 15-25 concrete tasks mapped to files/issues, with validation steps. Every task must be demonstrable on LAN from a workstation.

**F) Issue Triage Recommendations**: Which of the 58 open issues to keep, close (with rationale + superseding PR), or repurpose. Do NOT execute closes — just provide the list.

### Constraints

- Use repo as ground truth. Cite file paths, crate names, existing issues/PRs.
- Be concise and technical. Tie deliverables to ICN primitives (scope, capabilities, PolicyOracle, receipts, CCL, service discovery, federation agreements, dispute workflows).
- No vague "build a dashboard" language. If you can't point to the policy/model that governs a screen, the screen is not ICN.
- Every phase must end in a demonstrable user story with receipts.
- Do NOT invent new tracking systems. Work within existing phase numbering.

### Output Location

Write all findings to: `docs/plans/2026-02-14-icn-consensus-stack-a-to-b.md`
