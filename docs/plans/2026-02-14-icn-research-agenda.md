# ICN Formal Research Agenda: Architectural Hurdles and Track Briefs

**Date**: 2026-02-14
**Companion to**: `docs/plans/2026-02-14-icn-consensus-stack-a-to-b.md`
**Scope**: 13 research tracks covering kernel through federation, with formalization targets, prototype targets, failure modes, and pseudocode for core algorithms.

---

# ICN Architectural Hurdles and Formal Research Agenda

## Framing and evidence basis

ICN is a coordination substrate for cooperatives (economic engine), communities (civic engine), and their federations (scale bridge) that enforces generic constraints without understanding domain meaning. This "Meaning Firewall" is the core architectural commitment: policy oracles/apps compile human meaning (law, governance, entitlements, budget policy, etc.) into constraints; the kernel validates and executes constraints *blindly* with audit-grade receipts.

The project's current public artifacts already encode many of the needed primitives and "hard edges":
- Kernel ↔ app boundary and invariants (the separation itself is treated as a design object).
- Multi-device identity and social recovery designs, plus SDIS threat modeling and TOFU posture.
- Multi-scope ("cells and scopes") model and scope types in the kernel API.
- Governance primitives as CCL-invokable capabilities with explicit state machines (and explicit open questions).
- Economic safety rails for mutual credit (dynamic credit limits, new member throttling, dispute lifecycles) treated as deployable, testable defenses.
- Compute substrate "vertical slice" status and receipt stage work items.
- Pilot UI operator/treasurer guides that expose present-day onboarding and governance/economics workflows to non-technical users.
- The threat model assumes a hostile environment and includes concrete gaps and mitigations (especially in identity, networking, and trust).

External standards relevant to ICN's hostile-network and self-sovereign-identity posture include:
- DID core spec from W3C for identifier document semantics and resolution model.
- Ed25519/EdDSA as specified in RFC 8032 (for signatures and canonical signing procedures).
- IETF RFC 9000 (QUIC transport) for secure, migration-capable transport in adversarial networks.
- NAT traversal tools STUN (RFC 8489) and TURN (RFC 8656) for real-world connectivity behind consumer routers/firewalls.
- Append-only "accountability log" idea exemplified by Certificate Transparency (RFC 6962), useful as a design analogue for "government with receipts."

## Research track map

The research program decomposes ICN's architectural hurdles into thirteen tracks (10–15 requested), organized around the coop/community/federation stack and the Meaning Firewall boundary:

- Kernel constraint model and Meaning Firewall boundary
- Identity and personhood under hostile conditions
- Scope and jurisdiction formal model
- Authorization, delegations, and effective rights computation
- Governance mechanisms and receipts lifecycle
- Economics: mutual credit, treasuries, settlement, and safety rails
- CCL: contract semantics, compilation, determinism, and capability safety
- Disputes, evidence, and appeals across scopes
- Naming, discovery, and navigable provenance without DNS
- Networking, NAT traversal, and censorship resistance posture
- Receipts datastore: provenance graph, sharding, retention, query UX
- Compute fabric: placement, verification, disputes, and metered execution
- Federation theory: inter-scope treaties, cross-scope settlement, and escalation

## Track briefs with questions, formal targets, prototypes, failure modes, dependencies

### Kernel constraint model and Meaning Firewall boundary

**Core research questions**
- What is the *minimal* kernel action interface required to enforce constraints blindly while remaining sufficient for governance/economics/compute? (Define `ActionIntent`, `ConstraintSet`, `PreState`, `Effect`, `Receipt`.)
- How are constraints represented so that they are: (a) auditable, (b) deterministic, (c) verifiable by third parties, and (d) composable across scopes?
- What are the kernel-level invariants that must never be violated (e.g., receipt immutability, scope isolation, capability non-forgeability), and what is the smallest "trusted computing base" that enforces them?
- How does the kernel *refuse* semantic capture? Concretely: how do you prevent oracles from smuggling semantics into kernel paths (e.g., bespoke "special cases")?
- What is the kernel-level upgrade story: what changes require coordinated upgrades, what can be versioned at oracle level, and how are receipts kept backward-verifiable?

**Formalization targets**
- **Kernel action algebra**: a typed, spec-level definition of `Action` and `Constraint` evaluation order (including failure semantics).
- **Kernel/app separation spec**: normative list of kernel primitives + non-goals + forbidden APIs ("semantic leakage" list).
- **Determinism proof obligations**: for any oracle-compiled constraint, define what makes it deterministic (inputs, allowed syscalls, time sources, randomness constraints).

**Prototype targets**
- Minimal "constraint VM" that evaluates a tiny constraint language (boolean formulas and bounded comparisons) and emits canonical receipts for accept/reject decisions.
- A "policy oracle sandbox" harness: compile a simple governance rule → generic constraints → kernel enforces without understanding semantics; test that swapping oracle doesn't change kernel logic.

**Failure modes and adversarial considerations**
- Semantic capture via "convenience exceptions" in kernel.
- Oracle ambiguity causing multiple honest nodes to compile different constraints.
- Constraint DoS (pathological constraints that are expensive to verify).
- Kernel upgrade forks where old receipts become unverifiable or ambiguous.

**Dependencies**
- Scope and jurisdiction formal model (scoping is a kernel boundary)
- Receipts datastore and canonicalization
- Authorization/effective rights

---

### Identity and personhood under hostile conditions

**Core research questions**
- What constitutes "personhood" for ICN's threat model: unique human, unique coop, unique node operator, or all of these? How do requirements differ across coops vs civic communities?
- How do multi-device identities bind devices to a root identity, and what is the minimum proof chain to prevent rogue-device enrollment?
- Social recovery: what must be true for M-of-N trustee recovery to be robust against collusion, coercion, and trustee Sybils?
- How are DID documents and recovery events broadcast and validated in hostile networking conditions (partition, eclipse, replay)?
- Selective disclosure: when a community must prove entitlement (housing, benefits, voting eligibility), what is the minimal disclosure model consistent with explainability and receipts? (DID-based claims vs credential proofs.)

**Formalization targets**
- DID document / key rotation / recovery event state machines (including revocation and mapping old→new DID after recovery).
- Signature and canonicalization rules for identity operations (Ed25519/EdDSA verification).
- Threat-model-backed requirements for enrollment challenges, replay windows, and TOFU pinning rules.

**Prototype targets**
- End-to-end SDIS simulation: enroll second device → revoke device → attempt forged enrollment → verify refusal.
- Social recovery tabletop + implementation test: lose all devices → collect attestations → delay window cancellation case → finalize mapping and verify ledger continuity.

**Failure modes and adversarial considerations**
- "Recovery capture" (trustees collude; user's identity transferred).
- Long-range replay of old attestations; mitigation via time bounds and challenge freshness.
- Admission Sybils (fake trustees / fake members) undermining community governance and economic credit.
- Censorship via blocking enrollment or recovery messages; requires transport redundancy and gossip strategy.

**Dependencies**
- Networking/NAT/censorship posture
- Authorization and membership model
- Receipts and provenance (identity operations must yield receipts)

---

### Scope and jurisdiction formal model

**Core research questions**
- What is the minimal formal definition of "scope" in ICN: a DID, a cell identifier, a tuple (scope-id, scope-level, parent linkage), or something else?
- How do scope boundaries reflect the societal model: individual ↔ coop ↔ community ↔ federation ↔ commons, and how do rights vary per scope?
- What does it mean for an action to occur "within a scope" if it touches multiple scopes (e.g., cross-coop settlement, inter-community service entitlement)?
- How are cross-scope references resolved deterministically under partitions (e.g., "current charter version of scope X")?
- How do scopes compose: can a community scope delegate some domains to a coop scope without mixing jurisdiction?

**Formalization targets**
- Scope identity and parent/child resolution algorithm (including "effective scope" when actions target nested scopes).
- Cross-scope reference semantics ("treaties" and "delegations" as first-class objects; see federation track).
- A formal consistency model: per-scope append-only decision log vs causal DAG vs quorum-based consensus *per scope* (explicitly rejecting global consensus).

**Prototype targets**
- "Scope explorer" UI: show a scope graph, memberships, and rights computation for a user across scopes (human-legible).
- Minimal multi-scope event issuance: same identity acts in two scopes; verify receipts correctly annotate scope and can be queried by scope.

**Failure modes and adversarial considerations**
- Jurisdiction confusion attacks (malicious actors route actions through "wrong" scope to gain rights).
- Scope squatting (creating similar-looking scopes to confuse users).
- Cross-scope downgrade: federation forces local scopes to adopt rules via protocol pressure rather than agreements.

**Dependencies**
- Identity/personhood
- Kernel constraint model
- Naming/discovery and receipt graph query (for legible scope navigation)

---

### Authorization, delegations, and effective rights computation

**Core research questions**
- What is the canonical representation of capabilities: role-based? capability tokens? CCL capabilities? hybrid?
- How is "effective rights" computed across memberships, roles, delegations, and trust thresholds in a way that is deterministic and auditable?
- How do you represent revocation (membership removed; delegation cancelled; trustee revoked) so that old receipts remain verifiable but future actions are blocked?
- How do you prevent "delegation loops" and "role escalation" without introducing centralized adjudication?
- How do authorization checks remain explainable: can the UI always answer "why can/can't I do this?" with a navigable proof trail?

**Formalization targets**
- Effective rights algorithm spec (including cycle detection, scope inheritance rules, and time/version scoping).
- Delegation object schema and validation rules.
- Capability safety contract: which capability implies which kernel actions are allowed, and what constraints must accompany them.

**Prototype targets**
- A reference `explain_rights(actor, scope, action)` function that returns a proof graph: memberships → delegations → capability → constraints.
- "Role escalation red team": attempt to craft delegations that bypass rules; confirm kernel refuses.

**Failure modes and adversarial considerations**
- Delegation laundering: federation delegates a right that local scope did not intend.
- Coercion-based delegation ("sign this or lose membership").
- Stale authorization caches in partitioned networks causing temporary escalation.

**Dependencies**
- Scope/jurisdiction model
- Identity and recovery mappings
- Kernel constraint model and receipt format

---

### Governance mechanisms and receipts lifecycle

**Core research questions**
- What is the canonical governance lifecycle state machine that all governance models can map onto (proposal → deliberation → decision → execution → archival), while still allowing diverse patterns (consensus, sociocracy, councils)?
- Where is the "Meaning Firewall" boundary: does governance output produce constraints directly, or does it produce CCL code that later produces constraints?
- How are votes represented: public (auditable) vs private (coercion-resistant)? What's the local-scope tradeoff? (This is explicitly called out as an open question in governance primitives.)
- How do proposal outcomes translate into *machine-legible outputs* that cause effects (treasury transfers, membership changes) with receipts?
- What are minimal dispute hooks: how can governance decisions themselves be appealed, and how does the system represent that appeal path?

**Formalization targets**
- Governance receipt schema: proposal ID, scope, eligibility set, tally evidence, rule set identifier, and "execution plan."
- Deterministic tallying requirements (inputs, deadlines, time sources).
- "Decision registry" spec: canonical mapping from governance decisions → executable actions (for "government with receipts").

**Prototype targets**
- Implement the governance templates described in governance primitives (e.g., consensus-with-fallback majority, emergency lock) and demonstrate: proposal → pass → automatic effect execution.
- Governance UI that exposes receipts: show "intent → constraints → decision → effect → receipt" for a single budget vote.

**Failure modes and adversarial considerations**
- Governance capture via spam proposals and shallow participation.
- "Receipt forgery" (fake tally proofs) if eligibility sets are ambiguous.
- Time attacks: inconsistent clocks across nodes change deadline evaluation.
- Social engineering: malicious proposal payloads with misleading human text but benign machine constraints (or vice versa).

**Dependencies**
- Authorization/effective rights
- Receipts format and datastore query
- CCL semantics (if governance compiles to executable contracts)

---

### Economics: mutual credit, treasuries, settlement, and safety rails

**Core research questions**
- Mutual credit invariants: what kernel-level constraints maintain ledger integrity (double-entry, sum-to-zero, non-negative treasury rules) in a partitioned network?
- Economic safety rails: how do dynamic credit limits and new member throttling interact with governance changes and trust graph updates (to avoid sudden "limit jumps" that enable grab-and-run)?
- Treasury execution: how are allocations authorized (governance) and executed (ledger) with receipts that treasurers and members can audit?
- Settlement across scopes: what is the minimal inter-scope settlement primitive that allows coop↔community↔federation exchanges without global consensus? (Receipt clearing, routing, dispute use.)
- Economic censorship resistance: how does a community continue transacting when some nodes are blocked or coerced?

**Formalization targets**
- Ledger entry canonicalization + validation rules (including conflict detection and resolution policy).
- Credit policy formal model: dynamic limit function based on trust, history, tenure, plus ramping for new members.
- Settlement protocol spec: receipt clearing semantics and idempotency (deduplication), plus dispute hooks.

**Prototype targets**
- Adversarial economics sim: synthetic coop with attackers (free rider, grab-and-run, limit gaming) and measured failure thresholds under different policy parameters. (The "economic safety" doc already enumerates attacks; the prototype must quantify.)
- Treasurer workflow test: export receipts + balances; verify audit properties (sum-to-zero) and show explanation path in UI.

**Failure modes and adversarial considerations**
- Coordinated default attacks; mitigation via conservative limits + governance emergency lock.
- Frivolous dispute spam blocks transactions.
- Partition-induced accounting forks; settlement "double spend" at federation boundary.

**Dependencies**
- Trust and authorization (for credit limits, treasuries)
- Receipts datastore
- Dispute/appeal model
- Scope/jurisdiction

---

### CCL: contract semantics, compilation, determinism, and capability safety

**Core research questions**
- What is the minimal semantic core of CCL required to express cooperatives' bylaws and policies without turning CCL into a general-purpose language (which increases attack surface)?
- Determinism classes: how are time, randomness, external data, and oracle calls controlled so that contract outcomes are reproducible? (Compute dispute resolution assumes determinism classes.)
- Capability safety: what is the smallest set of privileged primitives CCL can invoke (ledger read/write, proposal management, role changes), and how are they constrained?
- What is the formal compile target: constraints directly, or WASM + constraint receipts, or both? (Repo docs discuss CCL determinism and enforcement.)
- How do you make CCL explainable to non-technical users: is there a required "human-readable charter layer" with canonical diffs and enactment proofs?

**Formalization targets**
- CCL operational semantics subset (expressions, state updates, capability calls) + canonical serialization for hashing/receipt anchoring.
- Capability specification: preconditions, postconditions, and receipt fields required per call (e.g., ledger transfer must emit balance delta and policy reference).
- Determinism constraints: define allowed host environment, time sources, and I/O model.

**Prototype targets**
- "Governance primitives in CCL" MVP: implement two templates and validate with integration tests matching real coop workflow.
- Fuzz and negative testing for capability boundaries (attempt unauthorized ledger writes, ensure kernel enforcement).

**Failure modes and adversarial considerations**
- Contract gas/fuel griefing (complexity blowups).
- Nondeterminism leaks (wall clock/timezone, unordered maps, floating-point).
- Policy oracle capture: a contract looks legitimate to humans but encodes abusive constraints.

**Dependencies**
- Kernel constraint model and receipt schema
- Authorization/effective rights
- Economics and governance tracks

---

### Disputes, evidence, and appeals across scopes

**Core research questions**
- What is the unified dispute object model that can cover: ledger disputes, compute result disputes, and governance/rights appeals?
- What evidence types are admissible, and how are evidence objects stored, hashed, and privacy-controlled?
- Mediator selection: what trust thresholds and conflict-of-interest rules are required, and how does a community change them? (CCL dispute module already encodes mediator selection and penalties.)
- Cross-scope escalation: when does a coop dispute escalate to a community or federation scope, and what is the canonical "escalation receipt"?
- How do disputes interact with economics (penalties, write-offs, quarantines) without letting mediators become unaccountable mini-sovereigns?

**Formalization targets**
- Dispute lifecycle state machines for each dispute class + a shared envelope (status, parties, scope, deadlines, evidence references, resolution).
- Mediator selection and penalty rules spec (trust-weighted selection, escalation multipliers).
- Appeals routing spec (cross-scope escalation rules).

**Prototype targets**
- End-to-end ledger dispute scenario + governance escalation: file dispute → add evidence → assign mediator → escalate to governance vote → resolution receipt.
- Compute dispute scenario across verification modes (single, multi-executor, optimistic) with automated penalties and quarantine option.

**Failure modes and adversarial considerations**
- Dispute spam (DoS via contesting everything).
- Mediator cartel capture (a small group monopolizes dispute power).
- Evidence poisoning and leak (sensitive evidence posted publicly).
- Escalation griefing: malicious actors force cross-scope escalation to overload federations.

**Dependencies**
- Identity/personhood (who can file/mediate)
- Trust/authorization
- Receipts datastore and evidence storage semantics
- Governance (appeal votes; mediator removal)

---

### Naming, discovery, and navigable provenance without DNS

**Core research questions**
- What is the minimum naming primitive that lets users find "their coop services" without relying on global DNS, while staying explainable?
- How do service endpoint advertisements get authenticated, rotated, expired, and trust-gated? (Repo includes service endpoint models and discovery logic.)
- What is the human navigation model for receipts: how do users traverse "intent → constraints → decision → effect → receipt" across services and scopes without developer tools?
- Offline-first: what discovery and provenance queries must work on mobile with intermittent connectivity?
- How do you mitigate naming-level censorship (e.g., blocking specific bootstrap nodes or known service endpoints)?

**Formalization targets**
- Service-evidence signed record format (endpoint, scope, capabilities served, expiry, signature) and verification rules.
- A "receipt graph query protocol" spec: minimal query language for receipts by scope, actor, action type, and dependency edges.
- Human-facing naming schema: stable identifiers for scopes/services that are not DNS-hostnames, plus UI mapping rules.

**Prototype targets**
- "Receipts explorer" that can answer: "Why does Alice have the right to submit this proposal?" and "Where did this treasury transfer originate?" using only receipts + scope graph.
- Service discovery demo across 3 nodes on LAN, including key rotation and endpoint expiry.

**Failure modes and adversarial considerations**
- Endpoint spoofing (malicious node advertises as coop gateway).
- Eclipse attacks via discovery poisoning.
- UX capture: users can't distinguish "official coop service" vs "phishing mirror."

**Dependencies**
- Identity and auth (signing service ads)
- Receipts datastore and canonicalization
- Networking posture

---

### Networking, NAT traversal, and censorship resistance posture

**Core research questions**
- What connectivity assumptions are required for real coops/communities (home routers, CGNAT, mobile networks), and what protocols are needed for success? (MVC track flags NAT traversal as a pilot-critical gap.)
- What is the baseline secure transport: QUIC+TLS? How is identity bound to transport sessions (DID-TLS pattern)?
- NAT traversal: what subset of STUN/TURN is needed to reach "works for non-technical users," and how is relay selection governed by trust?
- How is censorship resistance expressed in architecture: multi-transport, relays, opportunistic routing, and "trust until proven" defaults?
- What is the network-level receipt: do we receipt-sign communications (envelopes) to support later audits and dispute evidence?

**Formalization targets**
- Transport security profile: handshake, key schedule use, session resumption rules, node identity binding rules.
- NAT traversal spec: STUN discovery + TURN relay fallback + relay authorization model.
- Gossip topic authorization and rate limiting as a formal policy (to avoid spam and infiltration).

**Prototype targets**
- NAT traversal integration test matrix: (home NAT↔home NAT), (mobile↔home), (CGNAT↔public), with and without TURN.
- Censorship simulation: block direct connections; verify relay fallback; measure leak of metadata and availability.

**Failure modes and adversarial considerations**
- Relay capture coerces traffic analysis.
- Denial of service on discovery channels.
- Partition-driven inconsistent state; must rely on receipts and conflict resolution.

**Dependencies**
- Identity/personhood (transport binding)
- Discovery and naming
- Threat model and operational posture

---

### Receipts datastore: provenance graph, sharding, retention, query UX

**Core research questions**
- What is the canonical receipt format that supports "government with receipts" and remains human-navigable?
- How do receipts link: do they form a DAG (edges to prior receipts), per-scope append-only logs, or both? What minimal structure enables audits without global consensus?
- What is the data retention policy across coops and communities: infinite ledger vs bounded receipts vs archival proofs? (Pilot operator docs already propose retention norms; formalize them.)
- How is the "receipt query surface" exposed securely to mobile clients while preventing information leakage across scopes?
- What is the sharding strategy: by scope, by receipt type, by time, by content hash?

**Formalization targets**
- Receipt canonicalization and hashing rules (encoding, field ordering, signature scheme).
- Provenance graph semantics: edge types ("caused-by", "authorized-by", "disputed-by", "superseded-by").
- "Accountability log" spec per scope: append-only log + inclusion proofs, following CT-like design patterns adapted to multi-scope sovereignty (no global log).

**Prototype targets**
- Receipts browser with graph view for a coop's last 30 actions, emphasizing non-developer comprehension.
- Storage benchmark: store and query 1M receipts partitioned by scope; validate worst-case query latencies and retention pruning.

**Failure modes and adversarial considerations**
- Receipt flood DoS.
- "Receipts not usable": information overload; users cannot find causality.
- Privacy leaks (receipts reveal sensitive relationships or service usage).

**Dependencies**
- Kernel constraint and receipt generation
- Scope model and naming/discovery
- UI architecture

---

### Compute fabric: placement, verification, disputes, and metered execution

**Core research questions**
- What is the minimal compute model that coops/communities can use early (hosting services, simple jobs) while preserving local sovereignty and avoiding global consensus?
- Placement: how are tasks assigned (bidding, scheduling), and what trust thresholds gate execution?
- Verification: what are the supported verification modes (single executor, multi-executor, optimistic challenge), and what triggers disputes?
- How do compute receipts relate to economic settlement (pay executor, penalize misbehavior), and what is the minimum set of receipt fields required?
- How do communities prevent compute capture (a few nodes become "cloud landlords")?

**Formalization targets**
- Compute job lifecycle state machine (submit → assign → execute → attest → settle → archive) plus dispute paths.
- Verifiable execution profile: deterministic execution requirements and receipt structure for output hash, fuel, timestamps.
- Economic settlement and penalty spec tied to compute disputes and trust score adjustments.

**Prototype targets**
- Vertical slice demo: submit deterministic job → execute → emit receipt → settle payment → challenge incorrect result and apply penalty.
- Executor diversity experiment: enforce placement constraints (e.g., "no single org executes >30% of jobs") and measure throughput impact.

**Failure modes and adversarial considerations**
- Fake execution receipts.
- Colluding executors in multi-executor verification.
- Arbiter capture in re-execution mode.
- Economic drain attacks via spammy cheap jobs; requires metering and admission controls.

**Dependencies**
- Identity and trust model
- Receipts datastore
- Economics settlement and dispute
- Networking posture

---

### Federation theory: inter-scope treaties, cross-scope settlement, and escalation

**Core research questions**
- What is the minimal "treaty" object that represents an inter-scope agreement (services, settlements, recognition of credentials, dispute escalation routes) without global consensus?
- How does a federation scope enforce treaties while preserving local sovereignty (local scopes can exit; treaties are versioned; effects are scoped)?
- Cross-scope settlement: what is the minimal clearing mechanism for receipts across scopes (federation receipt clearing exists in code).
- Who arbitrates federation-level disputes, and how do lower scopes appeal or exit?
- How are global commons formed: what is the "composition rule" for federations of federations?

**Formalization targets**
- Treaty schema: parties (scope IDs), obligations (constraint sets), settlement terms, termination conditions, and escalation paths.
- Cross-scope receipt clearing protocol: inclusion rules, deduplication, dispute marking, and local acceptance policies.
- Exit/withdrawal protocol: what receipts constitute "exit has been exercised" and how do dependent rights/economics unwind?

**Prototype targets**
- Two-coop federation pilot: shared infrastructure service hosted by one coop, paid by another, with federation-level receipts and dispute escalation.
- "Treaty violation detector": automatically detect when treaty constraints are violated (missing receipts, late payments) and file dispute in federation scope.

**Failure modes and adversarial considerations**
- Federation capture (a large coop dominates treaty terms).
- "Protocol imperialism": federation pushes kernel-level changes via treaties instead of oracle-level.
- Cross-scope spoofed receipts and settlement replay.

**Dependencies**
- Scope/jurisdiction model
- Governance lifecycle and receipt format
- Economics settlement and dispute mechanisms
- Discovery and naming (federation services)

---

### Threat model and security posture

**Core research questions**
- What is the explicit attacker taxonomy: state actor (censorship, seizure), capital actor (capture, coercion), insider (infiltration), outsider (spam/Sybil)?
- What must be assumed adversarial-by-default (external nodes) vs socially trusted (local coop) and how does that map to default policies in the kernel and UI?
- What are the key "security receipts" needed for auditability (key rotations, recovery events, treaty changes, governance upgrades)?
- How is TOFU justified and where must it be replaced with explicit trust establishment?
- How do you test semantic capture as a *security* threat (not just a governance problem)?

**Formalization targets**
- Security posture doc: required controls, threat mitigations, and explicit non-goals, tied to each track.
- Attack-surface inventory across kernel, oracle, UI, and network.

**Prototype targets**
- Red-team test suite:
  - Sybil trust graph injection attempts
  - Governance capture via proposal flooding
  - Discovery poisoning / endpoint spoofing
  - Receipt tampering attempts
- "Receipts under attack" drill: ensure auditing continues even during partial outages and partitions.

**Failure modes and adversarial considerations**
- Security-by-configuration (too complex for coops to operate).
- Hidden centralization (a small number of relay/bootstrap nodes become chokepoints).
- Privacy breaches via receipts metadata.

**Dependencies**
- All tracks; this is cross-cutting and should be maintained as a living spec.

---

### Migration, adoption, and mobile-first explainability

**Core research questions**
- What is the minimal onboarding flow that non-technical members can complete safely (identity, membership, token acquisition, first transaction, first vote), and what must be automated?
- What must the UI always surface: current scope, effective rights, applicable law/policy, and receipts trail, in human terms?
- What is the incremental adoption path for existing coops/communities (start with timebank ledger; add governance; add compute; add federation)? The MVC plan outlines a pilot path for a 10-person worker coop.
- How is operational continuity handled: backups, recoveries, upgrades, succession for admins/treasurers?
- What "hard stops" force product work before federation scaling (NAT, recovery, conflict resolution, UX)? MVC doc enumerates these as critical pilot blockers.

**Formalization targets**
- UX invariants: required explainability elements (rights reasoner output, scope breadcrumb, receipt drill-down).
- Operational runbooks and "failure receipts" (incident receipts, recovery receipts).
- Minimum viable coop/community reference profile: required roles, policies, and services.

**Prototype targets**
- Phase 0 LAN demo: run a coop timebank with Pilot UI using the existing onboarding steps, then add a receipts explorer panel and scope breadcrumbs.
- Pilot readiness simulation: 30-day operation including device loss recovery, node failure, and dispute filing.

**Failure modes and adversarial considerations**
- "Usability collapse": security requirements overwhelm non-technical users.
- Social recovery misconfiguration leads to lockouts or theft.
- Operator capture: a single admin becomes de facto sovereign because only they can operate the system.

**Dependencies**
- Identity/recovery
- Receipts datastore and explainability
- Governance and economics tracks
- Networking/NAT traversal

## Prioritized dependency graph and phase plan

Phase sequencing is based on one principle: **Phase 0 must prove legible sovereignty and receipts without requiring trust in the developers** ("government with receipts"), on a LAN with non-dev users. Phase 1 must survive real-world networking and operational failures for a single coop/community. Phase 2+ generalizes to federations and the universal commons without global consensus.

### Phase 0 LAN demo viability

**Goal criteria:** non-dev onboarding works, scope is legible, receipts are visible and navigable, basic governance and mutual credit operate on a LAN.

**Tracks to complete (minimum)**
- Kernel constraint model and Meaning Firewall boundary (freeze early kernel primitives; forbid semantic leakage).
- Identity and personhood (basic DID + multi-device enrollment; minimal recovery path or at least safe backup workflow; SDIS baseline).
- Scope and jurisdiction model (scope graph visible; actions always scoped).
- Authorization and effective rights (UI can answer "why can I do X").
- Governance lifecycle (simple proposal + vote + receipts, even if execution is manual).
- Economics minimal (log transactions + audit + basic limits).
- Receipts datastore + UI drill-down (receipt graph and breadcrumb navigation).

**Why these are Phase 0**
- If scope, rights, and receipts are not legible early, ICN becomes indistinguishable from either a centralized admin tool or an opaque protocol. The operator/treasurer UI docs already reflect what non-dev users need to do; Phase 0 must add receipt legibility to those flows.

### Phase 1 Real community or coop pilot

**Goal criteria:** a real coop/community can operate for months, including device loss, NAT conditions, disputes, and upgrades. The MVC track explicitly frames a 10-person worker coop pilot and flags recovery, NAT traversal, and conflict resolution/UX as blocking gaps.

**Tracks to complete (additions and hardening)**
- Networking, NAT traversal, and censorship posture (move from LAN to real-world).
- Economic safety rails fully enabled and monitored (dynamic limits, new member protections, dispute workflows).
- Disputes/evidence/appeals (ledger disputes and governance escalation path).
- Identity recovery: ship social recovery M-of-N with delay and mapping old→new DID, because key loss is guaranteed.
- Migration/adoption & ops: backups, runbooks, admin succession, upgrade procedures are required for pilot survival.
- Naming/discovery beyond LAN (bootstrap under hostile conditions and spoofing defense).

**Why these are Phase 1**
- A LAN demo can ignore NAT, long-lived reliability, and coercive environments; a pilot cannot. STUN/TURN are standard tools for NAT traversal and must be integrated into the threat+ops posture.
- Economic and dispute safeguards are specifically called out as defenses against common mutual credit failure patterns; pilots are where these must prove themselves under real user behavior.

### Phase 2+ Federation scale and global commons

**Goal criteria:** multiple coops and communities coordinate through federations, with inter-scope treaties, settlement, and compute scaling—without requiring global consensus.

**Tracks to scale**
- Federation treaties and inter-scope settlement (treaties as first-class, exit/appeal semantics, receipt clearing).
- Receipts datastore sharding/retention at scale (million+ receipts; privacy partitioning).
- Compute fabric (verifiable execution and disputes at federation scale).
- Naming/discovery without DNS at internet scale under censorship (stronger anti-eclipse, reputation-weighted discovery).
- Advanced privacy (selective disclosure and minimized receipts) consistent with explainability.

**Why these are Phase 2+**
- Treaty semantics and cross-scope settlement are inherently multi-party and require careful compositional reasoning; they should be informed by pilot experience (what agreements and failures actually occur).
- Large-scale data/compute introduce new centralization vectors; the system must prove sovereignty-preserving economics and governance first.

## Minimal formal core proposal

The "Minimal Formal Core" is what ICN must define correctly early so that everything else can be layered while respecting the Meaning Firewall. Kernel-level items are *generic* and semantics-blind; oracle/app-level items encode domain meaning (coops/community/federation law).

### Kernel-level primitives

These belong in the kernel because they define sovereignty boundaries, non-forgeable authority, and audit-grade receipts:

- **Scope identity and resolution**
  - `ScopeID` (stable identifier) + `ScopeLevel` (individual/coop/community/federation/commons) + parent linkage.
  - Deterministic resolution rules for "effective scope" of an action.

- **Actor identity and key state**
  - DID format and signed operation envelope rules (key rotation, recovery pointer).

- **Authorization evaluation contract**
  - A deterministic `EffectiveRights` function that maps (actor, scope, time/version) → capabilities, plus explainable proof path.

- **Canonical governance and receipts lifecycle hooks**
  - The kernel does *not* implement governance semantics; it must implement:
    - receiptable state transitions (proposal created, vote cast, tally accepted, execution applied),
    - and a canonical receipt format for "decision produced" and "effect applied."

- **Receipt canonicalization and anchoring**
  - Canonical serialization and hashing rules, signature rules, and provenance edge rules.

- **Inter-scope agreement representation (treaty envelope)**
  - A treaty is a typed object whose semantics are evaluated by oracles; the kernel must only validate identity, scope references, signatures, and receipt anchoring.

- **Naming and discovery primitives**
  - Signed service endpoint advertisements with TTL, scope binding, and trust gating hooks.

### Oracle/app-level primitives

These must remain above the Meaning Firewall (domain meaning lives here):

- Governance models (consensus, sociocracy, council delegation, emergency locks) implemented as CCL templates and community-specific oracles.
- Economic instruments beyond minimal ledger safety (multi-currency, demurrage, cross-ledger swaps, policy presets).
- Dispute mediation policy (mediator pools, penalty schedules, evidence admissibility).
- Compute placement policy (bidding, reputation weighting, locality constraints).
- UI experiences, onboarding wizards, and organizational workflows.

### Pseudocode sketches for minimal core algorithms

#### Scope resolution

```pseudo
// Kernel-level deterministic scope resolution
function resolve_scope(target_scope_ref, local_state) -> ScopeID:
    // target_scope_ref can be (scope_id) or (scope_id, version) or alias
    s = lookup_scope(target_scope_ref.scope_id)
    assert s.exists

    if target_scope_ref.version is present:
        assert version_exists(s, target_scope_ref.version)
        return (s.id, target_scope_ref.version)
    else:
        return (s.id, s.current_version)
```

Hard requirement: **actions must be rejected if scope cannot be resolved deterministically** (no "best effort" in kernel).

#### Effective rights computation with delegations

```pseudo
// Kernel-level: outputs both capabilities and an explanation DAG
function effective_rights(actor_did, scope_id, at_time) -> (Caps, ProofGraph):
    memberships = query_memberships(actor_did, scope_id, at_time)
    roles = union(m.role for m in memberships if m.active)

    // Delegations are edges: grantor -> grantee, scoped and time-bounded
    del_edges = query_delegations(target=actor_did, scope_id, at_time)

    // Compute closure with cycle detection
    caps = caps_from_roles(roles)
    proof = new_graph()

    queue = del_edges
    visited = set()
    while queue not empty:
        e = queue.pop()
        if e.id in visited: continue
        visited.add(e.id)

        if not delegation_valid(e, at_time): continue
        if not grantor_has_cap(e.grantor, e.cap, scope_id, at_time): continue

        caps.add(e.cap)
        proof.add_edge(e.grantor, actor_did, label="delegates:"+e.cap, receipt=e.receipt_id)

        // Support multi-hop delegation but enforce max depth to avoid DoS
        if proof.depth(actor_did) < MAX_DELEGATION_DEPTH:
            queue.extend(query_delegations(target=e.grantor, scope_id, at_time))

    return (caps, proof)
```

The explainability property ("why can I do this") falls out of `ProofGraph`.

#### Governance lifecycle receipts

```pseudo
state Proposal { Draft, Open, Closed, Executed, Disputed }

on create_proposal(intent, scope):
    require cap(actor, scope, "CreateProposal")
    receipt = make_receipt(type="proposal.created", intent, scope, actor)
    store(receipt)
    return proposal_id

on cast_vote(proposal_id, vote):
    require cap(actor, scope_of(proposal_id), "VoteProposal")
    receipt = make_receipt(type="proposal.vote_cast", proposal_id, vote, actor)
    store(receipt)

on close_and_tally(proposal_id):
    require cap(actor, scope, "CloseProposal") or time_expired(proposal_id)
    tally = deterministic_tally(votes(proposal_id), eligibility_set(proposal_id))
    receipt = make_receipt(type="proposal.closed", proposal_id, tally)
    store(receipt)
    if tally.passed:
        emit EffectPlan receipt (type="proposal.effect_plan", refs=[constraints...])
```

Key: **the kernel does not interpret vote meaning**; it verifies that the receipts form a valid, deterministic state machine and that the effect plan references constraints that can be checked.

#### Receipt structure for "intent → constraints → decision → effect"

```pseudo
Receipt {
  receipt_id: Hash(canonical_bytes(this_receipt_without_signature))
  type: enum
  scope: ScopeID
  actor: DID
  intent: IntentRef or inline intent
  constraints: [ConstraintHash] // compiled by oracle/app
  decision: {accepted|rejected|tally|...}
  effects: [EffectRef] // concrete state transitions or ledger deltas
  parents: [ReceiptID] // causal or authorization dependencies
  timestamp: u64 // policy: trusted time source class
  signature: Ed25519(actor_key, canonical_bytes)
}
```

Use Ed25519/EdDSA rules from RFC 8032 for signature verification and canonical signing inputs.

#### Naming and discovery (signed endpoint announcements)

```pseudo
function verify_service_ad(ad):
    require now <= ad.expires_at
    require resolve_scope(ad.scope_ref) exists
    require verify_signature(ad.publisher_did, ad.signature, ad.signing_bytes)
    require trust_policy_allows(ad.publisher_did, ad.scope_ref)
    return true

function choose_endpoint(service_name, scope):
    ads = query_ads(service_name, scope)
    valid = filter(verify_service_ad, ads)
    // rank by trust score, recency, and latency measurements
    return argmax(valid, score(ad.publisher, ad.updated_at, rtt))
```

Transport runs over QUIC (RFC 9000), with real-world NAT traversal via STUN (RFC 8489) and TURN (RFC 8656) when needed.

## Closing synthesis: what "hard edges" dominate the research agenda

Across all tracks, the same non-negotiable hard edge appears: **ICN must remain legible to humans while treating external nodes as adversarial and refusing global consensus as a default tool**. The research agenda therefore prioritizes:

- Formal kernel minimalism and Meaning Firewall enforcement (no semantic creep).
- Deterministic, explainable rights and provenance—even when the network is hostile and partitions occur.
- Pilot-ready survivability (identity recovery, NAT traversal, economic safeguards, disputes) before federation scale, consistent with the MVC plan.
