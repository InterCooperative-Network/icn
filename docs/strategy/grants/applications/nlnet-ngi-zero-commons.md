---
Status: draft — submission-ready, pending checklist items
Funder: NLnet NGI Zero Commons Fund (13th call)
Amount target: €50,000
Deadline: 2026-06-01 12:00 CEST
Last Reviewed: 2026-05-22
---

# NLnet NGI Zero Commons — Application Draft

**Slice:** Federation Proof-Loop Runtime · **Apply at:** [nlnet.nl/propose](https://nlnet.nl/propose/) · **Deadline:** 2026-06-01, 12:00 CEST

> This file is the submission draft. Remaining blockers, the metrics-verification log, the
> overclaim audit, and the licensing assessment are in
> [`nlnet-ngi-zero-commons-checklist.md`](nlnet-ngi-zero-commons-checklist.md).

---

## Project name

InterCooperative Network (ICN) — Federation Proof-Loop Runtime

## Can you explain the whole project and its expected outcome(s)?

The InterCooperative Network (ICN) is open, cooperative-owned infrastructure for institutional
coordination. It lets democratic organisations — cooperatives, community land trusts,
federations — record decisions, obligations, and their provenance on infrastructure they run
themselves, and coordinate across organisational boundaries without depending on a commercial
platform. ICN's substrate exists today as a working reference implementation: decentralised
identity (Ed25519 DIDs), peer-to-peer QUIC/TLS networking, governance primitives, an
obligation-and-settlement ledger, and a layered receipt-and-provenance architecture, all on a
"constraint engine" design that keeps the kernel domain-agnostic.

This grant funds one bounded slice: the **federation proof-loop runtime**. ICN has a complete
specification for an eight-phase anti-entropy loop — observe, exchange state digests, detect
divergence, classify it, propose a repair, apply it, verify, emit evidence — and the wire-stable
record types the loop exchanges are already implemented and merged. What does not yet exist is
the runtime that drives the loop end to end. This project implements that runtime in ICN's
networking and protocol layers so that two independent ICN deployments can converge on shared
institutional state, detect and classify divergence, repair it, and emit an independently
verifiable receipt at each phase.

The expected outcome is a working, documented federation runtime: cooperatives running separate
ICN deployments can demonstrate — to each other, to auditors, to their members — that their
shared records genuinely agree, with proof anyone can check and no central server to trust.

## Have you been involved with projects or organisations relevant to this project?

ICN is developed by Matt Faherty, who has built it from first principles into a working
reference implementation. The Rust workspace declares 44 members — 37 library crates, 4
application crates, and 3 binaries — covering identity, networking, gossip, governance, the
ledger, the receipt-and-provenance layer, and the kernel/app boundary. The codebase carries a
substantial automated test suite spanning unit and multi-node integration tests, and is
exercised on a multi-node Kubernetes test cluster. The receipt-and-provenance architecture, the
kernel/app "meaning firewall," and the anti-entropy specification this grant builds on were all
designed and implemented within the project.

Matt co-organises the New York Cooperative Summit and is in active conversation with the New
York Cooperative Network (NYCN), the intended first rehearsal partner for ICN's federation
work. The project also draws on an advisory network in cooperative development (Cooperative
Fund of the Northeast; Institute for the Cooperative Digital Economy).

## Requested amount

€50,000.

## Explain what the requested budget will be used for. Does the project have other funding sources?

The grant funds a six-month, milestone-bound work plan on the federation proof-loop runtime.
Indicative allocation, each line tied to delivered work:

| Item | Amount | Tied to |
|------|--------|---------|
| Implementation and testing of the runtime | €34,000 | The six monthly milestones below |
| Test deployment — hardware + hosting for an isolated two-deployment anti-entropy run | €6,000 | Month 5–6 rehearsal |
| Documentation and operator runbook | €4,000 | Developer docs + runbook deliverable |
| Security-review preparation | €3,000 | Threat model + test harness for independent review |
| Administration and fiscal-sponsor fees | €3,000 | Grant administration |
| **Total** | **€50,000** | |

ICN has not previously received public funding; it is currently sustained by the founder's own
time and a small voluntary-contributions page. Grant funds will be administered through an
open-source fiscal sponsor, reimbursing project costs against delivered milestones rather than
functioning as open-ended salary.

## Compare your own project with existing or historical efforts

ICN is not a global-consensus system: it has no shared global ledger, no consensus protocol,
and no economic-incentive layer — cooperatives need verifiable coordination, not global
agreement. It is not a hosted platform: there is no vendor, no subscription, and no lock-in;
each organisation runs its own deployment.

Existing tools each solve part of the problem and miss the rest. Loomio records votes but
cannot prove them to someone who was not present. ActivityPub federates social posts, not
institutional decisions with provenance. State-replication libraries (for example CRDT systems)
converge replicas but emit no signed, classifiable evidence of *why* replicas diverged or *how*
they were reconciled. The federation proof-loop is different in that convergence and verifiable
provenance are one mechanism: every divergence is classified, every repair is signed, and every
phase emits a receipt any party can check independently. Recent NGI Zero work on independently
verifiable evidence and audit trails for software systems shows the direction is timely; this
project applies the same principle specifically to federated democratic institutions.

## What are significant technical challenges you expect to solve?

1. A deterministic, classifiable taxonomy of divergence between independent ICN deployments —
   and demonstrating the classifier is sound enough to act on.
2. Idempotent, signed repair operations, so a replayed or partial repair cannot produce a
   record that is false but still passes verification.
3. Keeping the runtime inside the kernel/app "meaning firewall": it must transport, classify,
   and verify evidence without the kernel interpreting domain semantics.
4. Safe degradation under network partition — a deployment that cannot converge must surface a
   "degraded" status, never silently present stale state as agreed.
5. Keeping anti-entropy digests cheap enough to run on a single low-resource self-hosted node,
   so the smallest cooperative can still participate.

## European dimension

ICN is open infrastructure for digital sovereignty: it lets European cooperatives, federations,
and communities run institutional coordination on hardware they control, under governance they
set, with no dependency on a commercial platform or non-European cloud. This directly serves
the Next Generation Internet goal of an internet that is open, trustworthy, and under public
rather than proprietary control. The cooperative movement has deep European roots and an active
technology community — Decidim and the broader civic- and cooperative-technology ecosystem —
and ICN's self-hosting model, commons-governance design, and open record formats are built for
interoperability with that ecosystem rather than competition with it. The proof-loop's record
formats are designed toward open standardisation, so European federated and fediverse projects
can adopt or interoperate with verifiable institutional provenance. The project actively seeks
review and engagement from European cooperative-technology peers as part of this work.

## Describe the project's ecosystem and how you will engage with it

All ICN source is published under recognised open-source licenses in the public
`InterCooperative-Network/icn` repository (AGPL-3.0 at the repository root; reusable library
crates under MIT OR Apache-2.0). Every output of this grant — runtime code, developer
documentation, and the operator runbook — will be released openly in that repository. The
runtime will be exercised in a controlled two-deployment anti-entropy rehearsal, with NYCN as
the intended first rehearsal partner. ICN will seek review from the wider open-infrastructure
and cooperative-technology community, including European projects such as Decidim, as
prospective review and engagement targets. The proof-loop record formats are designed toward
eventual open standardisation so that other federated systems can interoperate with them.

## Deliverables — six-month plan

- **Month 1–2:** `AntiEntropyProbe` + `StateDigest` runtime in the networking layer.
- **Month 2–3:** `DivergenceEvidence` + `RepairPlan` runtime; the divergence classifier.
- **Month 3–4:** the eight-phase loop integrated in the protocol layer; signed, idempotent repair.
- **Month 4–5:** fixture-based test slice; safe-degradation behaviour under partition.
- **Month 5–6:** a controlled two-deployment anti-entropy rehearsal between independent ICN
  deployments (one configured as the NYCN rehearsal package); receipts emitted at every phase.
- **Throughout:** developer documentation, operator runbook, and security-review preparation.
