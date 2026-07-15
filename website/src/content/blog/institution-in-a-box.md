---
title: "The Core Loop, Demonstrated: A Local Devnet Walk-through"
description: "What the cooperative core loop looks like running on ICN — four flows you can run yourself on a local devnet."
pubDate: 2026-03-21
tags: ["demo", "governance", "cooperative"]
---

> **Update (July 2026):** this March post described a local devnet
> walk-through, and its original title oversold that as a "live demo."
> Everything below runs on your laptop against fictional organizations —
> it demonstrates real mechanisms, not a deployed network. The current
> honest map of what is and isn't real is
> [What's Real Now](/whats-real-now); the current human-facing wedge is
> the Rehearsal Node.

The most common question we get is: "What does it actually do?"

Fair question. Coordination infrastructure is abstract until it isn't. So here is what it actually does, in the form of a demo you can run yourself.

## The Setup

Three nodes. Three cooperatives. One federation.

Each node runs `icnd`, the ICN daemon. Each has a DID generated from a local keypair. The nodes discover each other over the local network via mDNS, establish encrypted QUIC connections, and begin gossiping governance state.

Start time to three connected nodes: about thirty seconds. No configuration server. No cloud account. No "sign up for a developer key." The network builds itself from the cryptographic identities.

## Flow 1: Governance Proposal

A member of Cooperative A submits a governance proposal: allocate 500 commons credits to a shared infrastructure fund.

The proposal goes to `icnd` via the RPC API. The governance engine encodes it as a signed artifact, gossips it to all members, and opens a voting period. Members vote by signing a vote transaction with their keypair. The vote is counted. The proposal is accepted.

What makes this different from a voting app: the accepted proposal produces a `GovernanceReceipt` with a hash. That hash follows the proposal into every downstream action. If someone later tries to execute an allocation without that receipt in the chain, the protocol rejects it. The decision and the consequence are cryptographically linked.

## Flow 2: Patronage Distribution

Cooperative B runs `icnctl coop distribute-patronage`. The PatronageTracker computes each member's contribution share for the period, generates a distribution receipt, and posts it to the ledger.

The member who contributed the most doesn't get to decide they contributed the most. The tracker computes it from the signed work records accumulated during the period. The math is public. The inputs are auditable. The output is a signed artifact.

If your cooperative operates in New York and is required to distribute surplus based on patronage, this is the flow that does it correctly, in a way that can be audited and verified by members and, if necessary, by regulators.

## Flow 3: Federated Resource Allocation

Cooperative A has compute capacity. Cooperative C needs compute. They have a federation agreement.

Cooperative C submits a compute task. The federation protocol routes it to Cooperative A's available nodes. The task executes. An `ExecutionReceipt` comes back. The receipt is linked to the federation agreement that authorized the allocation, and the allocation is settled between the two cooperatives — recorded on both sides as signed journal entries.

No platform in the middle. No third-party routing compute marketplace. Two fictional organizations on a local devnet, coordinating directly, with a verifiable record on both sides. (No two real cooperatives are federating in production — that distinction matters, and [What's Real Now](/whats-real-now) keeps it explicit.)

## Flow 4: Audit Report

After the demo's governance, patronage, and compute flows run, `icnctl audit verify` generates a report. Every decision links to its execution. Every execution links to its authorization. Every allocation links to the receipt that proves it happened.

The audit report is not a dashboard that someone generated. It is a traversal of the cryptographic receipt chain. If the chain is valid, the report is valid. If anyone tampered with a record, the chain breaks and the report says so.

This is what cooperative accountability looks like when the infrastructure takes it seriously.

## Run It Yourself

```bash
git clone https://github.com/InterCooperative-Network/icn
cd icn && just devnet-up
# Three nodes start, form a network, ready for demo flows
bash demo/scripts/present-governance.sh --port 9080
```

The demo runs on your laptop. No cloud required. No account required. The cryptographic infrastructure is local, real, and auditable.

If you want to be part of building what it demonstrates, the issues are labeled, the codebase is open, and we have a Matrix room.

The core loop is real code you can run. What remains between "runs on a laptop" and "carries a real institution" is exactly the part we refuse to hand-wave — [What's Real Now](/whats-real-now) is the running answer.
