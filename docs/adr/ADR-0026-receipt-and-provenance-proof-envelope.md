---
id: "0026"
title: "Receipt and Provenance Proof Envelope"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["receipt", "provenance", "envelope", "audit", "back-fill"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partial"
references:
  - "ADR-0008 (Receipt Chain Vertical Slice and Audit Verify)"
  - "ADR-0011 (Canonical Truth Ownership)"
  - "ADR-0012 (Federation State Origin Model)"
  - "ADR-0014 (Constitutional Object Model)"
  - "ADR-0019 (Authority Grant Minting and Mandate Persistence Seam)"
  - "ADR-0020 (Institutional Bootstrap Activation)"
  - "PR #1648 (FederationProvenance persistence)"
  - "icn/crates/icn-governance/src/proof.rs"
---

# ADR 0026: Receipt and Provenance Proof Envelope

## Status

**Accepted (2026-04-26).** Back-fill consolidating the receipt and provenance work that has shipped piecemeal across ADRs 0008/0011/0012/0014/0019/0020 and across PRs landing through #1648. Records the envelope shape so cross-coop verifiability and audit endpoints (registry tranche 4) plug into a written contract.

## Context

ICN's institutional-memory story has been built incrementally:

- ADR-0008 established the receipt chain and audit-verify CLI.
- ADR-0011 located canonical truth ownership at the gateway.
- ADR-0012 placed federation state at the gateway, not governance/compute.
- ADR-0014 introduced `GovernanceProof` and the typed authority object model.
- ADR-0019 recorded the mandate persistence seam.
- ADR-0020 threaded receipt persistence into the institutional activation chain. ADR-0020 step 7 (action cards) verifies end-to-end that the `proposal`/`vote` action-card source path closes the proof loop into a Layer 2 `GovernanceDecisionReceipt` keyed by the same `proposal_id` the card surfaces as `source_id` (see `icn/apps/governance/tests/me_action_card_receipt_chain.rs`).
- PR [#1648](https://github.com/InterCooperative-Network/icn/pull/1648) made FederationProvenance persistent (no more in-memory loss across restart).

The pieces are real and shipping. What was not written down is the **envelope shape that all of these contribute to**. Without a single named envelope, follow-up work (cross-node verifiability, audit query endpoint, public attestation surface) risks rebuilding the wrapper for the third time.

## Decision

The receipt and provenance envelope has the following layered shape:

### Layer 1 — `GovernanceProof`
The proof of a single decision. Fields: `proposal_id`, `domain_id`, `outcome` (Accepted | Rejected | NoQuorum), `vote_tally`, `vote_hash` (merkle root of the vote set), `timestamp`, `signer_did`, `proof_hash`, `signature`. Defined in [icn/crates/icn-governance/src/proof.rs](../../icn/crates/icn-governance/src/proof.rs).

### Layer 2 — `ArtifactReceipt`
The receipt wrapping a `GovernanceProof` plus its institutional context: which mandate it produced (if any), which charter it activated against, which authority grants it minted. Persisted in the gateway receipt store.

### Layer 3 — `FederationProvenance`
The cross-coop chain: when a coop adopts another coop's clearing decision, this layer records the source proof, the adoption signature, and the persisted local copy. Persistent post-#1648.

### Layer 4 — `ProvenanceQuery` (forward, candidate ADR-0072)
The query surface that exposes the chain to auditors and federated peers. Not yet written.

### Envelope guarantees

- **Immutable.** Once a layer's record is signed and persisted, it is not edited. Reversal is a counter-record, not a mutation.
- **Re-verifiable.** Anyone with the public keys can verify the signature chain. Vote sets are merkle-rooted; proof hashes bind the set.
- **Holder-queryable.** A holder DID can discover its relevant governance artifacts via `/me/standing` plus `/me/action-cards`; today `/me/action-cards` is the discovery / index surface, exposing `source_id` for two source paths: `proposal_id` for the `proposal`/`vote` path (retrievable via the persisted Layer 2 `GovernanceDecisionReceipt` and the proposal-scoped endpoints `GET /v1/gov/proposals/{proposal_id}/proof` and `/chain`) and `item_id` for the `action_item`/`complete` path (retrievable via the persisted Layer 2 `ActionItemCompletionReceipt` keyed by `item_id`). Full holder-oriented receipt query — including a generic `GET /me/receipts` index — remains future work in Layer 4.
- **Cross-coop verifiable.** A federated peer can, in principle, verify a `FederationProvenance` chain. The query surface (Layer 4) is what is missing for "in practice."

## Consequences

- A new piece of provenance work knows where to land. New layers extend the envelope; they do not invent it.
- Audit endpoints (issue [#1438](https://github.com/InterCooperative-Network/icn/issues/1438)) plug into Layer 4 once ADR-0072 is drafted and accepted.
- The HMAC-bound fingerprint vs publicly-verifiable signature question (issue [#1435](https://github.com/InterCooperative-Network/icn/issues/1435)) is a Layer 1 concern; resolution amends or supersedes this ADR rather than fragmenting the envelope.
- Trade-off: amendment requires care. Adding a layer-fields is forward-compatible; renaming or removing a field is a supersession event.

## Implementation status

Partial.

| Layer | Status | Evidence |
|---|---|---|
| 1 — GovernanceProof | implemented | [icn/crates/icn-governance/src/proof.rs](../../icn/crates/icn-governance/src/proof.rs) |
| 2 — ArtifactReceipt | implemented | Gateway receipt store; ADR-0020 chain. End-to-end action-card linkage verified for `proposal`/`vote` (`GovernanceDecisionReceipt` keyed by `proposal_id`) at `icn/apps/governance/tests/me_action_card_receipt_chain.rs` and for `action_item`/`complete` (`ActionItemCompletionReceipt` keyed by `item_id`) at `icn/apps/governance/tests/me_action_item_receipt_chain.rs`. The `ActionItemCompletionReceipt` envelope (`icn/crates/icn-governance/src/proof.rs`) carries `item_id`, `domain_id`, `actor_did`, a closed `transition` taxonomy (`Completed` only today), `completed_at`, and a canonical `record_hash` under domain-tag `icn:gov:action_item_completion:v1`. |
| 3 — FederationProvenance | implemented (persistent) | PR [#1648](https://github.com/InterCooperative-Network/icn/pull/1648) |
| 4 — ProvenanceQuery | NOT IMPLEMENTED | Issue [#1438](https://github.com/InterCooperative-Network/icn/issues/1438); ADR-0072 in registry |

Open hardening items captured against this envelope:
- [#1435](https://github.com/InterCooperative-Network/icn/issues/1435) — replace JWT-secret-bound HMAC fingerprint with publicly verifiable per-entry signing.
- [#1436](https://github.com/InterCooperative-Network/icn/issues/1436) — disambiguate `ProvenanceRef::DirectMember.signature` field semantics.
- [#1438](https://github.com/InterCooperative-Network/icn/issues/1438) — federation/auditor-accessible provenance query endpoint.

To call this envelope `implemented` overall would require Layer 4 plus the resolution of #1435/#1436.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Keep the layers as separate ADRs without an enclosing envelope | What we had through #1648. Each ADR is correct in its own scope but the cross-cutting shape was not visible to reviewers, making cross-layer drift easy. |
| Use a single flat record per decision (no layering) | Conflates governance proof, institutional receipt, and federation provenance. Each has different lifetimes and different consumers. |
| Defer this ADR until Layer 4 is implemented | Layer 4 wants a written envelope to plug into. Drafting now is the cheaper move. |
