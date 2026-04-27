---
id: "0027"
title: "Action Card Contract"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["action-cards", "member-shell", "standing", "derived-views", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partially implemented (vertical slice; proof-loop verified for proposal/vote source path)"
references:
  - "ADR-0020 (Bootstrap Activation and Standing Read Model)"
  - "ADR-0025 (Institutional Effect Record Canonical Schema)"
  - "ADR-0028 (Accessibility Baseline for Member Interfaces)"
  - "GitHub #1608 (GET /me/action-cards)"
  - "GitHub #1646 (action card implementation)"
---

# ADR 0027: Action Card Contract

## Status

**Proposed (2026-04-26).** Names what action cards will be. ADR-0020 promised the surface; this ADR records the contract so issue [#1608](https://github.com/InterCooperative-Network/icn/issues/1608) plugs into a written shape rather than re-deriving one.

## Context

Action cards are the proposed member-facing nervous system. The intent: a member opens their shell and sees a prioritized list of "things you need to do" — proposals to deliberate, action items assigned, signals requiring attention, allocations awaiting approval, charters awaiting signature. Today this surface does not exist. Issue [#1608](https://github.com/InterCooperative-Network/icn/issues/1608) is the implementation issue.

Without a written contract, action-card implementation will be tempted to:

- Treat cards as stored entities (creating a card table, mutation API, lifecycle).
- Bake institution-specific UX into the gateway (NYCN-style cards, coop-X-style cards).
- Hard-code prioritization rules (deadlines first, severity second, …) instead of letting policy decide.

This ADR forecloses those failure modes by writing the contract.

## Decision (proposed)

Action cards are **derived views**, not stored entities. The contract:

### Endpoint

`GET /v1/gov/me/action-cards`

Returns a prioritized list of cards for the calling DID. No mutation API. Cards reference underlying objects; mutation flows through those.

### Card shape

Each card carries:

1. **Identity.** `card_id` (deterministic hash of derivation inputs), `card_kind` (closed taxonomy).
2. **Subject.** `entity_id`, optional `structure_id`, optional `holder_role`. Where this card lives in the holder's standing.
3. **Reference.** Pointer to the underlying object (proposal id, action item id, allocation request id, signal id).
4. **Plain-language summary.** The "what does this mean to me" one-line string.
5. **Suggested action.** The "what do I do next" hint (open proposal, sign charter, attend deliberation).
6. **Deadline (optional).** ISO-8601 timestamp; absent means no time pressure encoded by the card.
7. **Priority hint.** Numeric score from a policy oracle; UI may sort but should not hide cards.
8. **Accessibility metadata.** Locale, assistive notes, deadline-justice profile (per ADR-0028).

### Card kind taxonomy (closed; growable by ADR amendment)

- `ProposalDeliberation`, `ProposalVote`
- `ActionItemAssigned`, `ActionItemDue`
- `AllocationApproval`, `AllocationDecline`
- `CharterSignature`, `CharterRatification`
- `RoleAcceptance`, `RoleRevocation`
- `SignalAttention` (requires ADR-0029 conflict object model and the future signals ADR)
- `MembershipApproval`, `MembershipDeparture`

Unknown card kinds are not emitted.

### Derivation rules

Cards are computed at request time from:

- The holder's standing (memberships, roles, authority scope) — ADR-0020.
- Pending governance objects (proposals, votes, action items) in entities the holder is a member of.
- Effect records affecting the holder (ADR-0025) — for "FYI" cards.
- Signal rules (future ADR) — for attention triggers.

Derivation is **stateless from the gateway's perspective.** A card is what falls out of the current state. Two requests at the same moment from the same DID return identical cards.

### What action cards are not

- Not a notification system (notifications are a separate surface; ADR-0052 candidate).
- Not a task tracker (tasks are action items, owned by governance).
- Not a stored entity (no card lifecycle, no mutation, no archival).
- Not institution-specific (institution packages contribute card *templates* and rendering hints, not card kinds).

## Consequences

- Action cards as derived views means the surface stays composable: any new institutional concept that produces a card kind plugs in without a schema change.
- Statelessness means cards never get out of sync with reality. The cost: every request re-derives.
- Holder-side rendering (mobile shell, web member portal) is free to design UX without coordinating on backend state shape.
- Trade-off: prioritization is centralized (a policy oracle assigns scores). Priority disputes are policy disputes, not state disputes.
- Trade-off: adding a card kind requires ADR amendment. This is the right trade — silent expansion of the surface is exactly what member-experience reviewers should not accept.

## Implementation status

Partially implemented (vertical slice; icn#1646). The handler `GET /v1/gov/me/action-cards` is live; cards derive from `/me/standing` + open governance state for the `proposal`/`vote`, `meeting`/`attend`, and `action_item`/`complete` source paths. The full proof loop `standing → action card → authorized action → receipt` is verified end-to-end for the `proposal`/`vote` source path only — see `icn/apps/governance/tests/me_action_card_receipt_chain.rs`, which pins that the action card's `source_id` equals the persisted `GovernanceDecisionReceipt.proposal_id` (ADR-0026 Layer 2). Issue [#1608](https://github.com/InterCooperative-Network/icn/issues/1608) and the implementation issue [#1646](https://github.com/InterCooperative-Network/icn/issues/1646) remain open.

To call this `implemented` would require:
- `GET /me/action-cards` handler in `icn-gateway`.
- Card-kind enum and derivation logic in `icn-governance`.
- Priority policy oracle (a `PolicyOracle` impl returning a numeric score).
- Test coverage proving derivation is deterministic and stateless.
- Accessibility metadata wiring (ADR-0028 dependency).

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Cards as stored entities with a lifecycle | Re-introduces a mutable surface that can drift from underlying state. The whole point of cards is to be a deterministic view. |
| Per-institution card kinds | Couples the substrate to institution vocabulary. Institution packages contribute card templates and rendering hints, not new kinds. |
| Hard-coded prioritization rules | Priority is a policy concern. The kernel/oracle separation says policy belongs in oracles. |
| Notification system instead of cards | Notifications optimize for capturing attention; cards optimize for surfacing the right work at the right time. Different design problems; both may exist; this ADR is about cards. |
