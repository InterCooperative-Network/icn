---
title: "Sprint 14: Closing the Economic Loop"
description: "PatronageTracker, ExecutionReceiptGate, and why linking governance decisions to execution outputs is harder than it sounds."
pubDate: 2026-03-21
tags: ["development", "sprint", "economics", "governance"]
---

Every sprint has a theme. Sprint 14's theme was: prove that decisions and their consequences are actually connected.

This sounds obvious. In practice, it is one of the hardest things to build correctly. Most software systems treat governance and execution as separate layers that communicate by convention. Someone decides something. Someone else executes it. The link between the two is a handshake agreement, not a verifiable chain.

ICN is trying to do something different. Sprint 14 is where that ambition became code.

## PatronageTracker: NY Corp Law §93 in Rust

New York's cooperative corporation law requires that worker-owned businesses track member contributions over time. The law exists to make surplus allocation transparent: members should be able to see what they contributed and verify that distribution reflects those contributions.

Nobody had implemented this as software that actually runs.

`PatronageTracker` is now in the codebase. It accumulates per-member contribution history, ties each entry to a governance period, and produces cryptographic artifacts that can be audited. If your cooperative is incorporated in New York and operates on ICN, the accounting your members are legally entitled to see is automatically generated and signed.

This is what "cooperative-native infrastructure" means. Not a generic accounting tool adapted for cooperatives. Software that knows what cooperatives need because it was built for that context from the start.

## ExecutionReceiptGate: Invariant Seven

ICN has seven core architecture invariants. Invariant seven is the most important one for economic integrity:

**Execution receipts close the governance loop.**

A governance decision without an execution receipt is a promise. An execution receipt without a linked governance decision is an unauthorized action. The protocol must enforce that these two things are always connected.

`ExecutionReceiptGate` is the enforcement mechanism. Before compute results are accepted and settlement proceeds, the gate checks that the task execution was authorized by a governance decision in the receipt chain. No decision hash, no settlement. No shortcuts.

The adversarial model here is important. ICN assumes peers are untrusted. A malicious compute operator could attempt to claim payment for work that was never authorized. The gate rejects this by construction. The receipt chain is the proof of authorization, and the proof is checked cryptographically, not by policy enforcement after the fact.

## DelegationManager: Persistence Across Restarts

Governance doesn't stop when a node restarts. If a member has delegated their voting power to a coordinator for a six-month governance period, that delegation needs to survive node crashes, network outages, and software updates.

`DelegationManager` now persists delegation state with gossip sync. When a node comes back online, its delegation graph is restored from local storage and reconciled with what peers have seen. The gossip layer ensures that delegation changes propagate without requiring all nodes to be online simultaneously.

This is table stakes for a production governance system. Sprint 14 made it real.

## What Closing the Loop Actually Costs

None of these features are simple. `PatronageTracker` required designing a contribution accounting model from scratch. `ExecutionReceiptGate` required threading receipt hashes through the entire compute pipeline and making the gate rejection a hard failure rather than a warning. `DelegationManager` persistence required careful conflict resolution for concurrent delegation changes.

The work is unglamorous. It is not the kind of thing that generates a press release. But it is the kind of thing that makes the difference between a demo and a system someone can actually run their cooperative on.

Sprint 14 closed the economic loop. Sprint 18 is hardening the network beneath it. The receipt chain is real. The governance is real. The only thing left is deployment.

We are closer than we have ever been.
