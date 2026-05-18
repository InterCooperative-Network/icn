---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-17
---

# ICN Thursday Meeting Brief — 2026-05-21

> **A reading-frame for one meeting.** This brief grounds what can and cannot be said about ICN to cooperative developers this Thursday. It is sourced from the repo at session start, not from memory or roadmap aspiration. Treat the [non-claims section](#what-we-should-not-claim) as load-bearing.

## One-sentence frame

ICN is social software infrastructure: a cooperative-owned substrate for standing, authority, decisions, obligations, receipts, evidence, and verifiable collective action.

## Ninety-second explanation

Democratic organizations today rent their institutional infrastructure. Governance lives in Loomio, Google Forms, and Slack threads. Accounting lives in QuickBooks or a spreadsheet. Membership lives in a Mailchimp list. Inter-organizational coordination lives in email. Each piece is owned by a landlord. When the landlord changes terms or shuts off access, the cooperative's institutional memory goes with it — and there is no way to prove to an outside party that a decision actually happened, by whom, under what rules, without that party trusting the organization's word for it.

ICN is the infrastructure layer that tries to give democratic organizations something they can own and verify instead. The core path is not "an app." It is an institutional spine: **standing → authority → decision → mandate → effect → receipt → evidence → review / federation**. The substrate enforces constraints; cooperative governance (charters, policy oracles, institution packages) carries meaning. The substrate does not understand "vote," "credit limit," or "trust score" — it sees signed records, capabilities, and constraint sets, and it produces receipts that can be verified later by anyone holding them.

The project is not finished. Most of the *institutional* surface — member-facing app, operator cockpit, federation-wide proof loops, cooperative pilots — is design and rehearsal, not production. The substrate and the proof architecture under it are mature enough to evaluate seriously and rehearse against fictional or sanitized data. They are not mature enough to entrust real institutional decisions to without more hardening, partner review, and explicit pilot formalization.

## What exists today

| Area | Current repo truth | Meeting-safe claim | Do not claim |
|---|---|---|---|
| **Core daemon / substrate (`icnd`)** | Tokio actor runtime, supervisor, kernel/app separation, gateway on port 8080, 34 crates in workspace. Identity (Ed25519 + Age keystore), networking (QUIC/TLS with DID-binding, mDNS), gossip with vector clocks and Bloom filters, double-entry mutual-credit ledger, CCL interpreter, governance proposal/vote primitives. | A daemon you build and run on your own hardware; identity, networking, ledger, governance primitives, and the gateway are working at the substrate level. Local + 3-node K3s convergence is tested. | "Production-ready." "Audited." "Scaling-tested past 10 nodes." "Hosted instance you can sign up for." |
| **Receipt / provenance architecture** | ADR-0026 four-layer proof envelope (GovernanceProof / ArtifactReceipt / FederationProvenance / ProvenanceQuery). `ProcessGateResultReceipt` runtime slice persisting through the opaque cascade (PRs #1755 / #1757 / #1758 / #1759). Action-card runtime emits proof-bearing receipts for `proposal`/`vote`, `action_item`/`complete`, `meeting`/`attend`. | The architecture for verifiable, content-addressed, chained receipts exists; one process-transition class is emitted and durably persisted on the production gateway path. | "Every state transition emits a receipt today." "Receipts alone prove institutional legitimacy." |
| **Opaque receipt storage / meaning firewall** | Gateway stores typed receipts as `(class, record_hash) → bytes` via `put_opaque` / `get_latest_opaque` / `list_opaque_for`. Atomic primary + secondary + hash-bind writes; collision detection with stable sentinels. Kernel never parses the body. | Adding a new receipt class is a one-file change in apps/, with no new typed governance import on the gateway. The kernel/app boundary is structurally enforced. | "All ten frozen-core invariants are CI-enforced today." "The firewall denylist is a hard gate." (Wave 1 is complete; Waves 2–6 are still migrating; the denylist is currently advisory.) |
| **Governance proposal / vote paths** | Proposals, votes, amendments, charter management, delegation, vote tally, effect manifests. Governance thresholds and credit limits read from ratified charters via policy oracles. `add_domain_member`, `remove_domain_member`, `activate_charter`, `create_proposal`, `cast_vote` live in `apps/governance/src/http/handlers.rs`. | Real proposal/vote/charter machinery exists and is exercised in tests and devnet. Charter constraints flow from CCL through the policy oracle into kernel enforcement. | "All paths require ratified mandates." "Bootstrap and direct-mutation shortcuts are gated by full governance." (See [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) §3 for the documented gaps.) |
| **Action Cards** | ADR-0027 ActionCard schema published as institution-package contract (#1764). `GET /v1/gov/me/action-cards` member endpoint. Three source paths proof-bearing; `signal_rule` and `obligation_lifecycle` are RFC-gated and not implemented. Generic example fixture exists at `docs/contracts/institution-package/action-card.example.json`. | A member-facing card schema is published, validatable, and emitted for three governance source paths. | "Stewards have their own ActionCard contract." (That gap is named in #1837, still open.) "All cooperative workflows can be expressed as ActionCards today." |
| **Anti-entropy proof rail** | Spec at [`docs/spec/network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md). Wire-stable record shapes landed: `AntiEntropyProbe`, `StateDigest` family, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `PeerSyncReport`, `SyncDegradedStatus`, `FederationSyncWindow`, `QuorumSyncCheck`, `RoutingProof`, `RedundancyProof`. The entire spec-named identifier set is wire-stable as of #1862. | The proof-rail identifiers are wire-stable in `icn-kernel-api`. The proof-loop *contract* — eight phases, eighteen divergence classes, nine-field cockpit surface, seven-string member-shell vocabulary — is defined. | "Live anti-entropy proof loops are running." "Federation peers are exchanging RepairReceipts in production." No code path constructs, signs, gossips, classifies, repairs, or autonomously emits these records yet. |
| **Member shell** | Spec at [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) — mobile-first, offline-tolerant, accessibility-first, plain-language-first rendering contract. Fixture-backed rendering rehearsal (Slice A) consuming `PeerSyncReport` and `SyncDegradedStatus`. | A v0 rendering contract for the member surface exists. Slice A fixture consumes the merged proof-rail records deterministically. | "Mobile or web member app." "Offline-sync working in production." There is no live member shell implementation. |
| **Steward cockpit** | Spec at [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) — twelve cockpit surfaces, fourteen operator action-card scenarios, member-impact summary mapping. Fixture-backed divergence-render rehearsal (Slice A) consuming `SyncDegradedStatus`. Steward required-action card contract is open work (#1837). | A v0 contract for the operator-facing surface exists. Slice A fixture renders deterministic technical detail. | "Live cockpit dashboard." "Operators are running an ICN cockpit against a real federation today." |
| **Debian appliance / node image** | [`docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../architecture/DEBIAN_APPLIANCE_MODEL.md). Scaffold landed (#1865); local QCOW2 build + one-VM boot smoke verifies `/v1/health` on 8080 (#1866). Image stage: **Bootable dev image**. | A local dev image can be built and smoked. The substrate boots, generates per-VM secrets on first boot, and serves a health endpoint. | "Production appliance." "Signed release." "Immutable rootfs." "A-B updates." "Claim flow." "Partner-handoff ready." "K3s / DNS / Forgejo state mutated." Not any of these. |
| **Federation** | Attestations, clearing, netting, registry, router scaffolding exist. AgreementSchema lifecycle not yet wired through end-to-end. Federation-side proof-loop records (`FederationSyncWindow`, `QuorumSyncCheck`) are wire-stable; no runtime federation emits them. | Federation primitives exist as scaffolding. The federation-bound proof contract is defined at the spec level. | "Live inter-cooperative federation." "Cross-coop clearing in production." "End-to-end federation agreement lifecycle is wired." |
| **Abuse-case hardening** | [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md). Ten one-line doctrine rules; ten code-anchored abuse stories; matching hardening tracks named with P0–P3 candidates. Strategy / doctrine only — no runtime change. | The institutional-failure-mode doctrine is written and code-anchored. Reviewers can quote the rules at design time. The substrate is honest about which paths are currently shortcuts. | "All hardening tracks are implemented." "Direct membership mutation, direct charter activation, fail-open resolver paths, and broad `governance:write` scope are closed today." Most tracks are open implementation work. |
| **Design system / design principles** | [`docs/DESIGN_PRINCIPLES.md`](../DESIGN_PRINCIPLES.md) consolidates the three tiers (operational invariants, firewall contract, frozen-core invariants). Design language brief, accessibility baseline, content style guide, and twelve-category accessibility gate exist. Claude Design setup documented (#1867). | The kernel's safety invariants are written down in one place. The design language has a normative brief, an accessibility baseline, and a working setup for visual contributions. | "Member shell visual identity is final." "Design system is implemented in code." No live UI consumes the design system yet. |

## What is fixture-backed

A **fixture-backed** path proves a shape deterministically inside tests or rehearsals. Bytes go in, bytes come out, and a test asserts what they look like. Fixture-backed is not the same as live: there is no real network, no real peer disagreement, no real cooperative making real decisions. The value of a fixture is that it forces the contract to be precise enough to render, while the failure modes of fixture work — drift between fixture and code, the absence of real users, the absence of adversaries — remain real.

Currently fixture-backed in the repo:

- **Receipt-index anti-entropy Slice A** (#1838 / #1855) — consumes `PeerSyncReport` to render a degraded receipt-index sync deterministically.
- **Steward cockpit Slice A** (#1840 / #1859) — consumes `SyncDegradedStatus` to render the operator-facing divergence row deterministically.
- **Member shell Slice A** (#1839 / #1859) — consumes `SyncDegradedStatus` to render the member-facing "Sync delayed" plain-language status deterministically.
- **Public proof-record retrofits** — when a new wire-stable record lands (e.g. `PeerSyncReport`, `SyncDegradedStatus`), the matching cockpit and shell fixtures are wired to consume it before the spec layer treats the record as done.
- **Redundancy proof Slice B** (#1862 / branch HEAD as of 2026-05-17) — replica-count `RedundancyProof` fixture exercising the wire-stable `RedundancyProof` / `RedundancyOutcome` records.

Each Slice A / Slice B fixture is a *rendering rehearsal*, not an integration test of live emission. The Slice C federation placement of `QuorumSyncCheck` and any live runtime emission path remain forward-direction.

## What is only design / spec

These contracts exist at the spec level and are not implemented as live runtime surfaces. They are read-then-render contracts; nothing in them implies a working app.

- **Live member shell implementation.** [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) is a design-level rendering contract. There is no native iOS / Android app, no PWA, no web shell consuming it in production.
- **Live steward cockpit dashboard.** [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md) is the operator-facing complement, also design-level. No HTML / CSS / JS / React component implements it.
- **Steward required-action card contract** (#1837). ADR-0027 covers member ActionCards; the cockpit's fourteen operator scenarios cannot be represented by its closed enums. The gap is open work.
- **Compute placement schemas.** `PlacementDecision` and `ExecutorAdmissionDecision` (#1836) are filed as schema follow-ups from the architecture-spec sprint, not yet wire-stable.
- **Private disclosure boundary.** `ScopedVault` body-byte non-rendering is doctrine ([`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) §2.9 / §4.9). The regression-test fixture matrix that enforces it is named, not implemented.
- **Production hardening tracks** from the abuse-case strategy: scope decomposition of `governance:write`, fail-closed resolver / checker policy in production, closed lifecycle vocabulary across API / shell / cockpit, per-effect idempotency, governance-parameter sanity bands, shell / cockpit fixture matrix, PrivateEvidence non-rendering regression, typed-receipt atomicity inventory. P0 candidates named in §14; none filed in this packet.
- **Full live federation proof-loop / runtime emission.** The wire-stable records (`AntiEntropyProbe`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `PeerSyncReport`, `SyncDegradedStatus`, `FederationSyncWindow`, `QuorumSyncCheck`, `RoutingProof`, `RedundancyProof`) are constructible and testable in isolation only. No code path constructs, signs, gossips, classifies, repairs, or autonomously emits them yet.

## What is explicitly not ready

Bluntly, before this meeting:

- **Not production-ready.** Single sentence: the project is deployed as a working reference implementation with a growing test suite, not a v1.0 release. Do not use ICN to hold or reference anything whose loss would be consequential.
- **Not live federation.** Inter-cooperative federation at the runtime level is not running anywhere. Federation primitives exist as scaffolding; AgreementSchema lifecycle is not wired end-to-end.
- **Not a formal partner pilot.** NYCN is the intended first cooperative partner — active partnership track — but **not** a formally committed pilot. The next concrete step in Phase 2 is presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers and then formalizing the pilot. Phase 2 status is ⏳ in [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). Phase 2 is not complete.
- **Not a live mobile or member app.** No iOS, Android, PWA, or web shell consumes the v0 spec.
- **Not a live cockpit dashboard.** No operator surface is running against a real federation.
- **Not a signed, immutable, or production appliance.** The Debian image is a local bootable dev image. No signed release, no A-B updates, no immutable rootfs, no claim flow, no partner federation activation, no measured boot, no TPM-backed identity.
- **Not externally audited.** No external security audit, no legal/regulatory certification. The terminology discipline and the five ledger invariants are deliberate choices to stay compatible with cooperative law and money-transmitter exemptions, but this has not been externally certified.
- **Receipts alone do not prove legitimacy.** The substrate can produce procedurally clean records of institutionally illegitimate sequences — see the [abuse-case hardening strategy](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) for ten code-anchored stories. Hardening tracks are open work, not closed.
- **Abuse-case hardening is doctrine, not implementation.** The ten one-line doctrine rules are written down; most of the named hardening tracks are open.

## The strongest proof path

The institutional spine is what to walk a serious reader through. It is the through-line that connects the substrate to the human practice it is supposed to support.

```
Decision
  → authority / mandate
    → effect dispatch
      → evidence / receipt
        → proof loop (detect / prove / identify / repair / emit / surface)
          → member-facing status (plain language)
          → steward-facing detail (technical)
            → review / challenge / federation
```

Why this matters:

- ICN is trying to make institutional action *verifiable* without turning the infrastructure into a semantic dictator. The kernel enforces constraints. Apps and institutions carry meaning. The proof rail is what connects "something happened" to "you can challenge it without trusting us."
- A receipt is not the end of the story. It is the start of a chain that runs through member shell ("what actually happened, in plain language"), through steward cockpit ("what diverged, what was repaired, what is degraded"), into federation review and challenge paths.
- The doctrine in [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) §2 is the precondition for the spine being honest: receipts prove events, not legitimacy; authority shortcuts must label themselves; unresolved standing is not standing; accepted is not applied. The spine assumes those rules are enforced; the implementation that gets there is sequenced and unfiled.

## Meeting demo spine

A sequence to walk verbally or by opening one document at a time. The goal is legibility, not theatrical completeness. Do not try to demo every repo subsystem.

1. **Open [`docs/OVERVIEW.md`](../OVERVIEW.md).** Start with the one-paragraph definition (constraint engine, eight primitives, four entity types) and §9 *Current State* (strong / partial / not ready). This sets the honesty bar.
2. **Open [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md).** Show that Phase 2 is ⏳ and the next concrete human gate is organizer presentation → pilot formalization → first operator rehearsal. This is the phase truth; do not improvise on it.
3. **Open [`docs/DESIGN_PRINCIPLES.md`](../DESIGN_PRINCIPLES.md).** Tier 1 (operational invariants), Tier 2 (firewall contract), Tier 3 (frozen-core invariants). One paragraph each is enough. This is the constraint-engine / meaning-firewall frame.
4. **Open [`docs/spec/network-anti-entropy-proof-loops.md`](../spec/network-anti-entropy-proof-loops.md).** Walk the eight-phase proof loop and the steward / member rendering surfaces. Name which records are wire-stable; name what is forward-direction.
5. **Open [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) and [`docs/spec/steward-cockpit-v0.md`](../spec/steward-cockpit-v0.md).** Show one v0 design principle from each (member-shell: "plain language before formal records"; cockpit: "stewardship-not-domination"). Make clear these are *contracts*, not implementations.
6. **Open [`docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md`](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md).** Read §2 (the ten doctrine rules) aloud if needed. This is how the project is honest about legitimacy.
7. **Close with the [meeting asks](#possible-collaboration-asks).** Move the conversation toward what cooperative developers can help review or design.

Optional, only if the room asks: [`docs/architecture/DEBIAN_APPLIANCE_MODEL.md`](../architecture/DEBIAN_APPLIANCE_MODEL.md) for "what does deployment actually look like." The honest answer ("bootable dev image, not production, not signed, not partner-ready") is part of the story.

## What we should not claim

- We do not claim production readiness.
- We do not claim a live federation.
- We do not claim a formally committed cooperative pilot. NYCN is an active partnership track, not a formally committed pilot.
- We do not claim a live member or mobile app.
- We do not claim a live steward cockpit dashboard.
- We do not claim a signed, immutable, A-B-updated, or production-ready appliance.
- We do not claim an external security audit, legal certification, or regulatory clearance.
- We do not claim that all ten abuse-case hardening tracks are implemented.
- We do not claim that receipts alone prove institutional legitimacy.
- We do not claim that the kernel/app firewall is fully CI-enforced today — Wave 1 is complete; the firewall denylist is currently advisory through Waves 2–6.
- We do not use the words *payment*, *currency*, *balance*, *wallet*, *token*, *crypto*, *blockchain*, or *timebank* for ICN-native primitives. The vocabulary is **settlement, obligation, allocation, unit, position, receipt, provenance, evidence**. The terminology is a regulatory boundary, not a style preference.
- We do not present cooperative developers with a finished product. We present them with infrastructure they can stress-test before it is too late to redirect.

## Questions for cooperative developers / Launch.coop

- What is the smallest cooperative-development workflow where verifiable standing, authority, and receipts would matter immediately — and which workflow would be a waste of this infrastructure?
- Where do cooperatives currently lose institutional memory? When a chair rotates, when a platform migrates, when a fiscal sponsor changes — what gets dropped on the floor?
- What decisions need proof for funders, members, boards, fiscal sponsors, accountants, partner organizations, or future versions of themselves?
- What would make a cooperative developer trust or distrust a tool like this? What surface would you look at first, and what would you look for?
- What must remain human / process-first and should *not* be prematurely automated? What are the explicit anti-software boundaries?
- What legal / accounting boundaries should ICN respect from day one — cooperative law, member protections, fiduciary responsibilities, anti-money-transmitter framing, jurisdiction-specific rules?
- What would be a realistic first rehearsal that does *not* require production deployment? A tabletop walkthrough, a fixture rehearsal against sanitized data, a paper-record reconstruction?
- What existing cooperative software / processes would ICN need to **complement** rather than replace? What is already working that we should not displace?
- Where does a cooperative's institutional vocabulary need to remain its own — and where would a shared vocabulary (settlement, obligation, allocation, etc.) help cooperatives talk to each other?
- Who else should be in this conversation — cooperative accountants, legal advisors, federation organizers, mutual aid operators — that we have not invited yet?

## Possible collaboration asks

Concrete, not vague.

- Review the institutional spine (decision → authority → mandate → effect → receipt → evidence → review / federation) for realism against actual cooperative practice. What is missing? What is over-engineered?
- Identify one small cooperative workflow that would be suitable for a fixture or rehearsal slice — something small enough that a tabletop walkthrough is the right ambition, not a deployment.
- Help validate vocabulary and legal / accounting framing. Is "settlement" the right word in your community? Where would "obligation" or "allocation" mislead a non-technical member?
- Help identify what evidence and receipts outside parties (funders, accountants, regulators, partner cooperatives) would actually care about. What is the *minimum receipt* that closes a real question?
- Help design a first "operator rehearsal" — fictional or sanitized data only — that exercises the member shell + steward cockpit fixture matrix end-to-end without ever entering a partner's real records.
- Introduce cooperative developers, accountants, or legal advisors who can stress-test the model against the realities of cooperative practice. Adversarial, not promotional.
- Help name what *not* to build. Anti-features are as load-bearing as features in this kind of project.

## Close: honest meeting posture

This meeting is not a launch. It is a legibility and calibration meeting. The goal is to let serious cooperative people look at the machinery, identify what is useful, identify what is dangerous or unclear, and help choose the smallest honest next rehearsal. If the meeting closes with one concrete, sanitized rehearsal target and one or two cooperative developers willing to stress-test the institutional spine, that is a successful meeting. Anything beyond that — and certainly any framing that sounds like "we're ready to deploy" — is an overclaim and should be walked back.
